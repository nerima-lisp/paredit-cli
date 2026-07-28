//! Workspace filesystem discovery adapters.

mod discovery;
pub mod error;
mod filters;
mod types;

pub use discovery::discover_workspace_files;
pub use error::{WorkspaceError, WorkspaceLimit, WorkspaceRefusal, WorkspaceResult};
pub use types::{WorkspaceDiscovery, WorkspaceDiscoveryOptions};

#[cfg(test)]
mod tests;
