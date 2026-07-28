//! Infrastructure adapters that turn filesystems and workspace discovery into
//! inputs consumable by the application layer.

// Phase 2 facade (section 4.1). This layer is now re-exports only.
pub use paredit_core_workspace::{fs_identity, workspace};
