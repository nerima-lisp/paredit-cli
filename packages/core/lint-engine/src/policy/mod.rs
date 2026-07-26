//! Decisions about a lint run that do not involve walking a tree: which rules
//! apply, and whether the result passes the caller's gate.

mod dialect_scope;
mod gate;
mod selection;

pub use dialect_scope::RuleDialectScope;
pub use gate::{evaluate_lint_policy, lint_gate_violations, summarize_lint_findings};
pub use selection::{RuleSelection, resolve_active_rules};
