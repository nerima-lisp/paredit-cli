//! Common Lisp redundant-`let*` detection: a `let*` whose binding list holds
//! zero or one binding. `let*` differs from `let` only in that each binding's
//! init form is evaluated in the scope of the *earlier* bindings; with no
//! earlier binding to see, that difference vanishes. So `(let* ((x e)) body)`
//! is exactly `(let ((x e)) body)` and `(let* () body)` is `(let () body)` —
//! same evaluation order, same scope, same result. The plain `let` states "no
//! binding depends on another" directly, and readers no longer have to check.
//!
//! Only the zero- and one-binding shapes are flagged; a `let*` with two or more
//! bindings may genuinely rely on the sequential scope, so it is left alone. A
//! binding list that is not a parenthesized list (e.g. the atom `nil`, or a
//! reader-conditional operand) is left alone as well, since its settled binding
//! count is not statically knowable here.
//!
//! The fix rewrites just the `let*` head symbol to `let`, leaving the binding
//! list, body, spacing, and comments byte-identical, so the rule is
//! auto-fixable.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// binding list containing one has no settled binding count.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct RedundantLetStarItem {
    /// The span of the whole `(let* … …)` form.
    pub span: ByteSpan,
    /// The span of the `let*` head symbol (for the head-only rewrite to `let`).
    ///
    pub head_span: ByteSpan,
    /// The number of bindings (0 or 1) that made the `let*` redundant.
    pub binding_count: usize,
}

impl Finding for RedundantLetStarItem {
    /// The rule's own name. The binding count is a number, not a tag, so it
    /// stays a JSON field.
    fn kind(&self) -> &'static str {
        "redundant-let-star"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("bindings={}", self.binding_count)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("binding_count", json!(self.binding_count))]
    }

    fn message(&self) -> String {
        format!(
            "let* with {} binding{} is just let; sequential scope is unused",
            self.binding_count,
            if self.binding_count == 1 { "" } else { "s" }
        )
    }
}

pub fn examine_let_star(
    view: &ExpressionView,
    let_star_form_count: &mut usize,
    violations: &mut Vec<RedundantLetStarItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("let*") {
        return;
    }
    *let_star_form_count += 1;

    // children: [let*, binding-list, body…] — need at least the binding list.
    if view.children.len() < 2 {
        return;
    }
    let binding_list = &view.children[1];
    // Only a parenthesized binding list has a statically knowable count; a bare
    // `nil` or a macro-expanded operand is left alone.
    if !is_paren_list(binding_list) {
        return;
    }
    // A reader-conditional binding makes the true count build-dependent.
    if binding_list.children.iter().any(is_reader_conditional) {
        return;
    }
    let binding_count = binding_list.children.len();
    if binding_count > 1 {
        return;
    }

    violations.push(RedundantLetStarItem {
        span: view.span,
        head_span: view.children[0].span,
        binding_count,
    });
}

/// Collects every redundant `let*` (≤ 1 binding) in one file, with the number
/// of `let*` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_redundant_let_star_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantLetStarItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("let_star_form_count", json!(0))],
        ));
    }

    let mut let_star_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_let_star(subview, &mut let_star_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("let_star_form_count", json!(let_star_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RedundantLetStarItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_let_star_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant let* report")
    }

    fn let_stars(input: &str) -> (u64, Vec<RedundantLetStarItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "let_star_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("let_star_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_single_binding() {
        let source = "(let* ((x 1)) (+ x x))";
        let (count, violations) = let_stars(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].binding_count, 1);
        assert_eq!(slice(source, violations[0].head_span), "let*");
    }

    #[test]
    fn flags_zero_bindings() {
        let (_, violations) = let_stars("(let* () (side-effect))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].binding_count, 0);
    }

    #[test]
    fn flags_single_bare_symbol_binding() {
        // `(let* (x) ...)` binds x to nil — one binding, still redundant.
        let (_, violations) = let_stars("(let* (x) x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].binding_count, 1);
    }

    #[test]
    fn does_not_flag_two_bindings() {
        let (count, violations) = let_stars("(let* ((x 1) (y (* x 2))) y)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_plain_let() {
        let (count, violations) = let_stars("(let ((x 1)) x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_nil_binding_list() {
        // A bare `nil` binding list is not a paren list; leave it alone.
        let (_, violations) = let_stars("(let* nil (foo))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_reader_conditional_binding() {
        let (_, violations) = let_stars("(let* (#+sbcl (x 1)) x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = let_stars("(LET* ((x 1)) x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_redundant_let_star() {
        let (_, violations) = let_stars("(defun f () (let* ((x 1)) x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(let* ((x 1)) x)", Dialect::Clojure).expect("parse");
        let report = build_redundant_let_star_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build redundant let* report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("let_star_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(let ((x 1)) x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_binding_count() {
        let report = report("(defun f ()\n  (let* ((x 1)) x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "redundant-let-star");
        assert_eq!(finding.json_fields(), vec![("binding_count", json!(1))]);
        assert_eq!(finding.text_columns(), vec!["bindings=1".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_let_star_scanned_not_only_the_flagged_ones() {
        let report = report("(let* ((x 1)) x)\n(let* ((a 1) (b a)) b)\n");
        assert_eq!(report.summary, vec![("let_star_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
