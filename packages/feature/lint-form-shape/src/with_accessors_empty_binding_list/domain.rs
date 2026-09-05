//! Common Lisp empty-slot-binding-form detection: a `with-slots` or
//! `with-accessors` whose binding list is `()`.
//!
//! `(with-slots () object body…)` and `(with-accessors () object body…)` are
//! both legal — verified against SBCL, which compiles and runs each without a
//! warning — and both establish no bindings at all. The instance form is still
//! evaluated, so the whole thing is `(progn object body…)` written the long
//! way. It is almost always a binding list someone emptied while editing, or a
//! macro that produced no slots.
//!
//! This is [`crate::empty_let`]'s idiom applied to the two CLOS binding forms.
//! The triggers are disjoint by head: `empty-let` anchors on `let` alone (not
//! even `let*`), this one on `with-slots`/`with-accessors` alone.
//!
//! # What this rule deliberately does not flag
//!
//! - **`with-slots`/`with-accessors` with no instance form at all**
//!   (`(with-slots ())`). That is a malformed form, which is a different
//!   complaint; this rule wants the shape that is *valid* and pointless, so it
//!   requires the instance operand to be present.
//! - **A binding list that is not a `()` list** — a symbol, a `[…]` vector, or
//!   anything a macro produced — since nothing can be concluded about it.
//! - **A form reached only as quoted data.** `'(with-slots () o)` is a list of
//!   symbols, not a binding form. See [`crate::support::is_unevaluated_at`].
//!
//! Report only. Rewriting `(with-slots () o body)` to `(progn o body)` is
//! mechanical in the same way `empty-let`'s rewrite is, but `with-slots` also
//! establishes a `symbol-macrolet` scope that a reader may be relying on for
//! documentation, and this project has a documented history of autofixes
//! silently changing meaning. The judgement is left to a human.
//!
//! Scope: Common Lisp only. Neither operator exists in the other dialects this
//! tool reads.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::for_each_evaluated_subview;

/// The two CLOS binding forms, in the spelling [`list_head`] produces.
pub const BINDING_FORM_HEADS: [&str; 2] = ["with-slots", "with-accessors"];

#[derive(Debug, Clone)]
pub struct WithAccessorsEmptyBindingListItem {
    /// The span of the whole `(with-slots () …)` form.
    pub span: ByteSpan,
    /// The span of the empty `()` binding list, for an editor to jump to.
    pub binding_list_span: ByteSpan,
    /// The operator as written, normalized — `with-slots` or `with-accessors`.
    pub operator: String,
}

impl Finding for WithAccessorsEmptyBindingListItem {
    fn kind(&self) -> &'static str {
        "with-accessors-empty-binding-list"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.operator.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            (
                "binding_list_span",
                json!({
                    "start": self.binding_list_span.start().get(),
                    "end": self.binding_list_span.end().get(),
                }),
            ),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} with no bindings establishes nothing; ({} () object body) is (progn object body)",
            self.operator, self.operator
        )
    }
}

///
/// Cheapest predicate first: the head comparison rejects every node the head
/// index let through for another rule before anything else is read.
pub fn examine(
    view: &ExpressionView,
    binding_form_count: &mut usize,
    violations: &mut Vec<WithAccessorsEmptyBindingListItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &BINDING_FORM_HEADS) {
        return;
    }
    *binding_form_count += 1;

    // `(with-slots BINDINGS INSTANCE …)`: the instance operand must be there,
    // or this is a malformed form rather than an empty one.
    if view.children.len() < 3 {
        return;
    }
    let bindings = &view.children[1];
    if !is_paren_list(bindings) || !bindings.children.is_empty() {
        return;
    }
    // A `'()` binding list is a quoted empty list, not an empty binding list.
    if !bindings.reader_prefixes.is_empty() {
        return;
    }

    violations.push(WithAccessorsEmptyBindingListItem {
        span: view.span,
        binding_list_span: bindings.span,
        operator: paredit_core_syntax::view_query::unqualified(head).to_ascii_lowercase(),
    });
}

