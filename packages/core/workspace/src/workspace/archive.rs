//! Reading a source tree out of a tar archive.
//!
//! The obvious reading of "analyse a tarball or a git URL" is an HTTP client, a
//! TLS stack, a URL parser and a decompressor inside a tool whose entire job is
//! to be trusted with somebody's source tree. That is a large attack surface
//! bought for a small convenience, and every one of those pieces already exists
//! on the machine:
//!
//! ```sh
//! curl -sSL https://example.invalid/project.tar.gz | gzip -dc \
//!   | paredit inspect sources --from-archive - --extract-to /tmp/project /tmp/project
//! ```
//!
//! So this module reads *uncompressed tar*, from a file or from standard input,
//! and nothing else. Compression and transport are the shell's, which also
//! means `.tar.gz`, `.tar.zst`, `git archive`, and an S3 URL all work without a
//! line of code here.
//!
//! Extraction is where tar archives have historically gone wrong, and every one
//! of those failures is refused rather than sanitised:
//!
//! * an absolute path, or any `..` component, is refused — not stripped,
//!   because a stripped path is a path the archive did not name;
//! * symlinks, hardlinks, devices, FIFOs and sockets are skipped, so nothing
//!   the archive contains can redirect a later write outside the destination;
//! * entry count and total bytes are bounded, so a "tar bomb" fails instead of
//!   filling the disk;
//! * an existing file is never overwritten.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use super::error::{WorkspaceError, WorkspaceLimit, WorkspaceRefusal};

/// One tar block.
const BLOCK_BYTES: usize = 512;

/// The most entries an archive may contain.
const MAX_ENTRIES: usize = 200_000;

/// The most bytes an archive may expand to.
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The most bytes one entry may expand to.
const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;

/// The longest path a GNU long-name entry may carry.
const MAX_LONG_NAME_BYTES: u64 = 64 * 1024;

/// What an extraction produced.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExtractedArchive {
    /// The regular files written, in archive order.
    pub files: Vec<PathBuf>,
    /// Directories created.
    pub directories: Vec<PathBuf>,
    /// Entries skipped because they were not a regular file or directory.
    ///
    /// Reported rather than silently dropped: a source tarball full of symlinks
    /// analysed as if it had none is a different tree from the one the archive
    /// describes, and the user should know.
    pub skipped_special_count: usize,
    /// Total bytes written.
    pub written_bytes: u64,
}

/// Why an archive was refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchiveRefusal {
    /// A header did not look like tar.
    MalformedHeader { entry: usize },
    /// A header's checksum did not match its contents.
    ChecksumMismatch { entry: usize },
    /// A size or numeric field was not readable.
    MalformedField { entry: usize, field: &'static str },
    /// An entry named an absolute path.
    AbsolutePath { name: String },
    /// An entry escaped the destination with `..`.
    EscapingPath { name: String },
    /// An entry name was not UTF-8.
    NonUtf8Name { entry: usize },
    /// The destination already holds a file the archive wants to write.
    DestinationOccupied { path: PathBuf },
}

