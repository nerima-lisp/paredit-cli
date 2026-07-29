//! Value objects and aggregates of the lint context.
//!
//! Nothing here reads a tree or decides anything: these are the types a rule,
//! the registry, and a report all agree on. Behaviour lives in
//! [`super::policy`] (selection and gating) and [`super::engine`] (the pass).

mod explanation;
mod finding;
mod fix;
mod fixability;
mod head_filter;
mod policy;
mod rule_category;
mod rule_meta;
mod rule_name;
mod rule_setting;
mod rule_tag;
mod severity;

pub use explanation::{RuleExample, RuleExplanation};
pub use finding::{FindingId, LintFinding, LintOutcome, LintSummary};
pub use fix::{Replacement, RuleFix};
pub use fixability::Fixability;
pub use head_filter::{HeadFilter, NormalizedHead};
pub use policy::{LintPolicy, LintPolicyOptions};
pub use rule_category::RuleCategory;
pub use rule_meta::RuleMeta;
pub use rule_name::RuleName;
pub use rule_setting::{RuleSetting, RuleSettings};
pub use rule_tag::{RuleTag, RuleTags};
pub use severity::Severity;
