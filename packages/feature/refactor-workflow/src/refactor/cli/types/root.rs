use cap_std::fs::Dir;
use paredit_core_workspace::fs_identity::FilesystemIdentity;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
pub struct RefactorRootReport {
    pub enforced: bool,
    pub path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct RefactorRootGuard {
    pub root: PathBuf,
    pub canonical_root: PathBuf,
    pub root_dir: Arc<Dir>,
    pub root_identity: FilesystemIdentity,
}
