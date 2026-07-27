use super::super::super::*;
use super::super::types::root::{RefactorRootGuard, RefactorRootReport};
use crate::infrastructure::fs_identity::FilesystemIdentity;
use crate::presentation::cli::shared::{
    AnchoredExpectedWrite, ExpectedWriteTarget, read_text_file_with_expected_target,
};
use cap_std::ambient_authority;

pub(in crate::presentation::cli) const MAX_MANIFEST_SOURCE_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

impl RefactorRootGuard {
    pub(in crate::presentation::cli) fn new(root: &FsPath) -> Result<Self> {
        let canonical_root = fs::canonicalize(root)
            .with_context(|| format!("failed to canonicalize refactor root {}", root.display()))?;
        if !canonical_root.is_dir() {
            anyhow::bail!(
                "refactor root {} is not a directory",
                canonical_root.display()
            );
        }
        let root_dir = cap_std::fs::Dir::open_ambient_dir(&canonical_root, ambient_authority())
            .with_context(|| {
                format!(
                    "failed to open refactor root capability {}",
                    canonical_root.display()
                )
            })?;
        let ambient_metadata = fs::metadata(&canonical_root).with_context(|| {
            format!(
                "failed to inspect ambient refactor root {}",
                canonical_root.display()
            )
        })?;
        let capability_metadata = root_dir.dir_metadata().with_context(|| {
            format!(
                "failed to inspect refactor root capability {}",
                canonical_root.display()
            )
        })?;
        let root_identity = FilesystemIdentity::from_cap(&capability_metadata)
            .context("refactor root capability identity is unavailable")?;
        anyhow::ensure!(
            FilesystemIdentity::from_std(&ambient_metadata) == Some(root_identity),
            "refactor root changed while opening capability {}",
            canonical_root.display()
        );
        Ok(Self {
            root: root.to_path_buf(),
            canonical_root,
            root_dir: std::sync::Arc::new(root_dir),
            root_identity,
        })
    }

    fn validate_root_identity(&self) -> Result<()> {
        let ambient_metadata = fs::metadata(&self.canonical_root).with_context(|| {
            format!(
                "failed to inspect ambient refactor root {}",
                self.canonical_root.display()
            )
        })?;
        let capability_metadata = self.root_dir.dir_metadata().with_context(|| {
            format!(
                "failed to inspect refactor root capability {}",
                self.canonical_root.display()
            )
        })?;
        anyhow::ensure!(
            ambient_metadata.is_dir()
                && FilesystemIdentity::from_std(&ambient_metadata) == Some(self.root_identity)
                && FilesystemIdentity::from_cap(&capability_metadata) == Some(self.root_identity),
            "refactor root identity changed after capability open: {}",
            self.canonical_root.display()
        );
        Ok(())
    }

    pub(in crate::presentation::cli) fn resolve_manifest_path(
        &self,
        path: &FsPath,
    ) -> Result<PathBuf> {
        self.validate_root_identity()?;
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        let canonical_path = fs::canonicalize(&resolved)
            .with_context(|| format!("failed to canonicalize manifest path {}", path.display()))?;
        if !canonical_path.starts_with(&self.canonical_root) {
            anyhow::bail!(
                "manifest path {} is outside refactor root {}",
                path.display(),
                self.canonical_root.display()
            );
        }
        self.validate_root_identity()?;
        Ok(canonical_path)
    }

