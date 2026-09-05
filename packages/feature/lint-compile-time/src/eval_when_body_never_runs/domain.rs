//! An `eval-when` that is *not* at top level and does not name `:execute`, so
//! its body never runs — in any phase.
//!
//! ```lisp
//! (defun run ()
//!   (eval-when (:compile-toplevel) (setf *marker* :fired))
//!   :done)
//! ```
//!
//! CLHS 3.2.3.1 is explicit that `:compile-toplevel` and `:load-toplevel` are
//! considered **only** for a top level `eval-when`; anywhere else the form is
//! equivalent to `(if (member :execute situations) (progn body) nil)`. Naming
//! neither `:execute` nor `eval` in a nested `eval-when` therefore expands to
//! `nil`, and the body is dead code that reads like phase control.
//!
//! Verified against SBCL 2.6.0 on exactly that file, both phases:
//!
//! - `(load "src.lisp")` → `*marker*` is `:UNTOUCHED`.
//! - `(load (compile-file "src.lisp"))` → `*marker*` is `:UNTOUCHED`.
//! - the same body with `(eval-when (:execute) …)` → `:FIRED` in both phases.
//! - `(eval-when (:compile-toplevel :load-toplevel) …)` nested → `:UNTOUCHED`
//!   in both phases.
//!
//! **SBCL emits no diagnostic whatsoever for this.** Not a warning, not a style
//! warning, not a note. That is what separates this rule from
//! [`crate::eval_when_execute_only`], whose subject SBCL at least style-warns
//! about downstream: here there is nothing to notice, in either phase, ever. The
//! author wrote code that they believe runs at compile time and it runs nowhere.
//!
//! # Deliberate limits
//!
//! - **Top level is CLHS 3.2.3.1's recursion, not depth 0.** The body of a
//!   top-level `progn`/`locally`/`macrolet`/`symbol-macrolet`/`eval-when` is
//!   still top level, so an `eval-when` there is *correct* to name
//!   `:compile-toplevel` and is not flagged. Treating depth as top-level status
//!   produces false positives;
//!   [`crate::support::is_top_level_form`] enumerates the operators rather than
//!   counting depth.
//! - **The situation list must name something.** `(eval-when () …)` is dead
//!   everywhere, top level or not, and says nothing about phases.
//! - **The body must be non-empty.** `(eval-when (:compile-toplevel))` discards
//!   nothing.
//! - **No fix.** Whether the author meant `:execute`, meant to hoist the form to
//!   top level, or meant to delete it is not recoverable from the source.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::eval_when_execute_only::domain::{EVAL_WHEN, is_eval_when};
use crate::support::{
    EvalWhenSituations, for_each_evaluated_subview, is_top_level_form, mentions, read_situations,
};

/// Where an `eval-when`'s body begins: head, situations, then forms.
const BODY_START: usize = 2;

#[derive(Debug, Clone)]
pub struct EvalWhenBodyNeverRunsItem {
    /// The span of the whole `eval-when`.
    pub span: ByteSpan,
    /// The situations named, which are exactly the ones being ignored.
    pub situations: String,
    /// How many body forms never run.
    pub body_form_count: usize,
}

impl Finding for EvalWhenBodyNeverRunsItem {
    fn kind(&self) -> &'static str {
        "eval-when-body-never-runs"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("situations={}", self.situations),
            format!("body_form_count={}", self.body_form_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("situations", json!(self.situations)),
            ("body_form_count", json!(self.body_form_count)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "this eval-when is not a top level form, so {} {} ignored here and only :execute would \
             be considered; naming neither :execute nor eval means its {} body form(s) never run, \
             in any phase, with no diagnostic from the compiler",
            self.situations,
            if self.situations.contains(' ') {
                "are"
            } else {
                "is"
            },
            self.body_form_count
        )
    }
}

/// The situations named, in the standard spelling, for the message.
fn describe(situations: EvalWhenSituations) -> String {
    let mut named = Vec::new();
    if situations.compile_toplevel {
        named.push(":compile-toplevel");
    }
    if situations.load_toplevel {
        named.push(":load-toplevel");
    }
    named.join(" ")
}