impl std::fmt::Display for ArchiveRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedHeader { entry } => {
                write!(
                    formatter,
                    "archive entry {entry} has a malformed tar header"
                )
            }
            Self::ChecksumMismatch { entry } => write!(
                formatter,
                "archive entry {entry} has a tar header checksum that does not match"
            ),
            Self::MalformedField { entry, field } => write!(
                formatter,
                "archive entry {entry} has an unreadable {field} field"
            ),
            Self::AbsolutePath { name } => write!(
                formatter,
                "refusing archive entry with an absolute path: {name}"
            ),
            Self::EscapingPath { name } => write!(
                formatter,
                "refusing archive entry that escapes the destination: {name}"
            ),
            Self::NonUtf8Name { entry } => {
                write!(formatter, "archive entry {entry} has a non-UTF-8 name")
            }
            Self::DestinationOccupied { path } => write!(
                formatter,
                "refusing to overwrite an existing file while extracting: {}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ArchiveRefusal {}

impl From<ArchiveRefusal> for WorkspaceError {
    fn from(refusal: ArchiveRefusal) -> Self {
        Self::Io {
            context: refusal.to_string(),
            source: std::io::Error::other(refusal.to_string()),
        }
    }
}

/// Extracts an uncompressed tar stream into `destination`.
///
/// `destination` is created if absent and must not already contain any file the
/// archive names. Returns what was written, so the caller can scan exactly
/// those paths rather than whatever else happens to be there.
pub fn extract_tar<R: Read>(
    mut reader: R,
    destination: &Path,
) -> Result<ExtractedArchive, WorkspaceError> {
    fs::create_dir_all(destination).map_err(|source| WorkspaceError::Io {
        context: format!("failed to create {}", destination.display()),
        source,
    })?;
    let canonical_destination =
        fs::canonicalize(destination).map_err(|source| WorkspaceError::Io {
            context: format!("failed to resolve {}", destination.display()),
            source,
        })?;

    let mut extracted = ExtractedArchive::default();
    let mut block = [0_u8; BLOCK_BYTES];
    let mut entry_index = 0;
    let mut pending_long_name: Option<String> = None;
    let mut zero_blocks = 0;

    loop {
        if !read_exact_or_eof(&mut reader, &mut block)? {
            break;
        }
        if block.iter().all(|byte| *byte == 0) {
            zero_blocks += 1;
            // Two consecutive zero blocks terminate a tar stream. Anything
            // after them is padding a writer added to reach a block boundary.
            if zero_blocks == 2 {
                break;
            }
            continue;
        }
        zero_blocks = 0;
        entry_index += 1;
        if entry_index > MAX_ENTRIES {
            return Err(WorkspaceLimit::Files {
                maximum: MAX_ENTRIES,
            }
            .into());
        }

        verify_checksum(&block, entry_index)?;
        let size = octal_field(&block[124..136], entry_index, "size")?;
        let type_flag = block[156];

        // GNU long names put the real path in the *next* entry's data.
        if type_flag == b'L' {
            if size > MAX_LONG_NAME_BYTES {
                return Err(WorkspaceLimit::ReadSize {
                    path: PathBuf::from("<archive long name>"),
                    maximum: MAX_LONG_NAME_BYTES,
                }
                .into());
            }
            let data = read_entry_data(&mut reader, size)?;
            let name =
                String::from_utf8(data.split(|byte| *byte == 0).next().unwrap_or(&[]).to_vec())
                    .map_err(|_| ArchiveRefusal::NonUtf8Name { entry: entry_index })?;
            pending_long_name = Some(name);
            continue;
        }
        // Pax and GNU long-link records carry metadata this reader does not
        // use; their data still has to be consumed to stay block-aligned.
        if matches!(type_flag, b'K' | b'x' | b'g') {
            read_entry_data(&mut reader, size)?;
            continue;
        }

        let name = match pending_long_name.take() {
            Some(name) => name,
            None => header_name(&block, entry_index)?,
        };
        let relative = safe_relative_path(&name)?;

        match type_flag {
            b'5' => {
                let target = canonical_destination.join(&relative);
                fs::create_dir_all(&target).map_err(|source| WorkspaceError::Io {
                    context: format!("failed to create {}", target.display()),
                    source,
                })?;
                extracted.directories.push(target);
            }
            b'0' | b'\0' | b'7' => {
                if size > MAX_ENTRY_BYTES {
                    return Err(WorkspaceLimit::ReadSize {
                        path: relative.clone(),
                        maximum: MAX_ENTRY_BYTES,
                    }
                    .into());
                }
                let total = extracted.written_bytes.saturating_add(size);
                if total > MAX_TOTAL_BYTES {
                    return Err(WorkspaceLimit::TotalBytes {
                        actual: total,
                        maximum: MAX_TOTAL_BYTES,
                    }
                    .into());
                }
                let data = read_entry_data(&mut reader, size)?;
                let target = canonical_destination.join(&relative);
                write_regular_file(&target, &data)?;
                extracted.written_bytes = total;
                extracted.files.push(target);
            }
            // '1' hardlink, '2' symlink, '3'/'4' devices, '6' FIFO. None of
            // them is a source file, and each is a way for an archive to point
            // a later write somewhere it was not allowed to go.
            _ => {
                read_entry_data(&mut reader, size)?;
                extracted.skipped_special_count += 1;
            }
        }
    }

    Ok(extracted)
}

/// Extracts from a file, or from standard input when `archive` is `-`.
pub fn extract_tar_path(
    archive: &Path,
    destination: &Path,
) -> Result<ExtractedArchive, WorkspaceError> {
    if archive == Path::new("-") {
        return extract_tar(std::io::stdin().lock(), destination);
    }
    let file = fs::File::open(archive).map_err(|source| WorkspaceError::Io {
        context: format!("failed to open {}", archive.display()),
        source,
    })?;
    extract_tar(file, destination)
}

fn write_regular_file(target: &Path, data: &[u8]) -> Result<(), WorkspaceError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| WorkspaceError::Io {
            context: format!("failed to create {}", parent.display()),
            source,
        })?;
    }
    // `symlink_metadata`, not `metadata`: a symlink already sitting at the
    // target would otherwise report the type of whatever it points at, and the
    // write would follow it out of the destination.
    if let Ok(existing) = fs::symlink_metadata(target) {
        if existing.is_dir() {
            return Err(WorkspaceRefusal::NonRegularFile {
                path: target.to_path_buf(),
            }
            .into());
        }
        return Err(ArchiveRefusal::DestinationOccupied {
            path: target.to_path_buf(),
        }
        .into());
    }
    fs::write(target, data).map_err(|source| WorkspaceError::Io {
        context: format!("failed to write {}", target.display()),
        source,
    })
}

