//! Common Lisp modify-macro-arity detection: a place-modifying macro called
//! with the wrong number of arguments. `incf` and `decf` are
//! `(incf place [delta])` — one or two arguments; `push` is
//! `(push item place)` — exactly two; `pop` is `(pop place)` — exactly one. A
//! wrong argument count (`(incf x 1 2)`, `(push a)`, `(pop)`) is a program
//! error, caught at macroexpansion rather than by the reader.
//!
//! Scoped to these fixed-arity macros on purpose: `pushnew` and the general
//! `setf`/`setq` family take a variable number of arguments (keyword options
//! or place/value pairs) and are handled elsewhere
//! (`setf-arity`).
//!
//! Forms whose argument count is not statically visible are skipped to avoid
//! false positives: a quoted/quasiquoted form (data or a template), and any
//! call with a `#+`/`#-` reader conditional or `,@` splice argument, where the
//! written count differs from the evaluated one.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// The canonical name and the inclusive `(min, max)` argument arity of a
/// place-modifying macro, or `None` if `head` is not one this rule checks.
///
/// The name is returned alongside the arity rather than recomputed, so the
/// canonical spelling and the arity it belongs to cannot drift apart.
fn expected_arity(head: &str) -> Option<(&'static str, usize, usize)> {
    match head.to_ascii_lowercase().as_str() {
        "incf" => Some(("incf", 1, 2)),
        "decf" => Some(("decf", 1, 2)),
        "push" => Some(("push", 2, 2)),
        "pop" => Some(("pop", 1, 1)),
        _ => None,
    }
}

/// Whether an argument's reader prefix or `#+`/`#-` marker makes the static
/// argument count unreliable.
fn is_arity_ambiguous(view: &ExpressionView) -> bool {
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

fn arity_phrase(min: usize, max: usize) -> String {
    if min == max {
        format!("exactly {min}")
    } else {
        format!("{min} or {max}")
    }
}

#[derive(Debug, Clone)]
pub struct ModifyMacroArityItem {
    pub span: ByteSpan,
    /// The macro's canonical lowercase name, which is a closed set of four.
    pub canonical_operator: &'static str,
    /// The macro as it is spelled in the source, whose case is not folded.
    pub operator: String,
    pub argument_count: usize,
    pub min_arity: usize,
    pub max_arity: usize,
}

impl Finding for ModifyMacroArityItem {
    /// The macro's canonical name rather than its source spelling: `kind` is a
    /// selector, and `(INCF …)` and `(incf …)` are the same mistake. The
    /// spelling as written stays in the `operator` field.
    fn kind(&self) -> &'static str {
        self.canonical_operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("op={}", self.operator),
            format!("expected={}", expected_arity_phrase(self)),
            format!("arguments={}", self.argument_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("argument_count", json!(self.argument_count)),
            ("min_arity", json!(self.min_arity)),
            ("max_arity", json!(self.max_arity)),
            ("expected", json!(expected_arity_phrase(self))),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} takes {} argument(s) but has {}",
            self.operator,
            expected_arity_phrase(self),
            self.argument_count
        )
    }
}

pub fn examine_call(
    view: &ExpressionView,
    call_count: &mut usize,
    violations: &mut Vec<ModifyMacroArityItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some((canonical_operator, min_arity, max_arity)) = expected_arity(head) else {
        return;
    };
    // A quoted/quasiquoted/unquoted call is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    // A `#+`/`#-` or `,@` argument makes the written arity unreliable.
    if view.children.iter().skip(1).any(is_arity_ambiguous) {
        return;
    }
    *call_count += 1;

    let argument_count = view.children.len() - 1;
    if !(min_arity..=max_arity).contains(&argument_count) {
        violations.push(ModifyMacroArityItem {
            span: view.span,
            canonical_operator,
            operator: head.to_owned(),
            argument_count,
            min_arity,
            max_arity,
        });
    }
}

/// Collects every misarity modify-macro call in one file, with the number of
/// such calls scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_modify_macro_arity_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ModifyMacroArityItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("call_count", json!(0))],
        ));
    }

    let mut call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(subview, &mut call_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("call_count", json!(call_count))],
    ))
}

/// A human phrase for the expected arity of one violation, e.g. `exactly 2`.
#[must_use]
pub fn expected_arity_phrase(item: &ModifyMacroArityItem) -> String {
    arity_phrase(item.min_arity, item.max_arity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ModifyMacroArityItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_modify_macro_arity_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build modify macro arity report")
    }

    fn violations(input: &str) -> (u64, Vec<ModifyMacroArityItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "call_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("call_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_incf_with_too_many_arguments() {
        let (call_count, items) = violations("(incf x 1 2)");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "incf");
        assert_eq!(items[0].argument_count, 3);
    }

    #[test]
    fn does_not_flag_incf_with_one_or_two_arguments() {
        let (_, one) = violations("(incf x)");
        assert!(one.is_empty());
        let (_, two) = violations("(incf x 2)");
        assert!(two.is_empty());
    }

    #[test]
    fn flags_decf_with_too_many_arguments() {
        let (_, items) = violations("(decf y 1 2)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "decf");
    }

    #[test]
    fn flags_pop_with_too_many_arguments() {
        let (_, items) = violations("(pop stack extra)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 2);
        assert_eq!(expected_arity_phrase(&items[0]), "exactly 1");
    }

    #[test]
    fn flags_pop_with_no_arguments() {
        let (_, items) = violations("(pop)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 0);
    }

    #[test]
    fn does_not_flag_valid_pop() {
        let (call_count, items) = violations("(pop stack)");
        assert_eq!(call_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn flags_push_with_too_few_arguments() {
        let (_, items) = violations("(push item)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "push");
        assert_eq!(expected_arity_phrase(&items[0]), "exactly 2");
    }

    #[test]
    fn does_not_flag_valid_push() {
        let (_, items) = violations("(push item stack)");
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_reader_conditional_argument() {
        let (call_count, items) = violations("(incf x #+sbcl 1 #-sbcl 2)");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_call() {
        let (call_count, items) = violations("(list '(incf x 1 2))");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn folds_operator_case() {
        let (_, items) = violations("(INCF x 1 2)");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn finds_a_call_nested_in_a_function_body() {
        let (call_count, items) = violations("(defun f (x) (incf x 1 2))");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(incf x 1 2)", Dialect::Clojure).expect("parse input");
        let report = build_modify_macro_arity_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build modify macro arity report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(incf x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_every_field_the_report_publishes() {
        let report = report("(defun f (x)\n  (incf x 1 2))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "incf");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!("incf")),
                ("argument_count", json!(3)),
                ("min_arity", json!(1)),
                ("max_arity", json!(2)),
                ("expected", json!("1 or 2")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "op=incf".to_owned(),
                "expected=1 or 2".to_owned(),
                "arguments=3".to_owned(),
            ]
        );
    }

    /// `kind` folds case so a consumer can select on it; `operator` does not,
    /// so the source spelling survives.
    #[test]
    fn the_kind_is_canonical_while_the_operator_keeps_its_source_casing() {
        let report = report("(INCF x 1 2)");
        let finding = &report.findings[0];
        assert_eq!(finding.kind(), "incf");
        assert_eq!(finding.operator, "INCF");
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report("(incf x 1 2)\n(incf y)\n(pop stack)\n");
        assert_eq!(report.summary, vec![("call_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
