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

    /// The run's wall-clock budget expired during the walk.
    ///
    /// The second variant, and the reason the doc comment above no longer
    /// reads "one variant today": a lint pass is total with respect to its
    /// input and *not* with respect to time. A 170-rule pass over a
    /// pathological file is the one place this package can run unboundedly
    /// long, and reporting that as a clean file would be the worst possible
    /// answer.
    #[error(transparent)]
    TimedOut(#[from] paredit_core_safety::deadline::TimeoutError),
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
    #[error("unknown lint rule {name:?}; valid rules: {valid}{suggestion}")]
    UnknownRule {
        name: String,
        valid: String,
        /// `; did you mean "x"?`, or empty when nothing was close enough.
        /// Baked into the message at construction — see [`did_you_mean`] —
        /// rather than left for a caller to compute, so every consumer of
        /// this error (text, JSON, a wrapping tool) sees the same hint.
        suggestion: String,
    },

    #[error("unknown lint category {name:?}; valid categories: {valid}{suggestion}")]
    UnknownCategory {
        name: String,
        valid: String,
        suggestion: String,
    },

    #[error("unknown lint tag {name:?}; valid tags: {valid}{suggestion}")]
    UnknownTag {
        name: String,
        valid: String,
        suggestion: String,
    },

    #[error("unknown lint preset {name:?}; valid presets: {valid}{suggestion}")]
    UnknownPreset {
        name: String,
        valid: String,
        suggestion: String,
    },

    /// A `--rule-arg` naming a knob the rule does not declare.
    ///
    /// Separate from `UnknownRule` because the remedy differs: the rule exists
    /// and the caller is close, so the message lists *that rule's* knobs rather
    /// than all 143 rule names.
    #[error("lint rule {rule:?} has no setting {key:?}; valid settings: {valid}{suggestion}")]
    UnknownRuleSetting {
        rule: String,
        key: String,
        valid: String,
        suggestion: String,
    },

    /// A `--rule-arg` that is not `<rule>.<key>=<value>`.
    #[error("malformed --rule-arg {argument:?}; expected <rule>.<key>=<value>")]
    MalformedRuleArgument { argument: String },
}

/// `; did you mean "closest"?` when `name` is close to one of `candidates`,
/// else an empty string.
///
/// Distance threshold of 3 matches the same trick already used for
/// `paredit.toml` keys and rule names in `paredit-core-config`: close enough
/// to catch a typo, far enough not to suggest an unrelated rule for a name
/// that shares no real resemblance to anything registered.
#[must_use]
pub fn did_you_mean<'a>(candidates: impl IntoIterator<Item = &'a str>, name: &str) -> String {
    candidates
        .into_iter()
        .map(|candidate| (levenshtein(candidate, name), candidate))
        .filter(|(distance, _)| *distance <= 3 && *distance > 0)
        .min_by_key(|(distance, candidate)| (*distance, *candidate))
        .map_or(String::new(), |(_, candidate)| {
            format!("; did you mean {candidate:?}?")
        })
}

fn levenshtein(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (row, left_char) in left.iter().enumerate() {
        current[0] = row + 1;
        for (column, right_char) in right.iter().enumerate() {
            let substitution = usize::from(left_char != right_char);
            current[column + 1] = (previous[column] + substitution)
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wording is a CLI contract: `tests/cli/lint_report.rs` asserts on
    /// these prefixes, so the conversion to a typed error must not reword them.
    /// An empty `suggestion` contributes nothing to the rendering, which is
    /// what keeps this contract intact for a caller that builds the variant
    /// directly rather than through [`did_you_mean`].
    #[test]
    fn selection_errors_render_exactly_as_before() {
        assert_eq!(
            RuleSelectionError::UnknownRule {
                name: "no-such-rule".to_owned(),
                valid: "if-arity, empty-body".to_owned(),
                suggestion: String::new(),
            }
            .to_string(),
            r#"unknown lint rule "no-such-rule"; valid rules: if-arity, empty-body"#
        );
        assert_eq!(
            RuleSelectionError::UnknownCategory {
                name: "no-such".to_owned(),
                valid: "arity, dead-code".to_owned(),
                suggestion: String::new(),
            }
            .to_string(),
            r#"unknown lint category "no-such"; valid categories: arity, dead-code"#
        );
    }

    /// The suggestion is exercised end to end: a name one edit away from a
    /// registered rule gets a hint appended, spelled the same way the
    /// existing config-key and lint-rule "did you mean" hints are spelled.
    #[test]
    fn a_name_one_edit_away_gets_a_suggestion() {
        let error = RuleSelectionError::UnknownRule {
            name: "if-ariti".to_owned(),
            valid: "if-arity, empty-body".to_owned(),
            suggestion: did_you_mean(["if-arity", "empty-body"], "if-ariti"),
        };
        assert_eq!(
            error.to_string(),
            r#"unknown lint rule "if-ariti"; valid rules: if-arity, empty-body; did you mean "if-arity"?"#
        );
    }

    /// A name with no close match gets no suggestion rather than a wrong one.
    #[test]
    fn a_name_with_no_close_match_gets_no_suggestion() {
        assert_eq!(did_you_mean(["if-arity", "empty-body"], "no-such-rule"), "");
    }

    /// The candidate itself is never "suggested" as a fix for itself — a
    /// distance of zero means the name already matched, which is a different
    /// code path than `UnknownRule` entirely, but the helper should still
    /// refuse to echo an exact match back as a "did you mean".
    #[test]
    fn an_exact_match_produces_no_suggestion() {
        assert_eq!(did_you_mean(["if-arity"], "if-arity"), "");
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