/// Validates an archive entry name and returns it as a relative path.
///
/// Refuses rather than sanitises. A `../../etc/passwd` rewritten to
/// `etc/passwd` is a file the archive did not describe, written to a place the
/// user did not ask for; the only honest answer is to stop.
fn safe_relative_path(name: &str) -> Result<PathBuf, WorkspaceError> {
    let trimmed = name.trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(ArchiveRefusal::EscapingPath {
            name: name.to_owned(),
        }
        .into());
    }
    let path = Path::new(trimmed);
    if path.is_absolute() || trimmed.starts_with('/') {
        return Err(ArchiveRefusal::AbsolutePath {
            name: name.to_owned(),
        }
        .into());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ArchiveRefusal::EscapingPath {
                    name: name.to_owned(),
                }
                .into());
            }
        }
    }
    Ok(path.components().collect())
}

fn header_name(block: &[u8; BLOCK_BYTES], entry: usize) -> Result<String, WorkspaceError> {
    let name = nul_terminated(&block[0..100]);
    let prefix = nul_terminated(&block[345..500]);
    let name = std::str::from_utf8(name).map_err(|_| ArchiveRefusal::NonUtf8Name { entry })?;
    let prefix = std::str::from_utf8(prefix).map_err(|_| ArchiveRefusal::NonUtf8Name { entry })?;
    Ok(if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}/{name}")
    })
}

fn nul_terminated(field: &[u8]) -> &[u8] {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    &field[..end]
}

fn octal_field(field: &[u8], entry: usize, name: &'static str) -> Result<u64, WorkspaceError> {
    let text = nul_terminated(field);
    let text = std::str::from_utf8(text)
        .map_err(|_| ArchiveRefusal::MalformedField { entry, field: name })?
        .trim();
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8)
        .map_err(|_| ArchiveRefusal::MalformedField { entry, field: name }.into())
}

/// Verifies the header checksum, accepting both the signed and unsigned sums.
///
/// Historic tar implementations disagreed on whether the header bytes are
/// signed, and archives written by either are still in circulation. Accepting
/// both is what every reader does; rejecting one would refuse valid archives
/// while catching nothing extra.
fn verify_checksum(block: &[u8; BLOCK_BYTES], entry: usize) -> Result<(), WorkspaceError> {
    let recorded = octal_field(&block[148..156], entry, "checksum")?;
    let mut unsigned: u64 = 0;
    let mut signed: i64 = 0;
    for (index, byte) in block.iter().enumerate() {
        let value = if (148..156).contains(&index) {
            b' '
        } else {
            *byte
        };
        unsigned += u64::from(value);
        signed += i64::from(value as i8);
    }
    if recorded == unsigned || i64::try_from(recorded) == Ok(signed) {
        return Ok(());
    }
    Err(ArchiveRefusal::ChecksumMismatch { entry }.into())
}

fn read_entry_data<R: Read>(reader: &mut R, size: u64) -> Result<Vec<u8>, WorkspaceError> {
    let padded = size.div_ceil(BLOCK_BYTES as u64) * BLOCK_BYTES as u64;
    let mut data = Vec::new();
    let mut remaining = padded;
    let mut block = [0_u8; BLOCK_BYTES];
    while remaining > 0 {
        if !read_exact_or_eof(reader, &mut block)? {
            return Err(ArchiveRefusal::MalformedHeader { entry: 0 }.into());
        }
        let wanted = usize::try_from(size.saturating_sub(data.len() as u64))
            .unwrap_or(BLOCK_BYTES)
            .min(BLOCK_BYTES);
        data.try_reserve(wanted)
            .map_err(|_| WorkspaceError::Unavailable("failed to allocate an archive entry"))?;
        data.extend_from_slice(&block[..wanted]);
        remaining -= BLOCK_BYTES as u64;
    }
    Ok(data)
}

