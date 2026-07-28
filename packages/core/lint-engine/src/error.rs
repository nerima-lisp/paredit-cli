//! What the lint engine, and a rule running inside it, can fail with.
//!
//! Section 9.2. The interesting fact about this package is how *little* can go
//! wrong: of the 134 registered rules, four are fallible, and all four fail the
//! same way — they consult the whole tree, resolve an expression path, and the
//! path does not resolve. Everything else in a lint run is total.
//!
//! That is worth a type rather than `anyhow::Result` precisely *because* the
//! set is so small. `anyhow::Result<()>` on [`LintRule::check`](crate::rule::LintRule::check)
//! says "this can fail for any reason at all", which is what made the one
//! genuine failure mode invisible: a reader could not tell whether a rule was
//! allowed to report a malformed-source error, a resource limit, or an I/O
//! failure. [`LintError`] says it cannot.

use thiserror::Error;

use paredit_core_syntax::sexpr::SexprError;

/// A failure during a lint pass.
///
/// One variant today. It is an enum rather than a re-export of [`SexprError`]
/// because "what a lint rule may fail with" and "what the syntax layer may
/// fail with" are different questions that happen to have the same answer
/// right now — and the first is the one a rule author needs answered.
#[derive(Debug, Error)]
pub enum LintError {
    /// A rule that walks the whole tree could not resolve a path into it.
    ///
    /// Surfaced rather than swallowed: reading a resolution failure as "no
    /// findings" would turn a bug in the rule into a silently clean report.
    #[error(transparent)]
    Selection(#[from] SexprError),
}

/// The result type [`LintRule::check`](crate::rule::LintRule::check) and the
/// dispatch pass return.
pub type LintResult<T = ()> = std::result::Result<T, LintError>;

/// A rule or category name that is not registered.
///
/// Separate from [`LintError`] because it is raised while *choosing* rules,
/// before any tree exists — it is a CLI argument problem, and the caller's
/// response is to print the valid names rather than to abandon a pass.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuleSelectionError {
    #[error("unknown lint rule {name:?}; valid rules: {valid}")]
    UnknownRule { name: String, valid: String },

    #[error("unknown lint category {name:?}; valid categories: {valid}")]
    UnknownCategory { name: String, valid: String },

    #[error("unknown lint tag {name:?}; valid tags: {valid}")]
    UnknownTag { name: String, valid: String },

    #[error("unknown lint preset {name:?}; valid presets: {valid}")]
    UnknownPreset { name: String, valid: String },

    /// A `--rule-arg` naming a knob the rule does not declare.
    ///
    /// Separate from `UnknownRule` because the remedy differs: the rule exists
    /// and the caller is close, so the message lists *that rule's* knobs rather
    /// than all 143 rule names.
    #[error("lint rule {rule:?} has no setting {key:?}; valid settings: {valid}")]
    UnknownRuleSetting {
        rule: String,
        key: String,
        valid: String,
    },

    /// A `--rule-arg` that is not `<rule>.<key>=<value>`.
    #[error("malformed --rule-arg {argument:?}; expected <rule>.<key>=<value>")]
    MalformedRuleArgument { argument: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wording is a CLI contract: `tests/cli/lint_report.rs` asserts on
    /// these prefixes, so the conversion to a typed error must not reword them.
    #[test]
    fn selection_errors_render_exactly_as_before() {
        assert_eq!(
            RuleSelectionError::UnknownRule {
                name: "no-such-rule".to_owned(),
                valid: "if-arity, empty-body".to_owned(),
            }
            .to_string(),
            r#"unknown lint rule "no-such-rule"; valid rules: if-arity, empty-body"#
        );
        assert_eq!(
            RuleSelectionError::UnknownCategory {
                name: "no-such".to_owned(),
                valid: "arity, dead-code".to_owned(),
            }
            .to_string(),
            r#"unknown lint category "no-such"; valid categories: arity, dead-code"#
        );
    }

    /// A rule failure keeps its cause reachable instead of flattening it, so a
    /// caller can tell a path-resolution bug from any future variant.
    #[test]
    fn a_rule_failure_carries_the_syntax_error_it_came_from() {
        use paredit_core_syntax::sexpr::{SelectionError, SexprError};

        let error = LintError::from(SexprError::Selection(SelectionError::TreeMismatch));
        let LintError::Selection(SexprError::Selection(inner)) = &error else {
            panic!("expected a selection failure, got {error:?}");
        };
        assert_eq!(*inner, SelectionError::TreeMismatch);
        assert_eq!(
            error.to_string(),
            "selection belongs to a different syntax tree"
        );
    }
}
