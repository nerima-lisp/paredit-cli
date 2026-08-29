//! A top-level `eval-when` that names `:execute` and neither top-level
//! situation, wrapped around a definition.
//!
//! ```lisp
//! (eval-when (:execute)
//!   (defmacro m (x) `(* ,x 10)))
//!
//! (defun run () (m 4))
//! ```
//!
//! `load`ing that file works and returns 40. `compile-file`ing it and loading
//! the fasl does not: CLHS 3.2.3.1 says a top-level `eval-when` naming neither
//! `:compile-toplevel` nor `:load-toplevel` is **not processed at all** by the
//! file compiler, so the `defmacro` never reaches the compiler and never reaches
//! the fasl. `(m 4)` compiles as a call to an undefined function and fails at
//! run time.
//!
//! Verified against SBCL 2.6.0, both phases, on exactly that file:
//!
//! - `(load "src.lisp")` → `40`, no diagnostic at all.
//! - `(load (compile-file "src.lisp"))` → one **style** warning, `undefined
//!   function: M`, `warnings-p` and `failure-p` both `NIL` — so the build is
//!   *green* — then `UNDEFINED-FUNCTION: The function M is undefined` when the
//!   code is finally called.
//!
//! That gap is the entire point of the rule. The compile is reported as
//! successful, and the failure surfaces later, somewhere else, as a missing
//! function rather than as a phase mistake.
//!
//! # Why the situation set is exactly this one
//!
//! The four other shapes were measured on the same file and are **not** flagged:
//!
//! | situations | `load` | `compile-file` + load fasl |
//! | --- | --- | --- |
//! | *(no `eval-when` at all)* | 40 | 40 |
//! | `:compile-toplevel :load-toplevel :execute` | 40 | 40 |
//! | `:load-toplevel :execute` | 40 | **40** |
//! | `:execute` | 40 | **undefined function** |
//!
//! The third row is the surprising one, and it is why this rule does *not* ask
//! "does the situation list contain `:compile-toplevel`?". It does not need to:
//! `defmacro`'s own expansion contains an inner `(eval-when (:compile-toplevel)
//! …)`, and CLHS 3.2.3.1 keeps the body of a top-level `eval-when` top level, so
//! the inner one still runs at compile time. Omitting `:compile-toplevel` is
//! therefore harmless as long as `:load-toplevel` is present. A rule written
//! against the obvious predicate would have fired on every
//! `(eval-when (:load-toplevel :execute) …)` in the world, which is a correct
//! and common idiom.
//!
//! What actually matters is whether the body reaches the compiler **at all**,
//! which is `EvalWhenSituations::reaches_the_compiler`.
//!
//! # Deliberate limits
//!
//! - **`:execute` must be named.** `(eval-when () …)` also reaches no
//!   compiler, but it does nothing under `load` either, so it is dead in every
//!   phase rather than phase-dependent — a different and much more obvious
//!   defect. Requiring `:execute` keeps the rule's name exactly true of what it
//!   reports.
//! - **The body must contain a definition.** `(eval-when (:execute) (format t
//!   "interactive only"))` is a judgement about intent — a form deliberately
//!   confined to the interpreter. A *definition* that silently does not exist in
//!   the compiled file is not a judgement call.
//! - **Top level only**, in the CLHS 3.2.3.1 sense, which includes the body of
//!   a top-level `progn`/`locally`/`macrolet`/`symbol-macrolet`/`eval-when`.
//!   Nested inside a `let` or a `defun`, `:compile-toplevel` and
//!   `:load-toplevel` are ignored by the standard and `(eval-when (:execute) …)`
//!   is the *correct* spelling — flagging it there is a false positive, and
//!   [`crate::eval_when_body_never_runs`] is the rule about that context.
//! - **No fix.** Which situations were meant cannot be recovered from the
//!   source: `:load-toplevel :execute` and the full three are both plausible,
//!   and so is deleting the `eval-when`.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head};
use serde_json::{Value, json};

use crate::support::{
    for_each_evaluated_subview, is_definition_form, is_top_level_form, mentions, normalized_symbol,
    read_situations,
};

/// The byte-scan needle, and the head this rule anchors on.
pub const EVAL_WHEN: &str = "eval-when";

/// Where an `eval-when`'s body begins: head, situations, then forms.
const BODY_START: usize = 2;

#[derive(Debug, Clone)]
pub struct EvalWhenExecuteOnlyItem {
    /// The span of the whole `eval-when`, which is what would have to change.
    pub span: ByteSpan,
    /// The definition the compiled file will not contain, named as written.
    pub definition: String,
    /// How many definition forms the body holds. All of them are lost, not just
    /// the one named.
    pub definition_count: usize,
}

impl Finding for EvalWhenExecuteOnlyItem {
    fn kind(&self) -> &'static str {
        "eval-when-execute-only"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("definition={}", self.definition),
            format!("definition_count={}", self.definition_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("definition", json!(self.definition)),
            ("definition_count", json!(self.definition_count)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "this eval-when names :execute and neither :compile-toplevel nor :load-toplevel, so \
             compile-file discards its body entirely and the compiled file will not contain {} \
             (loading the source works, which is what hides it)",
            self.definition
        )
    }
}

