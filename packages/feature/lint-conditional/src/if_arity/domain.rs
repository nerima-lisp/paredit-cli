//! Common Lisp `if`-arity detection: an `if` special form with the wrong
//! number of arguments. Common Lisp's `if` is `(if test then [else])` — it
//! takes exactly two or three argument forms. Fewer (`(if test)`) or more
//! (`(if test then else extra)`) is a program error, caught only at
//! macroexpansion or compile time rather than by the reader. The classic bug
//! is treating the `else` position as an implicit `progn`: `(if c a b d)`
//! silently drops or errors instead of running `b` then `d`.
//!
//! Scope: Common Lisp only, and for good reason — Emacs Lisp's `if` *does*
//! take an implicit-progn else (`(if c then else...)`), so `(if a b c d)` is
//! valid there. Restricting to Common Lisp keeps "more than three arguments is
//! malformed" a provable claim.
//!
//! Forms whose written arity may differ from their evaluated arity are skipped
//! to avoid false positives: a quoted/quasiquoted/unquoted `if` (data or a
//! template, not a call), or any `if` with a child carrying a reader
//! conditional (`#+`/`#-` expand to zero or one form) or a splicing unquote
//! (`,@` splices an unknown number of forms).
//!

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// Whether a child form can change how many argument forms actually reach the
/// evaluator, making a static arity count unreliable. Two cases matter:
///
/// * A `,@` splice in a template (or a Clojure `#?`/`#?@` reader conditional),
///   which the reader models as a prefix on the following form.
/// * A Common Lisp `#+`/`#-` reader conditional, which expands to zero or one
///   form. Depending on the parse mode the reader models it either as a bare
///   `#+`/`#-` marker atom or as a single atom whose text begins `#+`/`#-`
///   (e.g. `#+sbcl c`); both start with the marker, so `starts_with` covers
///   the pair.
fn is_arity_ambiguous_child(view: &ExpressionView) -> bool {
    let ambiguous_prefix = view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
                | ReaderPrefix::UnquoteSplicing
        )
    });
    ambiguous_prefix
        || atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct IfArityItem {
    /// The span of the whole misarity `if` form.
    pub span: ByteSpan,
    pub argument_count: usize,
}

impl Finding for IfArityItem {
    /// The rule's own name. The discriminator here is a *number* of arguments,
    /// which is unbounded and not identifier-like, so it stays a JSON field and
    /// a column rather than becoming the kind.
    fn kind(&self) -> &'static str {
        "if-arity"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("arguments={}", self.argument_count)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("argument_count", json!(self.argument_count))]
    }

    fn message(&self) -> String {
        format!("if takes 2 or 3 arguments but has {}", self.argument_count)
    }
}

pub fn examine_if(
    view: &ExpressionView,
    if_form_count: &mut usize,
    violations: &mut Vec<IfArityItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("if")) {
        return;
    }
    // A quoted/quasiquoted/unquoted `if` is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    // A child that expands to zero-or-one form (`#+`/`#-`) or splices an
    // unknown number of forms (`,@`) makes the written arity unreliable.
    if view.children.iter().any(is_arity_ambiguous_child) {
        return;
    }
    *if_form_count += 1;

    // children = [if, test, then, else?]; the head does not count.
    let argument_count = view.children.len() - 1;
    if !(2..=3).contains(&argument_count) {
        violations.push(IfArityItem {
            span: view.span,
            argument_count,
        });
    }
}

/// Collects every misarity `if` form in one file, with the number of `if` forms
/// scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_if_arity_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<IfArityItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("if_form_count", json!(0))],
        ));
    }

    let mut if_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_if(subview, &mut if_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("if_form_count", json!(if_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<IfArityItem> {
        // Use the dialect-aware parse the CLI path uses: it groups Common Lisp
        // `#+`/`#-` reader conditionals differently than the default reader.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_if_arity_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build if arity report")
    }

    fn violations(input: &str) -> (u64, Vec<IfArityItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "if_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("if_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_too_many_arguments() {
        let (if_form_count, items) = violations("(if a b c d)");
        assert_eq!(if_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 4);
    }

    #[test]
    fn flags_too_few_arguments() {
        let (_, items) = violations("(if a)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 1);
    }

    #[test]
    fn does_not_flag_a_two_argument_if() {
        let (if_form_count, items) = violations("(if a b)");
        assert_eq!(if_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_three_argument_if() {
        let (_, items) = violations("(if a b c)");
        assert!(items.is_empty());
    }

    #[test]
    fn skips_an_if_with_a_reader_conditional_else() {
        // Only one of the two feature branches survives at read time, so the
        // written four-argument shape is not a real arity error.
        let (if_form_count, items) = violations("(if a b #+sbcl c #-sbcl d)");
        assert_eq!(if_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_if_form() {
        let (if_form_count, items) = violations("(list '(if a b c d))");
        assert_eq!(if_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_an_if_nested_in_a_function_body() {
        let (if_form_count, items) = violations("(defun f (x) (if x 1 2 3))");
        assert_eq!(if_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 4);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        // Emacs Lisp `if` has an implicit-progn else, so this is valid there.
        let tree = SyntaxTree::parse_with_dialect("(if a b c d)", Dialect::EmacsLisp)
            .expect("parse input");
        let report = build_if_arity_report(Path::new("app.el"), Dialect::EmacsLisp, &tree)
            .expect("build if arity report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("if_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(if a b c)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_argument_count() {
        let report = report("(defun f (x)\n  (if x 1 2 3))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "if-arity");
        assert_eq!(finding.json_fields(), vec![("argument_count", json!(4))]);
        assert_eq!(finding.text_columns(), vec!["arguments=4".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_if_scanned_not_only_the_flagged_ones() {
        let report = report("(if a b c d)\n(if a b)\n(if a b c)\n");
        assert_eq!(report.summary, vec![("if_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
