//! Common Lisp eval-when-situation detection: an `eval-when` whose situation
//! list names a situation that is not one of the valid keywords. The only
//! valid situations are `:compile-toplevel`, `:load-toplevel`, `:execute`, and
//! the deprecated pre-ANSI names `compile`, `load`, `eval`. A typo like
//! `(eval-when (:executee) …)` or `(eval-when (:compile-top-level) …)` compiles
//! without complaint but silently never runs its body at the intended time — a
//! subtle bug the compiler does not catch.
//!
//! Only the situation list (child 1) is inspected. Situations guarded by a
//! `#+`/`#-` reader conditional are skipped, as is a quoted/quasiquoted
//! `eval-when` (data, not a call).
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// Whether a situation atom is one CLHS accepts.
fn is_valid_situation(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        ":compile-toplevel" | ":load-toplevel" | ":execute" | "compile" | "load" | "eval"
    )
}

/// Whether a situation-list element is a `#+`/`#-` reader conditional, whose
/// presence makes the situation not statically checkable.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    let conditional_prefix = view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional | ReaderPrefix::ReaderConditionalSplicing
        )
    });
    conditional_prefix
        || atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct EvalWhenSituationItem {
    /// The span of the offending situation.
    pub span: ByteSpan,
    /// The 1-based line the situation starts on.
    pub line: usize,
    /// The invalid situation as written, or `()` when it is not an atom.
    pub situation: String,
}

impl Finding for EvalWhenSituationItem {
    /// The rule's own name. The situation is the misspelling itself — an open
    /// set, not a vocabulary a consumer could filter on — so it is a field
    /// rather than a variant.
    fn kind(&self) -> &'static str {
        "eval-when-situation"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("situation={}", self.situation)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("situation", json!(self.situation))]
    }

    /// The same sentence the `eval-when-situation` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!("eval-when situation {} is not valid", self.situation)
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_eval_when(
    view: &ExpressionView,
    source: &str,
    eval_when_form_count: &mut usize,
    violations: &mut Vec<EvalWhenSituationItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("eval-when")) {
        return;
    }
    // A quoted/quasiquoted eval-when is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    let Some(situations) = view.children.get(1) else {
        return;
    };
    // A non-list situations argument is a different malformation; skip it.
    if !is_paren_list(situations) {
        return;
    }
    *eval_when_form_count += 1;

    for situation in &situations.children {
        if is_reader_conditional(situation) {
            continue;
        }
        // A valid situation is one of the recognized bare atoms; anything else
        // (an unknown keyword, a typo, or a non-atom) is invalid.
        let valid = atom_text(situation).is_some_and(is_valid_situation);
        if !valid {
            violations.push(EvalWhenSituationItem {
                span: situation.span,
                line: line_of(source, situation.span.start().get()),
                situation: atom_text(situation).unwrap_or("()").to_owned(),
            });
        }
    }
}

/// Collects every `eval-when` with an invalid situation in one file, with the
/// number of `eval-when` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every situation is valid here" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_eval_when_situation_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EvalWhenSituationItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("eval_when_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut eval_when_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_eval_when(subview, source, &mut eval_when_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("eval_when_form_count", json!(eval_when_form_count))],
    ))
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<EvalWhenSituationItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_eval_when_situation_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build eval-when situation report")
    }

    /// The `(eval_when_form_count, violations)` pair the report is built from.
    fn violations(input: &str) -> (u64, Vec<EvalWhenSituationItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "eval_when_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("eval_when_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_keyword_typo() {
        let (form_count, items) = violations("(eval-when (:compile-toplevel :executee) 1)");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].situation, ":executee");
    }

    #[test]
    fn flags_a_non_keyword_typo() {
        let (_, items) = violations("(eval-when (excute) 1)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].situation, "excute");
    }

    #[test]
    fn does_not_flag_valid_keyword_situations() {
        let (form_count, items) =
            violations("(eval-when (:compile-toplevel :load-toplevel :execute) 1)");
        assert_eq!(form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_deprecated_situations() {
        let (_, items) = violations("(eval-when (compile load eval) 1)");
        assert!(items.is_empty());
    }

    #[test]
    fn folds_situation_case() {
        let (_, items) = violations("(eval-when (:Execute :COMPILE-TOPLEVEL) 1)");
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_reader_conditional_situation() {
        let (form_count, items) = violations("(eval-when (#+sbcl :execute) 1)");
        assert_eq!(form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_eval_when() {
        let (form_count, items) = violations("(list '(eval-when (:bad) 1))");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn flags_a_typo_among_valid_situations() {
        let (_, items) = violations("(eval-when (:execute :bogus) 1)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].situation, ":bogus");
    }

    #[test]
    fn does_not_flag_a_non_list_situations_argument() {
        let (form_count, items) = violations("(eval-when :execute 1)");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_an_eval_when_nested_in_a_body() {
        let (form_count, items) = violations("(progn (eval-when (:loud) 1))");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(eval-when (:bad) 1)", Dialect::Clojure)
            .expect("parse input");
        let report =
            build_eval_when_situation_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build eval-when situation report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("eval_when_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(eval-when (:execute) 1)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_situation() {
        let report = report("(progn\n  (eval-when (:bogus) 1))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "eval-when-situation");
        assert_eq!(finding.json_fields(), vec![("situation", json!(":bogus"))]);
        assert_eq!(finding.text_columns(), vec!["situation=:bogus".to_owned()]);
        assert_eq!(finding.message(), "eval-when situation :bogus is not valid");
    }

    #[test]
    fn the_summary_counts_every_eval_when_scanned_not_only_the_flagged_ones() {
        let report = report("(eval-when (:bogus) 1)\n(eval-when (:execute) 2)\n");
        assert_eq!(report.summary, vec![("eval_when_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
