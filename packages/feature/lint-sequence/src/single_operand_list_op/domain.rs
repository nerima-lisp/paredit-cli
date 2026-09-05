//! Common Lisp single-operand list-op detection: a one-argument call to
//! `append`, `nconc`, or `list*`. Each of these returns its *last* argument
//! unchanged, and with a single argument that argument is the last — so
//! `(append x)` is exactly `x`, `(nconc x)` is `x`, and `(list* x)` is `x`. The
//! single-argument call does nothing.
//!
//! Scope is limited to these three operators precisely because their
//! single-argument identity is *unconditional*: `(append x)` returns `x` for
//! any object (`(append 5)` is `5`), imposing no type constraint. Numeric n-ary
//! operators like `logand`/`max` are deliberately excluded — `(max x)` requires
//! `x` to be a real and would signal an error a bare `x` would not, so dropping
//! the wrapper there would change behavior. (`+`/`*` and `and`/`or` are covered
//! by `single-operand-arithmetic` and `single-operand-boolean`.)
//!
//! Only the exact two-element shape `(op x)` is flagged; a zero-argument
//! `(append)` (which is `nil`) and a reader-conditional operand are left alone.
//!
//! The fix replaces the whole form with the argument's source, so the rule is
//! auto-fixable.
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

/// The one-argument-identity n-ary list operators.
const LIST_OP_HEADS: [&str; 3] = ["append", "nconc", "list*"];

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// A `,@`-spliced operand, which is the reader conditional's problem pointed
/// the other way: `` `(or ,@predicates) `` has however many operands
/// `predicates` turns out to hold at expansion time, which is zero or three
/// just as easily as one. The parser sees one child and the arity premise reads
/// as satisfied, but nothing in the source settles it.
///
/// It is also unwritable as a rewrite even when it really is one operand —
/// `` `,@predicates `` is not a well-formed backquote expression and SBCL
/// refuses to read it — so this removes findings whose fix had no valid output.
fn is_spliced(view: &ExpressionView) -> bool {
    view.reader_prefixes
        .contains(&ReaderPrefix::UnquoteSplicing)
}

#[derive(Debug, Clone)]
pub struct SingleOperandListOpItem {
    /// The span of the whole `(op x)` form.
    pub span: ByteSpan,
    /// The span of the sole argument `x`.
    ///
    pub arg_span: ByteSpan,
    /// The operator name (`append`/`nconc`/`list*`), for the finding message.
    pub head: String,
}

impl Finding for SingleOperandListOpItem {
    /// One tag for every finding. The operator is *not* the tag: it is
    /// `list_head`'s text, so it carries the source's own casing — an
    /// `(APPEND xs)` reports `APPEND` — which makes it data to print rather
    /// than a class to filter on.
    fn kind(&self) -> &'static str {
        "single-operand-list-op"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.head.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("head", json!(self.head))]
    }

    fn message(&self) -> String {
        let head = &self.head;
        format!("{head} of one argument returns it unchanged; ({head} x) is x")
    }
}

pub fn examine_form(
    view: &ExpressionView,
    list_op_form_count: &mut usize,
    violations: &mut Vec<SingleOperandListOpItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !LIST_OP_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    *list_op_form_count += 1;

    // children: [op, arg] — require exactly one argument.
    if view.children.len() != 2 {
        return;
    }
    let arg = &view.children[1];
    if is_reader_conditional(arg) {
        return;
    }
    if is_spliced(arg) {
        return;
    }

    violations.push(SingleOperandListOpItem {
        span: view.span,
        arg_span: arg.span,
        head: head.to_owned(),
    });
}

/// Collects every single-argument `append`/`nconc`/`list*` in one file, with
/// the number of such forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_single_operand_list_op_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SingleOperandListOpItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("list_op_form_count", json!(0))],
        ));
    }

    let mut list_op_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_form(subview, &mut list_op_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("list_op_form_count", json!(list_op_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SingleOperandListOpItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_single_operand_list_op_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build single-operand list op report")
    }

    fn forms(input: &str) -> (u64, Vec<SingleOperandListOpItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "list_op_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("list_op_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_single_arg_append() {
        let source = "(append xs)";
        let (count, violations) = forms(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "append");
        assert_eq!(slice(source, violations[0].arg_span), "xs");
    }

    #[test]
    fn flags_single_arg_nconc() {
        let (_, violations) = forms("(nconc items)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "nconc");
    }

    #[test]
    fn flags_single_arg_list_star() {
        let (_, violations) = forms("(list* tail)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "list*");
    }

    #[test]
    fn preserves_a_compound_argument() {
        let source = "(append (mapcar #'f xs))";
        let (_, violations) = forms(source);
        assert_eq!(slice(source, violations[0].arg_span), "(mapcar #'f xs)");
    }

    #[test]
    fn does_not_flag_two_arguments() {
        let (count, violations) = forms("(append a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_zero_arguments() {
        let (count, violations) = forms("(append)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_numeric_op() {
        // (max x) requires a real; not covered here.
        let (count, violations) = forms("(max x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = forms("(APPEND xs)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_form() {
        let (_, violations) = forms("(defun f (xs) (nconc xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(append xs)", Dialect::Clojure).expect("parse");
        let report =
            build_single_operand_list_op_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build single-operand list op report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("list_op_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(append a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_operator() {
        let report = report("(defun f (xs)\n  (nconc xs))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "single-operand-list-op");
        assert_eq!(finding.json_fields(), vec![("head", json!("nconc"))]);
        assert_eq!(finding.text_columns(), vec!["nconc".to_owned()]);
    }

    /// The operator is reported as written, which is why it is a column rather
    /// than the kind.
    #[test]
    fn the_operator_keeps_the_source_casing() {
        let report = report("(APPEND xs)");
        assert_eq!(report.findings[0].head, "APPEND");
    }

    #[test]
    fn the_summary_counts_every_list_op_scanned_not_only_the_flagged_ones() {
        let report = report("(append xs)\n(append a b)\n(nconc items)\n");
        assert_eq!(report.summary, vec![("list_op_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