/// The finding this `eval-when` implies, if any.
///
/// **Ordering is load-bearing**, and in this rule it is the more important of
/// the two directions: the tree question here is the *expensive* one and the
/// one that is almost always answered "top level, no finding". Reading the
/// situation list first rejects every `eval-when` that names `:execute` — which
/// is nearly all of them, since the three-situation spelling is the idiom —
/// without touching the tree at all.
#[must_use]
pub fn examine_eval_when(
    tree: &SyntaxTree,
    view: &ExpressionView,
) -> Option<EvalWhenBodyNeverRunsItem> {
    // 1. node-local: the situation list.
    let situations = read_situations(view.children.get(1)?)?;
    if situations.execute || !situations.reaches_the_compiler() {
        return None;
    }
    // 2. node-local: is anything actually discarded?
    let body_form_count = view.children.get(BODY_START..).unwrap_or_default().len();
    if body_form_count == 0 {
        return None;
    }
    // 3. only now, the tree. A top-level eval-when honours these situations and
    //    is correct; this rule is about the other context.
    if is_top_level_form(tree, view.span) {
        return None;
    }
    Some(EvalWhenBodyNeverRunsItem {
        span: view.span,
        situations: describe(situations),
        body_form_count,
    })
}

fn collect(tree: &SyntaxTree) -> (Vec<EvalWhenBodyNeverRunsItem>, usize) {
    let mut findings = Vec::new();
    let mut candidates = 0;
    for_each_evaluated_subview(&tree.root_view(), |view| {
        if !is_eval_when(view) {
            return;
        }
        candidates += 1;
        if let Some(item) = examine_eval_when(tree, view) {
            findings.push(item);
        }
    });
    (findings, candidates)
}

