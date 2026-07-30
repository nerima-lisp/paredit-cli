//! Common Lisp malformed-`let`-binding detection: a `let` or `let*` binding
//! that is neither a bare symbol nor a `(var)` / `(var value)` list. A binding
//! list element with zero elements (`()`) or three or more elements
//! (`(x 1 2)`) is a program error — `let`/`let*` bindings, unlike `do`/`do*`
//! step bindings, never take a third form.
//!
//! The highest-value catch is a dropped-parenthesis typo: writing
//! `(let ((x 1 y 2)) …)` when `(let ((x 1) (y 2)) …)` was meant produces one
//! four-element binding, which this rule flags directly.
//!
//! Scoped to `let`/`let*` on purpose: `do`/`do*` bindings legitimately carry a
//! third `step` form, so they are never inspected here. Bare-symbol bindings
//! (`(let (x y) …)`) and single-element `(var)` bindings are valid and left
//! alone. This report walks the whole tree via the shared
//! [`paredit_core_syntax::view_query::for_each_subview`], since a `let` can appear
//! anywhere in a body.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

const LET_HEADS: [&str; 2] = ["let", "let*"];

#[derive(Debug, Clone)]
pub struct MalformedLetBindingItem {
    pub span: ByteSpan,
    pub binding: String,
    pub element_count: usize,
}

impl Finding for MalformedLetBindingItem {
    /// The rule's own name.
    ///
    /// `element_count` is the natural discriminator, but a `kind` is a tag from
    /// a closed set and an element count is unbounded data — a consumer wanting
    /// to select the four-element bindings reads `element_count` from
    /// `json_fields`, where it is a number rather than a string smuggled into a
    /// rule id.
    fn kind(&self) -> &'static str {
        "malformed-let-binding"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("elements={}", self.element_count),
            format!("binding={}", self.binding),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("element_count", json!(self.element_count)),
            ("binding", json!(self.binding)),
        ]
    }

    /// The same sentence the `malformed-let-binding` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "let binding {} has {} elements; expected a symbol or (var value)",
            self.binding, self.element_count
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_let(
    view: &ExpressionView,
    let_form_count: &mut usize,
    violations: &mut Vec<MalformedLetBindingItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !LET_HEADS.iter().any(|name| head.eq_ignore_ascii_case(name)) {
        return;
    }
    let Some(binding_list) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(binding_list) {
        return;
    }
    *let_form_count += 1;

    for binding in &binding_list.children {
        // A bare-symbol binding is valid; only list-shaped bindings can carry
        // the wrong number of elements.
        if !is_paren_list(binding) {
            continue;
        }
        let element_count = binding.children.len();
        if element_count == 0 || element_count > 2 {
            violations.push(MalformedLetBindingItem {
                span: binding.span,
                binding: render_expression(binding),
                element_count,
            });
        }
    }
}

/// Collects every malformed `let`/`let*` binding in one file, with the number of
/// `let`/`let*` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every binding is well-formed" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_malformed_let_binding_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MalformedLetBindingItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("let_form_count", json!(0))],
        ));
    }

    let mut let_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_let(subview, &mut let_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("let_form_count", json!(let_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MalformedLetBindingItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_malformed_let_binding_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build malformed let binding report")
    }

    /// The `(let_form_count, violations)` pair the report is built from.
    fn bindings(input: &str) -> (u64, Vec<MalformedLetBindingItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "let_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("let_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_three_element_binding() {
        let (let_form_count, violations) = bindings("(let ((x 1 2)) x)");
        assert_eq!(let_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].element_count, 3);
    }

    #[test]
    fn flags_a_dropped_paren_binding() {
        let (_, violations) = bindings("(let ((x 1 y 2)) x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].element_count, 4);
    }

    #[test]
    fn flags_an_empty_binding() {
        let (_, violations) = bindings("(let (()) 1)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].element_count, 0);
    }

    #[test]
    fn does_not_flag_a_var_value_binding() {
        let (let_form_count, violations) = bindings("(let ((x 1) (y 2)) (+ x y))");
        assert_eq!(let_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_single_element_binding() {
        let (_, violations) = bindings("(let ((x)) x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_bare_symbol_binding() {
        let (_, violations) = bindings("(let (x y) (list x y))");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_a_malformed_let_star_binding() {
        let (_, violations) = bindings("(let* ((x 1 2)) x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_do_step_bindings() {
        let (let_form_count, violations) =
            bindings("(do ((i 0 (1+ i)) (n 10)) ((>= i n)) (print i))");
        assert_eq!(let_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_let_nested_in_a_function_body() {
        let (let_form_count, violations) = bindings("(defun f () (let ((x 1 2)) x))");
        assert_eq!(let_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(let ((x 1 2)) x)", Dialect::Clojure).expect("parse");
        let report =
            build_malformed_let_binding_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build malformed let binding report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("let_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(let ((x 1)) x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_binding_and_its_element_count() {
        let report = report("(defun f ()\n  (let ((x 1 2)) x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "malformed-let-binding");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("element_count", json!(3)),
                ("binding", json!(finding.binding)),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "elements=3".to_owned(),
                format!("binding={}", finding.binding),
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_let_scanned_not_only_the_flagged_ones() {
        let report = report("(let ((x 1)) x)\n(let ((y 1 2)) y)\n");
        assert_eq!(report.summary, vec![("let_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