/// Collects every `with-slots`/`with-accessors` in one file whose binding list
/// is empty, with the number of such forms scanned as the denominator beside
/// them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_with_accessors_empty_binding_list_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<WithAccessorsEmptyBindingListItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("binding_form_count", json!(0))],
        ));
    }

    let mut binding_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine(subview, &mut binding_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("binding_form_count", json!(binding_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::is_unevaluated_at;

    fn report(input: &str) -> FileFindings<WithAccessorsEmptyBindingListItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_with_accessors_empty_binding_list_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    /// `examine` as the registered rule calls it: on one already-head-matched
    /// node, with the quote check the rule applies afterwards.
    fn fires(source: &str) -> bool {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let mut found = false;
        paredit_core_syntax::view_query::for_each_subview(&root, |view| {
            let mut count = 0;
            let mut items = Vec::new();
            examine(view, &mut count, &mut items);
            if !items.is_empty() && !is_unevaluated_at(&tree, view.span) {
                found = true;
            }
        });
        found
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_an_empty_with_slots() {
        let violations = report("(with-slots () obj (frob))").findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "with-slots");
    }

    #[test]
    fn flags_an_empty_with_accessors() {
        let violations = report("(with-accessors () obj (frob))").findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "with-accessors");
    }

    #[test]
    fn the_binding_list_span_covers_the_empty_parens() {
        let source = "(with-slots () obj (frob))";
        let violations = report(source).findings;
        let span = violations[0].binding_list_span;
        assert_eq!(&source[span.start().get()..span.end().get()], "()");
    }

    #[test]
    fn case_and_package_qualifier_fold() {
        assert_eq!(report("(CL:WITH-SLOTS () obj x)").findings.len(), 1);
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_binding_list_with_one_slot() {
        assert!(report("(with-slots (v) obj (frob v))").findings.is_empty());
    }

    /// The malformed shape: no instance operand, so nothing to be equivalent to.
    #[test]
    fn does_not_flag_a_form_without_an_instance_operand() {
        assert!(report("(with-slots ())").findings.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_list_binding_list() {
        assert!(report("(with-slots slots obj (frob))").findings.is_empty());
    }

    /// `'()` is a quoted datum, not an empty binding list.
    #[test]
    fn does_not_flag_a_quoted_empty_binding_list() {
        assert!(report("(with-slots '() obj (frob))").findings.is_empty());
    }

    #[test]
    fn does_not_flag_an_unrelated_head() {
        assert!(report("(with-open-file () obj (frob))").findings.is_empty());
    }

    // -- the five quote shapes ------------------------------------------------

    #[test]
    fn plain_code_fires() {
        assert!(fires("(with-slots () obj (frob))"));
    }

    #[test]
    fn a_hard_quoted_form_is_silent() {
        assert!(!fires("'(with-slots () obj (frob))"));
    }

    #[test]
    fn a_long_hand_quote_form_is_silent() {
        assert!(!fires("(quote (with-slots () obj (frob)))"));
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_silent() {
        assert!(!fires("'(a ,(with-slots () obj (frob)))"));
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_fires() {
        assert!(fires("`(a ,(with-slots () obj (frob)))"));
    }

    #[test]
    fn a_backquoted_template_is_silent() {
        assert!(!fires("(defmacro m () `(with-slots () obj (frob)))"));
    }

    // -- string literal -------------------------------------------------------

    #[test]
    fn a_form_spelled_only_inside_a_string_is_not_a_form() {
        assert!(
            report("(format t \"(with-slots () obj x)\")")
                .findings
                .is_empty()
        );
        assert!(!fires("(format t \"(with-slots () obj x)\")"));
    }

    // -- report envelope ------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(with-slots () obj x)", Dialect::Clojure)
            .expect("parse");
        let report = build_with_accessors_empty_binding_list_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("binding_form_count", json!(0))]);
    }

    #[test]
    fn the_summary_counts_every_form_scanned_not_only_the_flagged_ones() {
        let report = report("(with-slots () o x)\n(with-slots (v) o v)\n(with-accessors () o x)\n");
        assert_eq!(report.summary, vec![("binding_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_operator() {
        let report = report("(defun f (o)\n  (with-slots () o 1))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "with-accessors-empty-binding-list");
        assert_eq!(finding.text_columns(), vec!["with-slots".to_owned()]);
        assert!(finding.message().contains("establishes nothing"));
    }
}