    pub(in crate::presentation::cli) fn read_manifest_source(
        &self,
        path: &FsPath,
    ) -> Result<(PathBuf, String, ExpectedWriteTarget)> {
        let canonical_path = self.resolve_manifest_path(path)?;
        let ambient_metadata = fs::symlink_metadata(&canonical_path)
            .with_context(|| format!("failed to inspect manifest source {}", path.display()))?;
        anyhow::ensure!(
            ambient_metadata.is_file() && !ambient_metadata.file_type().is_symlink(),
            "refusing non-regular manifest source {}",
            path.display()
        );
        self.validate_root_identity()?;
        let relative = canonical_path
            .strip_prefix(&self.canonical_root)
            .context("manifest path lost refactor-root confinement")?;
        let file = open_manifest_source(&self.root_dir, relative)
            .with_context(|| format!("failed to read manifest source {}", path.display()))?;
        let file = file.into_std();
        let metadata = file
            .metadata()
            .with_context(|| format!("failed to inspect manifest source {}", path.display()))?;
        anyhow::ensure!(
            metadata.file_type().is_file(),
            "refusing non-regular manifest source {}",
            path.display()
        );
        let opened_identity = FilesystemIdentity::from_std(&metadata)
            .context("manifest source identity is unavailable")?;
        anyhow::ensure!(
            FilesystemIdentity::from_std(&ambient_metadata) == Some(opened_identity),
            "manifest source identity differs between ambient path and root capability: {}",
            path.display()
        );
        let text = read_text_with_limit(file, MAX_SOURCE_INPUT_BYTES, &path.display().to_string())?;
        self.validate_root_identity()?;
        let current_ambient_metadata = fs::symlink_metadata(&canonical_path)
            .with_context(|| format!("failed to re-inspect manifest source {}", path.display()))?;
        anyhow::ensure!(
            current_ambient_metadata.is_file()
                && !current_ambient_metadata.file_type().is_symlink()
                && FilesystemIdentity::from_std(&current_ambient_metadata) == Some(opened_identity),
            "manifest source changed while reading: {}",
            path.display()
        );
        self.validate_root_identity()?;
        let expected = ExpectedWriteTarget::from_metadata_and_content(&metadata, &text)
            .context("manifest source identity is unavailable")?;
        Ok((canonical_path, text, expected))
    }

    pub(in crate::presentation::cli) fn anchored_manifest_write(
        &self,
        display_path: PathBuf,
        content: String,
        expected: ExpectedWriteTarget,
    ) -> Result<AnchoredExpectedWrite> {
        let relative = display_path
            .strip_prefix(&self.canonical_root)
            .with_context(|| {
                format!(
                    "manifest write target {} is outside refactor root {}",
                    display_path.display(),
                    self.canonical_root.display()
                )
            })?;
        let file_name = relative
            .file_name()
            .with_context(|| {
                format!(
                    "manifest write target {} has no file name",
                    display_path.display()
                )
            })?
            .to_os_string();
        let relative_parent = relative
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| FsPath::new("."));
        let parent_dir = self.root_dir.open_dir(relative_parent).with_context(|| {
            format!(
                "failed to retain manifest write parent for {}",
                display_path.display()
            )
        })?;

        Ok(AnchoredExpectedWrite {
            display_path,
            parent_dir: std::sync::Arc::new(parent_dir),
            file_name,
            content,
            expected,
        })
    }
}

fn open_manifest_source(
    root_dir: &cap_std::fs::Dir,
    relative: &FsPath,
) -> std::io::Result<cap_std::fs::File> {
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt as _;

        let mut options = cap_std::fs::OpenOptions::new();
        options
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC);
        root_dir.open_with(relative, &options)
    }
    #[cfg(not(unix))]
    {
        root_dir.open(relative)
    }
}

impl RefactorRootReport {
    pub(in crate::presentation::cli) fn from_guard(root_guard: Option<&RefactorRootGuard>) -> Self {
        match root_guard {
            Some(root_guard) => Self {
                enforced: true,
                path: Some(root_guard.canonical_root.clone()),
            },
            None => Self {
                enforced: false,
                path: None,
            },
        }
    }
}

