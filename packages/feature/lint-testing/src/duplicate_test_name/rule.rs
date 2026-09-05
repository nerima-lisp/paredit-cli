//! `duplicate-test-name`: two top-level test definitions in one file sharing a
//! name.
//!
use paredit_core_lint_engine::LintResult;

use crate::duplicate_test_name::domain::shadowing_test_definitions;
use crate::support::TEST_DIALECTS;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// `Duplicate` — "the same key, place, test, or name given twice" names this
/// exactly.
///
/// `Severity::Error`, following `duplicate-defmethod-signature`, which is the
/// existing file-scoped duplicate-definition rule. The consequence is not a
/// judgement call: loading the file keeps the later definition and discards the
/// earlier one, so a test that was written and reviewed does not run, and
/// nothing in a green suite says so.
pub const META: RuleMeta = RuleMeta::new(
    "duplicate-test-name",
    RuleCategory::Duplicate,
    Severity::Error,
    "two test definitions in one file sharing a name",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    /// `WholeTree`, alone among this package's six rules, and for the same
    /// reason as `duplicate-parameters`: whether a definition shadows another
    /// is not a question about one node, so a per-node filter can only answer
    /// it by re-deriving the file's other definitions on every match.
    ///
    /// That is what the first version did, and it cost T×T on a file of T
    /// tests. `Heads` looks cheaper — it runs nothing on a file with no test
    /// definitions — but the run it makes cheap is one `WholeTree` already
    /// costs a single pass over the top-level forms, and the run it makes
    /// ruinous is the one that matters.
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&TEST_DIALECTS)
    }

    /// The analysis already walks evaluated code only, so — unlike this
    /// package's five head-filtered rules — there is no `is_unevaluated_at`
    /// call to make here: a `'(deftest …)` never reaches the findings at all.
    ///
    /// `view` is the root view, which under `WholeTree` the dispatcher has
    /// already materialized for this file and hands to every such rule. Using
    /// it is the entire point of being `WholeTree`: this rule first called
    /// `build_duplicate_test_name_report(.., context.tree())` instead, which
    /// rebuilt the whole document twice per file — two `Vec`s per node, and
    /// `root_view` is uncached — and that alone was +22% on the `clean/forms`
    /// benchmark, on files with no test definition in them at all.
    ///
    /// The denominator the standalone report publishes is deliberately not
    /// computed here. It needs a second walk, this time into every form rather
    /// than across the top level, and a rule has nowhere to put it: the old
    /// code paid for that walk on every file and then dropped the number.
    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        for item in shadowing_test_definitions(view, context.dialect()) {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The structural half of the cost regression, next to the timed one in
    /// `domain`: `Heads` means one `check` per test definition, and a `check`
    /// that has to look at the rest of the file is then quadratic by
    /// construction no matter how it is written. `WholeTree` means one `check`
    /// per file.
    #[test]
    fn the_correlation_runs_once_per_file_not_once_per_definition() {
        assert_eq!(RULE.head_filter(), HeadFilter::WholeTree);
    }
}
