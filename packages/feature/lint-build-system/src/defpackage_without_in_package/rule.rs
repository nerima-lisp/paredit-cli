//! `defpackage-without-in-package`: a file that declares a package, defines
//! things, and never enters it.
//!
//! The analysis lives in [`crate::defpackage_without_in_package::domain`],
//! which also backs the standalone `inspect defpackage-without-in-package`
//! command; this module only registers it with the lint suite and phrases its
//! findings.
//!
//! # Why this is `WholeTree` and not `Heads`
//!
//! "A file with definitions but no `in-package`" is a question about an
//! *absence*, and its answer is a property of the file, not of any node. The
//! sibling rules in this package are `Heads` and should be; this one is the
//! justified exception, for a reason that is about cost rather than taste.
//!
//! Under `Heads` the rule is dispatched once per `defpackage` in the file, and
//! every one of those calls runs the same whole-file walk and produces the same
//! answer, of which all but one are then thrown away by a span test. Worse, the
//! guard that decides whether to walk is a *negative* one — return early if
//! `in-package` is present — so the walk runs precisely on the shape with the
//! most `defpackage` forms and the least reason to look at any of them past the
//! first: a `package.lisp` with no `in-package`. N declarations cost N walks,
//! N−1 of them discarded.
//!
//! `WholeTree` collapses that to one call per file. It is free rather than
//! merely cheaper, because `collect_lint_pass` materializes the root view
//! unconditionally — `let root = tree.root_view();`, before the `whole_tree()`
//! loop and before the walk — and hands that same view to every `WholeTree`
//! rule. So this rule now borrows a view the dispatcher built anyway, where
//! under `Heads` it called `root_view()` itself and rebuilt the entire document
//! each time.
//!
//! `collect_lint_pass`: paredit_core_lint_engine::engine::dispatch
//!
//! Being dispatched on every file rather than only on files with a package
//! declaration is what the guard pair in [`examine_file`] pays for, and it is
//! ordered to make that free:
//!
//! 1. a byte scan for `defpackage`/`define-package` — the *positive* polarity,
//!    matching `ConditionHierarchy::collect`'s `mentions_define_condition`
//!    guard. A file with no package declaration cannot produce a finding, and
//!    now stops here instead of at dispatch;
//! 2. a byte scan for `in-package` — settles almost every file that *does*
//!    declare a package, since nearly all of them enter one;
//! 3. only then the whole-file walk, on the one shape this rule is about;
//! 4. never `binding_table()`/`value_table()`/`type_table()` — this rule needs
//!    no semantic pass and asks for none.
//!
//! The benchmark files are unaffected either way: `clean/forms/*` contain no
//! package declaration, so under `Heads` `check` was never called and under
//! `WholeTree` it returns on the first byte scan.

use paredit_core_lint_engine::LintResult;

use crate::defpackage_without_in_package::domain::examine_file;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "defpackage-without-in-package",
    // The file is well formed and loads without complaint; what it means is not
    // what it looks like it means. That is exactly `Suspicious`.
    RuleCategory::Suspicious,
    // A judgement about intent, not a settled defect: a file may declare a
    // package for another file to enter. `Warning` is the honest level.
    Severity::Warning,
    "a file that declares a package and defines symbols but never enters it",
    // Which of the file's packages should be entered is a decision.
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
        // `view` is the root the dispatcher already built. `examine_file` opens
        // with two byte scans, so a file that declares no package, or that
        // enters the one it declares, returns here without a walk.
        //
        // One finding per file, at the first declaration reached as *code* —
        // which is now what the single call naturally produces rather than
        // something a span test has to select. It is also what makes a quoted
        // `defpackage` silent: the walk visits evaluated code only, so a quoted
        // declaration is never reached and no separate `is_unevaluated_at`
        // question is needed.
        let Some(item) = examine_file(context.source(), view) else {
            return Ok(());
        };
        sink.report(item.span, item.message());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::run_rule;
    use paredit_core_lint_engine::rule::RuleEntry;

    /// A one-rule catalogue, so the engine's dispatch — the thing that decides
    /// whether and how often `check` is called — is exercised for real.
    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn messages(source: &str) -> Vec<String> {
        run_rule(&ENTRIES, source)
    }

    /// Under `WholeTree` no head index is consulted, so what this pins is the
    /// byte-scan guard rather than a dispatch table: each spelling has to get
    /// past `declares_a_package`, which is case-insensitive and sees the
    /// `cl:`/`uiop:` qualifiers as ordinary bytes around the needle.
    #[test]
    fn every_package_declaration_spelling_gets_past_the_byte_scan_guard() {
        for head in [
            "defpackage",
            "cl:defpackage",
            "uiop:define-package",
            "DEFPACKAGE",
        ] {
            assert_eq!(
                messages(&format!("({head} :app)\n(defun f () 1)\n")).len(),
                1,
                "`{head}` did not reach a finding"
            );
        }
    }

    /// One finding per file however many declarations it has. Under `Heads`
    /// this was a `check`-level guard against three dispatches; under
    /// `WholeTree` there is one dispatch and the property falls out of the
    /// domain. The test stays because the property is what callers rely on,
    /// not the mechanism that delivers it.
    #[test]
    fn three_declarations_in_one_file_produce_exactly_one_finding() {
        let found = messages("(defpackage :a)\n(defpackage :b)\n(defpackage :c)\n(defun f () 1)\n");
        assert_eq!(found.len(), 1, "expected one finding, got {found:?}");
        assert!(found[0].contains(":a"), "reported at the wrong declaration");
    }

    /// The regression this rule's `WholeTree` switch could have introduced:
    /// `check` now runs on files that were previously filtered out at
    /// dispatch, so a file with no package declaration must be settled by the
    /// positive-polarity byte scan rather than by a walk.
    #[test]
    fn a_file_with_no_package_declaration_at_all_produces_nothing() {
        assert!(messages("(defun f () 1)\n(defvar *x* 2)\n").is_empty());
        assert!(messages("").is_empty());
    }

    #[test]
    fn a_file_that_enters_its_package_produces_nothing_through_the_engine() {
        assert!(messages("(defpackage :app)\n(in-package :app)\n(defun f () 1)\n").is_empty());
    }

    #[test]
    fn a_package_file_with_no_definitions_produces_nothing_through_the_engine() {
        assert!(messages("(defpackage :app (:use :cl) (:export #:run))\n").is_empty());
    }

    #[test]
    fn a_quoted_declaration_produces_nothing_through_the_engine() {
        assert!(messages("'(defpackage :app)\n(defun f () 1)\n").is_empty());
        assert!(messages("(quote (defpackage :app))\n(defun f () 1)\n").is_empty());
        assert!(messages("'(a ,(defpackage :app))\n(defun f () 1)\n").is_empty());
        assert!(messages("`(defpackage :app)\n(defun f () 1)\n").is_empty());
        assert_eq!(
            messages("`(a ,(defpackage :app))\n(defun f () 1)\n").len(),
            1
        );
    }
}