/// Collects the file's findings with the number of `eval-when` forms scanned as
/// the denominator beside it.
pub fn build_eval_when_body_never_runs_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EvalWhenBodyNeverRunsItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("eval_when_count", json!(0))],
        ));
    }
    if !mentions(tree.source(), EVAL_WHEN) {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            true,
            tree.source(),
            Vec::new(),
            vec![("eval_when_count", json!(0))],
        ));
    }
    let (findings, candidates) = collect(tree);
    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        findings,
        vec![("eval_when_count", json!(candidates))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<EvalWhenBodyNeverRunsItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_eval_when_body_never_runs_report(Path::new("app.lisp"), Dialect::CommonLisp, &tree)
            .expect("build eval-when-body-never-runs report")
    }

    fn findings(input: &str) -> Vec<EvalWhenBodyNeverRunsItem> {
        report(input).findings
    }

    fn candidates(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "eval_when_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("eval_when_count in the summary")
    }

    // --- positive: the shape SBCL says nothing about

    #[test]
    fn flags_a_nested_compile_toplevel_only_eval_when() {
        let found = findings(
            "(defun run ()\n  (eval-when (:compile-toplevel) (setf *m* :fired))\n  :done)\n",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].situations, ":compile-toplevel");
        assert_eq!(found[0].body_form_count, 1);
    }

    #[test]
    fn flags_a_nested_compile_and_load_toplevel_eval_when() {
        let found =
            findings("(defun run () (eval-when (:compile-toplevel :load-toplevel) (f) (g)))\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].situations, ":compile-toplevel :load-toplevel");
        assert_eq!(found[0].body_form_count, 2);
    }

    #[test]
    fn flags_the_deprecated_spellings_too() {
        assert_eq!(
            findings("(defun run () (eval-when (compile) (f)))\n").len(),
            1
        );
    }

    #[test]
    fn flags_it_inside_any_ordinary_binding_form() {
        for source in [
            "(let () (eval-when (:compile-toplevel) (f)))",
            "(lambda () (eval-when (:load-toplevel) (f)))",
            "(when *flag* (eval-when (:compile-toplevel) (f)))",
            "(defmacro m () (eval-when (:compile-toplevel) (f)))",
        ] {
            assert_eq!(findings(source).len(), 1, "missed: {source}");
        }
    }

    /// A `macrolet`'s *bindings* are not its body, so an `eval-when` there is
    /// not a top level form and is correctly flagged.
    #[test]
    fn flags_it_in_a_non_body_position_of_a_top_level_operator() {
        assert_eq!(
            findings("(macrolet ((m () (eval-when (:compile-toplevel) (f)))) 1)").len(),
            1
        );
    }

    // --- negatives: the shapes that are correct

    /// The whole point of the rule: at top level these situations are honoured.
    #[test]
    fn does_not_flag_a_top_level_eval_when() {
        assert!(findings("(eval-when (:compile-toplevel) (defmacro m () 1))\n").is_empty());
        assert!(findings("(eval-when (:compile-toplevel :load-toplevel) (f))\n").is_empty());
    }

    /// CLHS 3.2.3.1's recursion prevents false positives in these forms.
    #[test]
    fn does_not_flag_inside_the_top_level_preserving_operators() {
        for source in [
            "(progn (eval-when (:compile-toplevel) (f)))",
            "(locally (eval-when (:compile-toplevel) (f)))",
            "(macrolet () (eval-when (:compile-toplevel) (f)))",
            "(symbol-macrolet () (eval-when (:compile-toplevel) (f)))",
            "(eval-when (:compile-toplevel) (eval-when (:compile-toplevel) (f)))",
            "(progn (progn (eval-when (:compile-toplevel) (f))))",
        ] {
            assert!(findings(source).is_empty(), "false positive on: {source}");
        }
    }

    /// A nested `eval-when` naming `:execute` is the correct spelling there.
    #[test]
    fn does_not_flag_a_nested_eval_when_that_names_execute() {
        assert!(findings("(defun run () (eval-when (:execute) (f)))\n").is_empty());
        assert!(
            findings("(defun run () (eval-when (:compile-toplevel :execute) (f)))\n").is_empty()
        );
        assert!(findings("(defun run () (eval-when (eval) (f)))\n").is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_situation_list_or_an_empty_body() {
        assert!(findings("(defun run () (eval-when () (f)))\n").is_empty());
        assert!(findings("(defun run () (eval-when (:compile-toplevel)))\n").is_empty());
    }

    #[test]
    fn does_not_flag_a_reader_conditional_situation_list() {
        assert!(findings("(defun run () (eval-when (#+sbcl :compile-toplevel) (f)))\n").is_empty());
    }

    // --- quote negatives

    #[test]
    fn quoted_data_is_never_a_finding() {
        for source in [
            "'(defun run () (eval-when (:compile-toplevel) (f)))",
            "(quote (eval-when (:compile-toplevel) (f)))",
            "`(defun run () (eval-when (:compile-toplevel) (f)))",
            "'(a ,(eval-when (:compile-toplevel) (f)))",
        ] {
            assert!(findings(source).is_empty(), "flagged data: {source}");
        }
    }

    /// A macro that emits this shape into its *template* is writing code for
    /// somewhere else, where it may well be top level.
    #[test]
    fn a_template_inside_a_macro_body_is_data_and_is_not_flagged() {
        assert!(findings("(defmacro m () `(eval-when (:compile-toplevel) (f)))\n").is_empty());
    }

    // --- denominator and envelope

    #[test]
    fn the_denominator_counts_every_eval_when_reached_as_code() {
        assert_eq!(
            candidates(
                "(eval-when (:compile-toplevel) (f))\n(defun g () (eval-when (:compile-toplevel) (h)))\n"
            ),
            2
        );
        assert_eq!(candidates("(defun f () 1)\n"), 0);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(eval-when (:compile-toplevel) 1)", Dialect::Clojure)
                .expect("parse");
        let report =
            build_eval_when_body_never_runs_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_fields() {
        let report = report("\n(defun run () (eval-when (:compile-toplevel) (f)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "eval-when-body-never-runs");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("situations", json!(":compile-toplevel")),
                ("body_form_count", json!(1)),
            ]
        );
        assert!(finding.message().contains("never run"));
    }

    /// Singular and plural agreement, since the message names a variable number
    /// of situations.
    #[test]
    fn the_message_agrees_with_the_number_of_situations_named() {
        let one = findings("(defun run () (eval-when (:compile-toplevel) (f)))\n");
        assert!(one[0].message().contains(":compile-toplevel is ignored"));
        let two = findings("(defun run () (eval-when (:compile-toplevel :load-toplevel) (f)))\n");
        assert!(
            two[0]
                .message()
                .contains(":compile-toplevel :load-toplevel are ignored")
        );
    }
}
