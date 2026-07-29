//! Decisions about a lint run that do not involve walking a tree: which rules
//! apply, and whether the result passes the caller's gate.

mod dialect_scope;
mod gate;
mod preset;
mod selection;
mod severity_override;

pub use dialect_scope::RuleDialectScope;
pub use gate::{evaluate_lint_policy, lint_gate_violations, summarize_lint_findings};
pub use preset::RulePreset;
pub use selection::{RuleFilter, RuleSelection, resolve_active_rules};
pub use severity_override::SeverityOverrides;
