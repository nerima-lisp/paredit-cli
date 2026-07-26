//! Workspace filesystem discovery adapters.

mod discovery;
mod filters;
mod types;

pub use discovery::discover_workspace_files;
pub use types::{WorkspaceDiscovery, WorkspaceDiscoveryOptions};

#[cfg(test)]
mod tests;
