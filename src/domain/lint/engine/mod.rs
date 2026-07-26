//! The single pass that runs every rule, and what a rule sees while it runs.

mod context;
mod dispatch;
mod head_index;
mod ordering;
mod sink;

pub use context::RuleContext;
pub use dispatch::collect_lint_outcomes;
pub use sink::RuleSink;
