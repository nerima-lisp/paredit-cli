//! Workspace filesystem discovery adapters.

pub mod archive;
pub mod cache;
mod discovery;
pub mod error;
mod filters;
pub mod glob;
pub mod ignore;
pub mod input;
pub mod manifest;
mod types;
pub mod vcs;

pub use archive::{ArchiveRefusal, ExtractedArchive, extract_tar, extract_tar_path};
pub use cache::{CacheOutcome, CachedDiscovery, DiscoveryCache, skip_counter_names};
pub use discovery::rehydrate_cached_discovery;
pub use discovery::{discover_workspace_files, discover_workspace_files_from_list};
pub use error::{WorkspaceError, WorkspaceLimit, WorkspaceRefusal, WorkspaceResult};
pub use glob::{GlobDecision, GlobParseError, GlobPattern, GlobSet};
pub use ignore::{
    GITIGNORE_FILE_NAME, IgnoreCache, IgnoreLayer, IgnoreMatch, IgnoreOptions, IgnoreScope,
    IgnoreSource, IgnoreStack, PAREDITIGNORE_FILE_NAME,
};
pub use input::{PathListSeparator, parse_path_list, read_path_list, read_path_list_from_stdin};
pub use manifest::{
    ManifestDependency, ManifestKind, ManifestSource, ManifestSourcePath, SourcePathRole,
    elisp_package_manifests_in_directory, manifests_in_directory, parse_elisp_package,
    parse_manifest, parse_manifest_as,
};
pub use types::{
    DirectoryFingerprint, SymlinkPolicy, WorkspaceDiscovery, WorkspaceDiscoveryOptions,
};
pub use vcs::{
    GitRefusal, RepositoryRoot, SinceOptions, changed_paths_since, find_repository_root,
    git_directory, git_info_exclude_files, is_repository_root, tracked_paths,
};

#[cfg(test)]
mod tests;