pub(in crate::presentation::cli) fn read_refactor_manifest_source(
    path: &FsPath,
    root_guard: Option<&RefactorRootGuard>,
) -> Result<(PathBuf, String, ExpectedWriteTarget)> {
    match root_guard {
        Some(root_guard) => root_guard.read_manifest_source(path),
        None => {
            let (text, expected) =
                read_text_file_with_expected_target(path, MAX_SOURCE_INPUT_BYTES)?;
            Ok((path.to_path_buf(), text, expected))
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;

    fn unique_test_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "paredit-manifest-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after Unix epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn manifest_source_rejects_fifo_without_blocking() {
        let directory = unique_test_directory("fifo");
        fs::create_dir_all(&directory).expect("create test directory");
        let fifo = directory.join("blocked.lisp");
        let status = Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("run mkfifo");
        assert!(status.success(), "create FIFO");

        let guard = RefactorRootGuard::new(&directory).expect("create root guard");
        let error = guard
            .read_manifest_source(FsPath::new("blocked.lisp"))
            .expect_err("FIFO must be rejected");

        assert!(error.to_string().contains("non-regular manifest source"));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn manifest_source_rejects_root_replacement_after_guard_creation() {
        let directory = unique_test_directory("root-exchange");
        let displaced_directory = unique_test_directory("root-exchange-displaced");
        fs::create_dir_all(&directory).expect("create test directory");
        fs::write(directory.join("source.lisp"), b"(from-a)\n").expect("write A source");
        let guard = RefactorRootGuard::new(&directory).expect("create root guard");

        fs::rename(&directory, &displaced_directory).expect("displace A root");
        fs::create_dir_all(&directory).expect("create B root");
        fs::write(directory.join("source.lisp"), b"(from-b)\n").expect("write B source");

        let error = guard
            .read_manifest_source(FsPath::new("source.lisp"))
            .expect_err("replacing the guarded root must be rejected");
        assert!(
            format!("{error:#}").contains("refactor root identity changed"),
            "unexpected error: {error:#}"
        );
        assert_eq!(
            fs::read(directory.join("source.lisp")).expect("read B source"),
            b"(from-b)\n"
        );
        assert_eq!(
            fs::read(displaced_directory.join("source.lisp")).expect("read A source"),
            b"(from-a)\n"
        );

        fs::remove_dir_all(directory).expect("remove B root");
        fs::remove_dir_all(displaced_directory).expect("remove A root");
    }

    #[test]
    fn anchored_manifest_write_rejects_same_inode_after_root_replacement() {
        let directory = unique_test_directory("anchored-root-exchange");
        let displaced_directory = unique_test_directory("anchored-root-exchange-displaced");
        fs::create_dir_all(&directory).expect("create A root");
        let original_path = directory.join("source.lisp");
        fs::write(&original_path, b"(same)\n").expect("write A source");
        let guard = RefactorRootGuard::new(&directory).expect("create root guard");
        let (resolved_path, _, expected) = guard
            .read_manifest_source(FsPath::new("source.lisp"))
            .expect("read source through A root");

        fs::rename(&directory, &displaced_directory).expect("displace A root");
        fs::create_dir_all(&directory).expect("create B root");
        let displaced_source = displaced_directory.join("source.lisp");
        let replacement_source = directory.join("source.lisp");
        fs::hard_link(&displaced_source, &replacement_source)
            .expect("link parsed inode into B root");
        fs::remove_file(&displaced_source).expect("remove parsed path from A root");

        let write = guard
            .anchored_manifest_write(resolved_path, "(changed)\n".to_owned(), expected)
            .expect("retain A parent after root replacement");
        let error =
            crate::presentation::cli::shared::write_files_with_rollback_expected_anchored(vec![
                write,
            ])
            .expect_err("replaced ambient root must be rejected before writing");

        assert!(
            format!("{error:#}").contains("refusing replaced parent directory"),
            "unexpected error: {error:#}"
        );
        assert_eq!(
            fs::read(&replacement_source).expect("read B source"),
            b"(same)\n"
        );
        assert!(
            !displaced_source.exists(),
            "writer must not recreate the target in retained A root"
        );

        fs::remove_dir_all(directory).expect("remove B root");
        fs::remove_dir_all(displaced_directory).expect("remove A root");
    }
}
