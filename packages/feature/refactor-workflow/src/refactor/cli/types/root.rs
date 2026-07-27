use super::super::super::*;
use crate::infrastructure::fs_identity::FilesystemIdentity;
use cap_std::fs::Dir;
use std::sync::Arc;

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorRootReport {
    pub(in crate::presentation::cli) enforced: bool,
    pub(in crate::presentation::cli) path: Option<PathBuf>,
}

#[derive(Debug)]
pub(in crate::presentation::cli) struct RefactorRootGuard {
    pub(in crate::presentation::cli) root: PathBuf,
    pub(in crate::presentation::cli) canonical_root: PathBuf,
    pub(in crate::presentation::cli) root_dir: Arc<Dir>,
    pub(in crate::presentation::cli) root_identity: FilesystemIdentity,
}