/// Whether `view` is an `(eval-when …)` form.
#[must_use]
pub fn is_eval_when(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| normalized_symbol(head) == EVAL_WHEN)
}

/// The definitions in an `eval-when`'s body, as written.
fn body_definitions(view: &ExpressionView) -> Vec<String> {
    view.children
        .get(BODY_START..)
        .unwrap_or_default()
        .iter()
        .filter(|form| is_definition_form(form))
        .map(|form| {
            let head = list_head(form).unwrap_or("?");
            let name = form.children.get(1).and_then(atom_text).unwrap_or("");
            if name.is_empty() {
                format!("({head} …)")
            } else {
                format!("({head} {name} …)")
            }
        })
        .collect()
}

/// The finding this `eval-when` implies, if any.
///
/// **Ordering is load-bearing.** The situation list and the body are read from
/// `view` alone — no tree access, no allocation beyond the situation read — and
/// they reject all but a handful of forms per file. Only then does
/// [`is_top_level_form`] touch the tree, which costs the enclosing top-level
/// form. Asking the top-level question first would pay that cost for every
/// `eval-when` in the corpus, including the overwhelming majority that name all
/// three situations and cannot produce a finding at all.
#[must_use]
pub fn examine_eval_when(
    tree: &SyntaxTree,
    view: &ExpressionView,
) -> Option<EvalWhenExecuteOnlyItem> {
    // 1. node-local: the situation list.
    let situations = read_situations(view.children.get(1)?)?;
    if !situations.execute || situations.reaches_the_compiler() {
        return None;
    }
    // 2. node-local: is anything actually lost?
    let definitions = body_definitions(view);
    let definition = definitions.first()?.clone();
    // 3. only now, the tree.
    if !is_top_level_form(tree, view.span) {
        return None;
    }
    Some(EvalWhenExecuteOnlyItem {
        span: view.span,
        definition,
        definition_count: definitions.len(),
    })
}

