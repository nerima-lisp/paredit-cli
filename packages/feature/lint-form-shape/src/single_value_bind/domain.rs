//! Common Lisp single-value `multiple-value-bind` detection: a
//! `multiple-value-bind` whose variable list names exactly one variable.
//! `multiple-value-bind` binds each variable to the corresponding value of the
//! form; `let` binds its variable to the form's *primary* value. Those coincide
//! for a single variable, so `(multiple-value-bind (x) form body)` is exactly
//! `(let ((x form)) body)` — same binding, same body, same result — but without
//! the multiple-values machinery the reader must otherwise account for.
//!
//! Only the one-variable shape is flagged. A binding list with two or more
//! variables genuinely captures secondary values that `let` would discard, and
//! an empty variable list `()` is a `progn`, not a `let` — both are left alone.
//! A `&optional`/lambda-list-keyword pseudo-variable and a reader-conditional
//! operand are left alone as well.
//!
//! The fix rewrites `(multiple-value-bind (x) form body…)` as
//! `(let ((x form)) body…)`, copying the variable, form, and body from their
//! exact source, so the rule is auto-fixable.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteOffset, ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled shape.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Whether `view` is a plain variable name: a bare symbol atom that is not a
/// lambda-list keyword (`&optional`, …) and not a reader-conditional operand.
fn is_plain_variable(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| !text.is_empty() && !text.starts_with('&'))
}

#[derive(Debug, Clone)]
pub struct SingleValueBindItem {
    /// The span of the whole `(multiple-value-bind (x) form body…)` form.
    pub span: ByteSpan,
    /// The span of the single bound variable `x`.
    ///
    pub var_span: ByteSpan,
    /// The span of the value form.
    ///
    /// The rewrite's input, not the report's — see [`Self::var_span`].
    pub form_span: ByteSpan,
    /// The span covering the body forms (`None` when there is no body).
    ///
    /// The rewrite's input, not the report's — see [`Self::var_span`].
    pub body_span: Option<ByteSpan>,
}

impl Finding for SingleValueBindItem {
    /// Fixed: this rule has exactly one shape to report, and no sub-kind to
    /// separate.
    fn kind(&self) -> &'static str {
        "single-value-bind"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Empty, because the old text row carried nothing past the path and
    /// offset: the span alone locates the bind.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// Empty, because the old JSON carried only the path and span, which the
    /// envelope already emits. The three rewrite spans stay off the report for
    /// the same reason they were off it before.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    fn message(&self) -> String {
        "multiple-value-bind of one variable is just let; (multiple-value-bind (x) f body) is (let ((x f)) body)".to_owned()
    }
}

pub fn examine_bind(
    view: &ExpressionView,
    bind_form_count: &mut usize,
    violations: &mut Vec<SingleValueBindItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("multiple-value-bind") {
        return;
    }
    *bind_form_count += 1;

    // children: [multiple-value-bind, varlist, form, body…] — need at least the
    // variable list and the value form.
    if view.children.len() < 3 {
        return;
    }
    let varlist = &view.children[1];
    if !is_paren_list(varlist) {
        return;
    }
    // Exactly one bound variable, and it must be a plain symbol.
    if varlist.children.len() != 1 {
        return;
    }
    let var = &varlist.children[0];
    if !is_plain_variable(var) {
        return;
    }
    let form = &view.children[2];
    if is_reader_conditional(form) {
        return;
    }

    // Body spans from the first body form through the last (verbatim copy keeps
    // its spacing and comments); `None` when there is no body.
    let body_span = if view.children.len() > 3 {
        let start = view.children[3].span.start();
        let end = view.children[view.children.len() - 1].span.end();
        Some(ByteSpan::new(
            ByteOffset::new(start.get()),
            ByteOffset::new(end.get()),
        ))
    } else {
        None
    };

    violations.push(SingleValueBindItem {
        span: view.span,
        var_span: var.span,
        form_span: form.span,
        body_span,
    });
}

/// Collects every single-value `multiple-value-bind` in one file, with the
/// number of `multiple-value-bind` forms scanned as the denominator beside
/// them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_single_value_bind_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SingleValueBindItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("bind_form_count", json!(0))],
        ));
    }

    let mut bind_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_bind(subview, &mut bind_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("bind_form_count", json!(bind_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SingleValueBindItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_single_value_bind_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build single-value bind report")
    }

    fn binds(input: &str) -> (u64, Vec<SingleValueBindItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "bind_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("bind_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_single_variable() {
        let source = "(multiple-value-bind (q) (truncate a b) (use q))";
        let (count, violations) = binds(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].var_span), "q");
        assert_eq!(slice(source, violations[0].form_span), "(truncate a b)");
        let body = violations[0].body_span.expect("body span");
        assert_eq!(slice(source, body), "(use q)");
    }

    #[test]
    fn flags_multi_form_body() {
        let source = "(multiple-value-bind (x) (f) a b c)";
        let (_, violations) = binds(source);
        let body = violations[0].body_span.expect("body span");
        assert_eq!(slice(source, body), "a b c");
    }

    #[test]
    fn flags_empty_body() {
        let (_, violations) = binds("(multiple-value-bind (x) (f))");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].body_span.is_none());
    }

    #[test]
    fn does_not_flag_two_variables() {
        let (count, violations) = binds("(multiple-value-bind (q r) (truncate a b) q)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_empty_variable_list() {
        // (multiple-value-bind () form body) is a progn, not a let.
        let (_, violations) = binds("(multiple-value-bind () (side-effect) done)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_lambda_list_keyword() {
        let (_, violations) = binds("(multiple-value-bind (&rest xs) (f) xs)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = binds("(MULTIPLE-VALUE-BIND (x) (f) x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_bind() {
        let (_, violations) = binds("(defun g () (multiple-value-bind (v) (compute) (list v)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(multiple-value-bind (x) (f) x)", Dialect::Clojure)
                .expect("parse");
        let report = build_single_value_bind_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build single-value bind report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("bind_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(multiple-value-bind (q r) (f) q)").dialect_modelled);
    }

    /// The old report published only the path and span, so the envelope's own
    /// columns are the whole finding — the three rewrite spans stay internal.
    #[test]
    fn a_finding_carries_its_line_and_nothing_the_envelope_does_not_already_print() {
        let report = report("(defun g ()\n  (multiple-value-bind (v) (compute) (list v)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "single-value-bind");
        assert!(finding.text_columns().is_empty());
        assert!(finding.json_fields().is_empty());
    }

    #[test]
    fn the_summary_counts_every_bind_scanned_not_only_the_flagged_ones() {
        let report = report(
            "(multiple-value-bind (x) (f) x)\n(multiple-value-bind (q r) (g) q)\n(multiple-value-bind (y) (h) y)\n",
        );
        assert_eq!(report.summary, vec![("bind_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