/// Fills `block`, returning `false` at a clean end of stream.
fn read_exact_or_eof<R: Read>(
    reader: &mut R,
    block: &mut [u8; BLOCK_BYTES],
) -> Result<bool, WorkspaceError> {
    let mut filled = 0;
    while filled < BLOCK_BYTES {
        let count = reader
            .read(&mut block[filled..])
            .map_err(|source| WorkspaceError::Io {
                context: "failed to read the archive".to_owned(),
                source,
            })?;
        if count == 0 {
            if filled == 0 {
                return Ok(false);
            }
            // A truncated final block is a damaged archive, not an end.
            return Err(ArchiveRefusal::MalformedHeader { entry: 0 }.into());
        }
        filled += count;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{BLOCK_BYTES, extract_tar, safe_relative_path};

    /// Builds one tar entry: a header block plus padded data.
    fn tar_entry(name: &str, type_flag: u8, data: &[u8]) -> Vec<u8> {
        let mut header = [0_u8; BLOCK_BYTES];
        header[..name.len()].copy_from_slice(name.as_bytes());
        let mode = b"0000644\0";
        header[100..108].copy_from_slice(mode);
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        let size = format!("{:011o}\0", data.len());
        header[124..136].copy_from_slice(size.as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        header[148..156].copy_from_slice(b"        ");
        header[156] = type_flag;
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");

        let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
        let rendered = format!("{checksum:06o}\0 ");
        header[148..156].copy_from_slice(rendered.as_bytes());

        let mut entry = header.to_vec();
        entry.extend_from_slice(data);
        let padding = (BLOCK_BYTES - data.len() % BLOCK_BYTES) % BLOCK_BYTES;
        entry.extend(std::iter::repeat_n(0_u8, padding));
        entry
    }

    fn tar(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut archive = entries.concat();
        archive.extend(std::iter::repeat_n(0_u8, BLOCK_BYTES * 2));
        archive
    }

    fn temporary_directory(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time is after the epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "paredit-archive-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn a_relative_entry_name_is_accepted_and_an_escaping_one_is_not() {
        assert_eq!(
            safe_relative_path("src/core.lisp").expect("relative path"),
            PathBuf::from("src/core.lisp")
        );
        assert!(safe_relative_path("/etc/passwd").is_err());
        assert!(safe_relative_path("../outside.lisp").is_err());
        assert!(safe_relative_path("src/../../outside.lisp").is_err());
        assert!(safe_relative_path("").is_err());
    }

    #[test]
    fn regular_files_and_directories_are_written_and_specials_are_skipped() {
        let destination = temporary_directory("extract");
        let archive = tar(&[
            tar_entry("project/", b'5', b""),
            tar_entry("project/core.lisp", b'0', b"(defun core () nil)\n"),
            tar_entry("project/link.lisp", b'2', b""),
        ]);

        let extracted = extract_tar(archive.as_slice(), &destination).expect("archive extracts");
        assert_eq!(extracted.files.len(), 1);
        assert_eq!(extracted.skipped_special_count, 1);
        assert_eq!(
            std::fs::read_to_string(&extracted.files[0]).expect("read extracted file"),
            "(defun core () nil)\n"
        );

        let _ = std::fs::remove_dir_all(&destination);
    }

    #[test]
    fn an_entry_that_escapes_the_destination_stops_the_extraction() {
        let destination = temporary_directory("escape");
        let archive = tar(&[tar_entry("../escaped.lisp", b'0', b"(defun x () nil)\n")]);

        let error = extract_tar(archive.as_slice(), &destination)
            .expect_err("an escaping entry is refused");
        assert!(
            error.to_string().contains("escapes the destination"),
            "unexpected message: {error}"
        );
        assert!(!destination.join("..").join("escaped.lisp").exists());

        let _ = std::fs::remove_dir_all(&destination);
    }

    #[test]
    fn a_corrupted_header_is_refused_rather_than_read() {
        let destination = temporary_directory("checksum");
        let mut archive = tar(&[tar_entry("a.lisp", b'0', b"(defun a () nil)\n")]);
        // Flip a byte in the name, which the recorded checksum no longer covers.
        archive[0] = b'z';

        let error = extract_tar(archive.as_slice(), &destination)
            .expect_err("a corrupted header is refused");
        assert!(
            error.to_string().contains("checksum"),
            "unexpected message: {error}"
        );

        let _ = std::fs::remove_dir_all(&destination);
    }

    #[test]
    fn an_existing_file_is_never_overwritten() {
        let destination = temporary_directory("occupied");
        std::fs::create_dir_all(&destination).expect("create destination");
        std::fs::write(destination.join("a.lisp"), "(defun original () nil)\n")
            .expect("write existing file");
        let archive = tar(&[tar_entry("a.lisp", b'0', b"(defun replacement () nil)\n")]);

        let error = extract_tar(archive.as_slice(), &destination)
            .expect_err("an occupied destination is refused");
        assert!(
            error.to_string().contains("refusing to overwrite"),
            "unexpected message: {error}"
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("a.lisp")).expect("read"),
            "(defun original () nil)\n"
        );

        let _ = std::fs::remove_dir_all(&destination);
    }
}
