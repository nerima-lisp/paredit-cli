//! The slice's public analysis surface.

pub use crate::structural_diff::domain::{
    Change, ChangeKind, Excerpt, diff_documents, index_subtrees, shape_hash,
};

/// The `--fail-on-change` gate's verdict.
#[derive(Debug, Clone)]
pub struct DiffPolicy {
    pub fail_on_change: bool,
    pub change_count: usize,
    pub passed: bool,
}

impl DiffPolicy {
    #[must_use]
    pub const fn evaluate(fail_on_change: bool, change_count: usize) -> Self {
        Self {
            fail_on_change,
            change_count,
            passed: !fail_on_change || change_count == 0,
        }
    }
}
