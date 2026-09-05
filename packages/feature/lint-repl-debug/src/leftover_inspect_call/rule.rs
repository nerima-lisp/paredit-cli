//! `leftover-inspect-call`: a Common Lisp (inspect x) or (describe x) left in committed source.
//!

use paredit_core_lint_engine::LintResult;

use crate::leftover_inspect_call::domain::examine;
use crate::support::{OperatorScope, evaluated_candidates};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// Report-only, deliberately — the same demotion `leftover-print-debug` took,
/// for the same reason and on worse numbers. This is the fact about the rule
/// most likely to be "tidied" back to `Fixable` by someone reading only
/// [`crate::leftover_inspect_call::domain`], where a `fix_span` is still
/// computed and still points at exactly the bytes a human would delete. The
/// span is not the problem; what it points at is.
///
/// The removal-safety analysis in [`crate::support`] establishes that deleting
/// the call cannot change the *value* of its enclosing body, and
/// [`OperatorScope`] withholds the fix when the head is *lexically* shadowed by
/// an enclosing `flet`/`labels`/`macrolet`. Neither can establish the thing
/// that actually matters here: that `describe` names `CL:DESCRIBE` at all.
/// `describe` is also the suite-declaring macro of every BDD test framework in
/// the Common Lisp ecosystem, imported at *package* level — a global rebinding
/// no lexical binding table can see. When it is that macro,
/// `(describe "name" (it …) (it …))` is an entire test suite, and the whole
/// form is what the fix deletes.
///
/// Measured over 6,078 SHA-256-deduplicated third-party Common Lisp files
/// (9,513 raw, 47.77 MB) the rule produced 2,388 findings — *every one of them
/// carrying a fix*, unlike `leftover-print-debug` where 28% did. Running
/// `fix apply --rule leftover-inspect-call` one file at a time over that corpus
/// deleted 8,496,293 bytes and shrank 818 files below half their original size;
/// `t/render-test.lisp` in `cl-asciiquarium` went from 156 lines to 28. Every
/// file still parsed afterwards, which is why no round-trip property caught it.
///
/// On a seeded stratified sample of 172 findings (seed 20260805), 172 were
/// false positives — 100%, against 97.0% for `leftover-print-debug`. 2,378 of
/// the 2,388 findings (99.6%), accounting for 100.0% of the deleted bytes, are
/// `(describe "string" …)`: a suite name, not an object to describe. The
/// closest thing to a true positive in the whole corpus was
/// `(describe object *standard-output*)` inside `cl-cc`'s own `%repl-inspect`
/// — the implementation of that REPL's inspect command, i.e. the program's
/// output, which is the `leftover-print-debug` failure exactly.
///
/// Three narrower remedies were measured and rejected:
///
/// - **Narrowing [`crate::leftover_inspect_call::domain::HEADS`] to `inspect`
///   alone.** That drops 2,382 of 2,388 findings and leaves 6 — all of which
///   are `cl-cc:inspect`/`cl-cc/debug:inspect`, a *user's own* debug library,
///   flagged inside that library's own test file. Narrowing leaves a rule with
///   no true positive at all, so it is deleting the rule, not fixing it.
/// - **Gating the fix to one dialect.** Unavailable by construction: the rule
///   is already Common-Lisp-only via the default
///   [`LintRule::dialect_scope`], so there is no second dialect to retreat to.
/// - **Demoting to `Pedantic`.** Insufficient, and note that the reason given
///   in #129 — that `fix apply --rule` bypasses presets entirely — does not
///   hold at this revision: `--rule` *intersects* with the preset.
///   `fix apply --preset minimal --rule leftover-inspect-call` leaves the file
///   untouched. But `pedantic` and `all` both still delete, measured on
///   `render-test.lisp`: 156 lines → 28 under each. A `Pedantic` tag would only
///   move the deletion behind a flag users routinely pass, not remove it.
///
/// Only `Fixability::ReportOnly` withholds the rewrite at every preset.
///
/// [`OperatorScope`]: crate::support::OperatorScope
/// [`LintRule::dialect_scope`]: paredit_core_lint_engine::rule::LintRule::dialect_scope
pub const META: RuleMeta = RuleMeta::new(
    "leftover-inspect-call",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a Common Lisp (inspect x) or (describe x) left in committed source",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let candidates = evaluated_candidates(context, view);
        let mut items = Vec::new();
        let scope = OperatorScope::shared(context);
        examine(candidates, &scope, context.path(), &mut items);
        for item in items {
            // No `report_fixed` branch, deliberately: see `META`. `item`'s
            // `fix_span` stays computed because the standalone
            // `inspect leftover-inspect-call` report still names the span a
            // human would delete — it just no longer becomes a rewrite the
            // tool applies on its own.
            sink.report(
                item.span,
                format!("{} is a leftover interactive-debugger call", item.head),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::model::Fixability;
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    use super::{META, RULE};

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    /// Runs the real dispatch and returns `(finding messages, rewritten source)`.
    ///
    /// The rewrite is applied exactly as `fix apply` would: every fix the rule
    /// attached, spliced into the source. With no fixes attached that is the
    /// identity — which is the property under test, expressed as the *source*
    /// rather than as `fix.is_none()`, so it still fails if a fix reappears
    /// through some other path.
    fn run(source: &str) -> (Vec<String>, String) {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let outcomes = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("probe.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("dispatch");

        let mut messages = Vec::new();
        let mut edits = Vec::new();
        for outcome in outcomes {
            let (finding, fix) = outcome.into_parts();
            messages.push(finding.message);
            if let Some(fix) = fix {
                for replacement in fix.replacements() {
                    edits.push((replacement.span(), replacement.text().to_owned()));
                }
            }
        }
        edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start().get()));
        let mut rewritten = source.to_owned();
        for (span, text) in edits {
            rewritten.replace_range(span.start().get()..span.end().get(), &text);
        }
        (messages, rewritten)
    }

    #[test]
    fn the_rule_is_report_only() {
        assert_eq!(META.fixability(), Fixability::ReportOnly);
    }

    /// The regression this rule was demoted for, reduced from the corpus case
    /// that shrank `cl-asciiquarium`'s `t/render-test.lisp` from 156 lines to
    /// 28: `describe` is the BDD suite macro, so the "leftover debug call" is
    /// an entire test suite and the fix deleted all of it.
    ///
    /// Asserted as the rewritten source, byte for byte, rather than as an
    /// absent fix — deleting the suite and reinserting equivalent text would
    /// pass the weaker check.
    #[test]
    fn a_bdd_suite_macro_is_reported_and_the_source_is_left_alone() {
        let source = "(describe \"draw-world\"\n  (it \"returns the screen\"\n    (expect (draw-world s w) :to-be s)))\n(run-tests)";
        let (messages, rewritten) = run(source);
        assert_eq!(
            rewritten, source,
            "the rule deleted a test suite it must only report on"
        );
        assert_eq!(messages.len(), 1, "the finding must survive the demotion");
    }

    /// The other corpus shape: a package-qualified head is a *different symbol*
    /// that merely shares a name — `cl-cc:inspect` is a user's own debug
    /// library, and all six `inspect` findings in the corpus were this.
    #[test]
    fn a_package_qualified_head_is_reported_and_the_source_is_left_alone() {
        let source = "(cl-cc:inspect table)\n(+ 1 2)";
        let (messages, rewritten) = run(source);
        assert_eq!(rewritten, source);
        assert_eq!(messages.len(), 1);
    }

    /// The closest thing to a true positive in the whole corpus — the body of
    /// `cl-cc`'s own `%repl-inspect`, i.e. the program's output. Reported, not
    /// rewritten.
    #[test]
    fn a_genuine_describe_call_is_still_reported_but_never_rewritten() {
        let source = "(defun %repl-inspect (object)\n  (describe object *standard-output*)\n  (force-output))";
        let (messages, rewritten) = run(source);
        assert_eq!(rewritten, source);
        assert_eq!(
            messages,
            vec!["describe is a leftover interactive-debugger call"]
        );
    }

    /// Anti-over-suppression control: demoting fixability must not have
    /// silenced anything, and must not have widened the rule either. Without
    /// this, every assertion above would pass for a rule that reported nothing.
    #[test]
    fn the_findings_themselves_are_unchanged_by_the_demotion() {
        for head in super::super::domain::HEADS {
            let source = format!("({head} x)\n(+ 1 2)");
            let (messages, rewritten) = run(&source);
            assert_eq!(messages.len(), 1, "{head} stopped reporting");
            assert_eq!(rewritten, source, "{head} rewrote source");
        }

        // Shapes the rule never reported still go unreported.
        for source in ["'(inspect x)", "(quote (describe x))", "(fboundp 'inspect)"] {
            let (messages, rewritten) = run(source);
            assert!(messages.is_empty(), "{source} became a finding");
            assert_eq!(rewritten, source);
        }
    }
}