/// Every `eval-when` in the file, reached as evaluated code, with its finding.
fn collect(tree: &SyntaxTree) -> (Vec<EvalWhenExecuteOnlyItem>, usize) {
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
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every eval-when here reaches the
/// compiler" for Common Lisp and "nothing was looked for" for Clojure, and the
/// two read identically without the flag.
pub fn build_eval_when_execute_only_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EvalWhenExecuteOnlyItem>> {
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
    // The byte scan settles every file that never says `eval-when` without
    // materializing the document at all.
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

    fn report(input: &str) -> FileFindings<EvalWhenExecuteOnlyItem> {
        // `parse_with_dialect`, never the legacy `SyntaxTree::parse`.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_eval_when_execute_only_report(Path::new("app.lisp"), Dialect::CommonLisp, &tree)
            .expect("build eval-when-execute-only report")
    }

    fn findings(input: &str) -> Vec<EvalWhenExecuteOnlyItem> {
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

    // --- positive: the shape SBCL loses under compile-file

    #[test]
    fn flags_execute_only_around_a_defmacro() {
        let found = findings("(eval-when (:execute)\n  (defmacro m (x) `(* ,x 10)))\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].definition, "(defmacro m …)");
        assert_eq!(found[0].definition_count, 1);
    }

    #[test]
    fn flags_execute_only_around_a_defconstant() {
        // Verified separately against SBCL: `(eval-when (:execute) (defconstant
        // +k+ 41))` then `(defun plus1 () (1+ +k+))` loads fine and fails to
        // compile with `undefined variable: +K+`.
        let found = findings("(eval-when (:execute) (defconstant +k+ 41))\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].definition, "(defconstant +k+ …)");
    }

    #[test]
    fn flags_the_deprecated_eval_spelling() {
        assert_eq!(findings("(eval-when (eval) (defmacro m () 1))\n").len(), 1);
    }

    #[test]
    fn counts_every_definition_the_compiled_file_will_lack() {
        let found = findings(
            "(eval-when (:execute)\n  (defmacro m () 1)\n  (defun f () 2)\n  (defconstant +c+ 3))\n",
        );
        assert_eq!(found[0].definition_count, 3);
        assert_eq!(found[0].definition, "(defmacro m …)");
    }

    /// CLHS 3.2.3.1: the body of a top-level `progn` is itself top level, and
    /// SBCL 2.6.0 loses this exactly as it loses the depth-0 spelling.
    #[test]
    fn flags_the_top_level_preserving_nestings() {
        for source in [
            "(progn (eval-when (:execute) (defmacro m () 1)))",
            "(locally (eval-when (:execute) (defmacro m () 1)))",
            "(macrolet () (eval-when (:execute) (defmacro m () 1)))",
            "(symbol-macrolet () (eval-when (:execute) (defmacro m () 1)))",
        ] {
            assert_eq!(findings(source).len(), 1, "missed: {source}");
        }
    }

    // --- negatives: the shapes that are correct

    /// The row that kills the obvious "missing :compile-toplevel" predicate.
    #[test]
    fn does_not_flag_load_toplevel_and_execute() {
        assert!(findings("(eval-when (:load-toplevel :execute) (defmacro m () 1))\n").is_empty());
    }

    #[test]
    fn does_not_flag_the_full_three_situations() {
        assert!(
            findings("(eval-when (:compile-toplevel :load-toplevel :execute) (defmacro m () 1))\n")
                .is_empty()
        );
        assert!(findings("(eval-when (compile load eval) (defmacro m () 1))\n").is_empty());
    }

    #[test]
    fn does_not_flag_compile_toplevel_alone() {
        assert!(findings("(eval-when (:compile-toplevel) (defmacro m () 1))\n").is_empty());
    }

    /// A body with no definition loses nothing a later form can miss.
    #[test]
    fn does_not_flag_execute_only_around_a_plain_call() {
        assert!(findings("(eval-when (:execute) (format t \"interactive only~%\"))\n").is_empty());
        assert!(findings("(eval-when (:execute) (setf *debug* t))\n").is_empty());
    }

    /// Names nothing, so it is dead in every phase rather than phase-dependent.
    #[test]
    fn does_not_flag_an_empty_situation_list() {
        assert!(findings("(eval-when () (defmacro m () 1))\n").is_empty());
    }

    /// Nested inside a binding form, `:compile-toplevel` and `:load-toplevel`
    /// are ignored by the standard, so `(eval-when (:execute) …)` is the
    /// *correct* spelling and flagging it is a false positive.
    #[test]
    fn does_not_flag_a_non_top_level_eval_when() {
        for source in [
            "(let () (eval-when (:execute) (defmacro m () 1)))",
            "(defun f () (eval-when (:execute) (defmacro m () 1)))",
            "(when *flag* (eval-when (:execute) (defmacro m () 1)))",
        ] {
            assert!(findings(source).is_empty(), "false positive on: {source}");
        }
    }

    /// The situations list of an outer `eval-when` is not one of its body forms.
    #[test]
    fn does_not_flag_a_candidate_in_a_non_body_position() {
        assert!(
            findings("(macrolet ((m () (eval-when (:execute) (defmacro q () 1)))) 2)").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_reader_conditional_situation_list() {
        // `#+sbcl :compile-toplevel` folds into one atom, so the situation set
        // cannot be read exactly. Saying nothing beats guessing it absent.
        assert!(
            findings("(eval-when (#+sbcl :compile-toplevel :execute) (defmacro m () 1))\n")
                .is_empty()
        );
    }

    // --- quote/quasiquote negatives (the five shapes)

    #[test]
    fn quoted_data_is_never_a_finding() {
        for source in [
            "'(eval-when (:execute) (defmacro m () 1))",
            "(quote (eval-when (:execute) (defmacro m () 1)))",
            "'(a ,(eval-when (:execute) (defmacro m () 1)))",
            "`(eval-when (:execute) (defmacro m () 1))",
        ] {
            assert!(findings(source).is_empty(), "flagged data: {source}");
        }
    }

    /// A macro that *emits* an `eval-when` is writing a template, not running
    /// one; the comma is what makes the difference.
    #[test]
    fn an_unquoted_eval_when_inside_a_backquote_is_code_but_is_not_top_level() {
        assert!(findings("`(a ,(eval-when (:execute) (defmacro m () 1)))").is_empty());
    }

    #[test]
    fn a_form_inside_a_string_literal_is_one_atom() {
        assert!(findings("(format t \"(eval-when (:execute) (defmacro m () 1))\")").is_empty());
        assert_eq!(
            candidates("(format t \"(eval-when (:execute) (defmacro m () 1))\")"),
            0
        );
    }

    // --- denominator

    #[test]
    fn the_denominator_counts_every_eval_when_reached_as_code() {
        assert_eq!(
            candidates(
                "(eval-when (:compile-toplevel :load-toplevel :execute) (defmacro a () 1))\n\
                 (eval-when (:execute) (defmacro b () 2))\n\
                 (let () (eval-when (:execute) (defmacro c () 3)))\n"
            ),
            3
        );
    }

    #[test]
    fn a_file_that_never_says_eval_when_has_a_zero_denominator() {
        assert_eq!(candidates("(defun f () 1)\n"), 0);
        assert_eq!(candidates(""), 0);
    }

    // --- envelope

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(eval-when (:execute) 1)", Dialect::Clojure)
            .expect("parse");
        let report =
            build_eval_when_execute_only_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("eval_when_count", json!(0))]);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_fields() {
        let report = report("\n(eval-when (:execute) (defmacro m () 1))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "eval-when-execute-only");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("definition", json!("(defmacro m …)")),
                ("definition_count", json!(1)),
            ]
        );
        assert!(finding.message().contains("compile-file discards"));
    }
}
