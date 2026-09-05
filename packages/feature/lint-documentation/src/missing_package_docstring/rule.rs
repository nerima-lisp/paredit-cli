//! `missing-package-docstring`: a `defpackage` that says nothing about what
//! the package is for.
//!
//!
//! `Heads`, not `WholeTree`, even though half the evidence is comments. The
//! *subject* here is a node — a `defpackage` form — and the comment scan is
//! only the second half of a question a matched node already raised. So the
//! rule is dispatched on the handful of files that declare a package rather
//! than on every file, and the comment list is read only for a declaration that
//! carries no `(:documentation …)` option: a documented package never reads a
//! comment at all.
//!
//! That ordering also keeps the cost linear. The comment scan runs at most once
//! per undocumented declaration, stops at the first comment past the
//! declaration, and stops at the first comment that is prose.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleTag,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::missing_package_docstring::domain::examine;
use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "missing-package-docstring",
    RuleCategory::Documentation,
    Severity::Warning,
    "a defpackage with no (:documentation \"…\") option and no comment describing the package",
    // Generating a description is a *transformation*, and a fix that inserted
    // an empty option would let a project satisfy the rule without documenting
    // anything — the reason `missing-docstring` is report-only too.
    Fixability::ReportOnly,
)
// Whether a package needs a description is a project's decision. Distinct from
// `missing-docstring`, which asks the same question of every definition: a
// project may well want this one on and that one off.
.with_tags(&[RuleTag::Pedantic])
.with_explanation(
    RuleExplanation::new(
        "A package is the unit a reader meets first — what `:use` names, what `apropos` groups \
         by, what an API index is organised around — and the one definition nothing else in this \
         suite asks about: `missing-docstring` does not list `defpackage`, and `inspect \
         docstrings` excludes it. A file can document all forty of its functions and still never \
         say what the package they live in is for.",
    )
    .with_example(
        "(defpackage :app (:use :cl) (:export #:run))",
        "(defpackage :app (:use :cl) (:export #:run)\n  (:documentation \"The application's \
         public interface.\"))",
    )
    .with_caveat(
        "A prose comment anywhere before the declaration counts as documentation, including a \
         file header several forms up. A project that describes its package in a `;;;;` header \
         instead of an option has described its package.",
    ),
);

/// The two spellings of a Common Lisp package declaration. Clojure's `ns` is
/// deliberately absent: its docstring has two spellings, one of them a
/// `^{:doc "…"}` metadata map, and reading that one wrongly would report a
/// documented namespace.
const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("defpackage"),
    NormalizedHead::new("define-package"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(found) = examine(context.tree(), view) else {
            return Ok(());
        };
        // Last, and only once a finding is otherwise settled: a `defpackage`
        // inside `'(…)` is list data, not a declaration.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(found.span, found.message());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::run_rule_with;
    use paredit_core_lint_engine::model::RuleSettings;
    use paredit_core_lint_engine::rule::RuleEntry;

    /// A one-rule catalogue, so the engine's own head index and dispatch sit
    /// between the test and the rule.
    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn messages(source: &str) -> Vec<String> {
        run_rule_with(&ENTRIES, source, &RuleSettings::new())
    }

    /// The head index folds package qualifiers and case before the lookup, so
    /// each spelling has to reach `check` through it.
    #[test]
    fn every_declaration_spelling_reaches_a_finding_through_the_real_dispatch() {
        for head in [
            "defpackage",
            "cl:defpackage",
            "uiop:define-package",
            "DEFPACKAGE",
        ] {
            assert_eq!(
                messages(&format!("({head} :app (:use :cl))\n")).len(),
                1,
                "`{head}` did not reach a finding"
            );
        }
    }

    /// Under `Heads`, one finding per declaration — unlike
    /// `defpackage-without-in-package`, which answers a question about the
    /// *file* and so answers it once. Two undocumented packages are two
    /// undocumented packages.
    #[test]
    fn two_undocumented_declarations_produce_two_findings() {
        let found = messages("(defpackage :a (:use :cl))\n(defpackage :b (:use :cl))\n");
        assert_eq!(found.len(), 2, "{found:?}");
    }

    #[test]
    fn a_documented_declaration_produces_nothing_through_the_real_dispatch() {
        assert!(
            messages("(defpackage :app (:use :cl) (:documentation \"The interface.\"))\n")
                .is_empty()
        );
        assert!(messages(";;;; The interface.\n(defpackage :app (:use :cl))\n").is_empty());
    }

    #[test]
    fn a_file_that_declares_no_package_produces_nothing() {
        assert!(messages("(defun f () 1)\n(defvar *x* 2)\n").is_empty());
        assert!(messages("").is_empty());
    }

    // --- the guard the domain tests cannot exercise

    #[test]
    fn no_finding_inside_any_of_the_five_quote_shapes() {
        for source in [
            "'(defpackage :app (:use :cl))",
            "(quote (defpackage :app (:use :cl)))",
            "'(a ,(defpackage :app (:use :cl)))",
            "`(defpackage :app (:use :cl))",
            "(defmacro defpkg (n) `(defpackage ,n (:use :cl)))",
        ] {
            assert!(
                messages(source).is_empty(),
                "fired on quoted data: {source}"
            );
        }
    }

    #[test]
    fn an_unquoted_declaration_inside_a_quasiquote_still_fires() {
        assert_eq!(messages("`(a ,(defpackage :app (:use :cl)))").len(), 1);
    }

    /// A realistic `package.lisp`: the shape a reviewer runs first, and the one
    /// this rule would be most embarrassing to nag about.
    #[test]
    fn a_realistic_documented_package_file_produces_nothing() {
        let source = ";;;; package.lisp — the application's public interface.\n\
             ;;;;\n\
             ;;;; Everything a caller needs is exported from here.\n\n\
             (defpackage #:app\n  \
             (:use #:cl #:alexandria)\n  \
             (:export #:run #:stop))\n\n\
             (in-package #:app)\n";
        assert!(messages(source).is_empty(), "{:?}", messages(source));
    }
}
