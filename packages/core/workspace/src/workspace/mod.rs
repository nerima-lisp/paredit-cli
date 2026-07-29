//! Workspace filesystem discovery adapters.

mod discovery;
pub mod error;
mod filters;
mod types;

pub use discovery::{discover_workspace_files, discover_workspace_files_with_limits};
pub use error::{WorkspaceError, WorkspaceLimit, WorkspaceRefusal, WorkspaceResult};
pub use types::{WorkspaceDiscovery, WorkspaceDiscoveryOptions, WorkspaceLimits};

#[cfg(test)]
mod tests;
