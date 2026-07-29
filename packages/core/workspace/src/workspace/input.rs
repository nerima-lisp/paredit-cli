//! Reading an explicit file list instead of walking a tree.
//!
//! `git ls-files '*.lisp' | paredit inspect lint --paths-from -` is the shape
//! this exists for. Every selection rule this tool implements — ignore files,
//! globs, `--since` — is an approximation of a decision the caller may already
//! have made with tools of their own, and the honest answer to that is a way to
//! hand the answer over rather than another flag to approximate it better.
//!
//! The list is read as bytes and split on a separator, never parsed as words.
//! A path containing a space is ordinary, a path containing a newline is legal
//! on every unix filesystem, and the NUL separator exists so that both survive.

use std::io::Read;
use std::path::PathBuf;

use super::error::{WorkspaceError, WorkspaceLimit};

/// The most paths a list will yield.
const MAX_LIST_PATHS: usize = 200_000;

/// The most bytes a list may occupy.
const MAX_LIST_BYTES: usize = 64 * 1024 * 1024;

/// How entries in a path list are separated.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PathListSeparator {
    /// Newline-separated, the shape of `find`, `ls` and most editors.
    ///
    /// Leading and trailing whitespace is trimmed and blank lines are skipped,
    /// because a hand-written or shell-produced list has both.
    #[default]
    Newline,
    /// NUL-separated, the shape of `git ls-files -z` and `find -print0`.
    ///
    /// Nothing is trimmed: a NUL-separated list is machine output, and a
    /// filename really can start or end with a space.
    Nul,
}

impl PathListSeparator {
    /// A stable identifier for JSON output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Newline => "newline",
            Self::Nul => "nul",
        }
    }
}

/// Splits a path list into paths.
///
/// Rejects a list that is too large rather than truncating it: a truncated
/// input set produces a clean report over the wrong files, which is the one
/// outcome worse than an error.
pub fn parse_path_list(
    bytes: &[u8],
    separator: PathListSeparator,
) -> Result<Vec<PathBuf>, WorkspaceError> {
    if bytes.len() > MAX_LIST_BYTES {
        return Err(WorkspaceLimit::TotalBytes {
            actual: bytes.len() as u64,
            maximum: MAX_LIST_BYTES as u64,
        }
        .into());
    }

    let delimiter = match separator {
        PathListSeparator::Newline => b'\n',
        PathListSeparator::Nul => 0,
    };

    let mut paths = Vec::new();
    for entry in bytes.split(|byte| *byte == delimiter) {
        let entry = match separator {
            PathListSeparator::Newline => trim_ascii_whitespace(entry),
            PathListSeparator::Nul => entry,
        };
        if entry.is_empty() {
            continue;
        }
        if paths.len() >= MAX_LIST_PATHS {
            return Err(WorkspaceLimit::Files {
                maximum: MAX_LIST_PATHS,
            }
            .into());
        }
        paths.push(decode_path(entry)?);
    }
    Ok(paths)
}

/// Reads a path list from `reader`, bounded by the same limits as a file.
pub fn read_path_list<R: Read>(
    mut reader: R,
    separator: PathListSeparator,
) -> Result<Vec<PathBuf>, WorkspaceError> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut chunk)
            .map_err(|source| WorkspaceError::Io {
                context: "failed to read the path list".to_owned(),
                source,
            })?;
        if count == 0 {
            break;
        }
        if buffer.len() + count > MAX_LIST_BYTES {
            return Err(WorkspaceLimit::TotalBytes {
                actual: (buffer.len() + count) as u64,
                maximum: MAX_LIST_BYTES as u64,
            }
            .into());
        }
        buffer
            .try_reserve(count)
            .map_err(|_| WorkspaceError::Unavailable("failed to allocate the path list buffer"))?;
        buffer.extend_from_slice(&chunk[..count]);
    }
    parse_path_list(&buffer, separator)
}

/// Reads a path list from standard input.
pub fn read_path_list_from_stdin(
    separator: PathListSeparator,
) -> Result<Vec<PathBuf>, WorkspaceError> {
    read_path_list(std::io::stdin().lock(), separator)
}

fn trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[start..end]
}

#[cfg(unix)]
fn decode_path(bytes: &[u8]) -> Result<PathBuf, WorkspaceError> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    Ok(PathBuf::from(OsStr::from_bytes(bytes)))
}

#[cfg(not(unix))]
fn decode_path(bytes: &[u8]) -> Result<PathBuf, WorkspaceError> {
    std::str::from_utf8(bytes)
        .map(PathBuf::from)
        .map_err(|_| WorkspaceError::Unavailable("the path list contains a non-UTF-8 path"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{PathListSeparator, parse_path_list};

    #[test]
    fn newline_lists_are_trimmed_and_blank_lines_skipped() {
        let paths = parse_path_list(
            b"  src/a.lisp \n\n\tsrc/b.lisp\n",
            PathListSeparator::Newline,
        )
        .expect("list parses");
        assert_eq!(
            paths,
            vec![PathBuf::from("src/a.lisp"), PathBuf::from("src/b.lisp")]
        );
    }

    #[test]
    fn nul_lists_preserve_every_byte_of_a_name() {
        let paths = parse_path_list(
            b"src/with space.lisp\0src/tab\t.lisp\0",
            PathListSeparator::Nul,
        )
        .expect("list parses");
        assert_eq!(
            paths,
            vec![
                PathBuf::from("src/with space.lisp"),
                PathBuf::from("src/tab\t.lisp"),
            ]
        );
    }

    #[test]
    fn a_newline_inside_a_name_survives_the_nul_separator() {
        let paths =
            parse_path_list(b"src/two\nlines.lisp\0", PathListSeparator::Nul).expect("list parses");
        assert_eq!(paths, vec![PathBuf::from("src/two\nlines.lisp")]);
    }

    #[test]
    fn a_crlf_list_does_not_leak_a_carriage_return() {
        let paths = parse_path_list(b"src/a.lisp\r\nsrc/b.lisp\r\n", PathListSeparator::Newline)
            .expect("list parses");
        assert_eq!(
            paths,
            vec![PathBuf::from("src/a.lisp"), PathBuf::from("src/b.lisp")]
        );
    }
}
