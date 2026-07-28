//! The single pass that runs every rule, and what a rule sees while it runs.

mod context;
mod dispatch;
mod head_index;
mod ordering;
mod sink;
mod timings;

pub use context::RuleContext;
pub use dispatch::{PassOptions, PassOutcome, collect_lint_outcomes, collect_lint_pass};
pub use head_index::{HeadIndex, build_head_index};
pub use sink::RuleSink;
pub use timings::RuleTimings;
