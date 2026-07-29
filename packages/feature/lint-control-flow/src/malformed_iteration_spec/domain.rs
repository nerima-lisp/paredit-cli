//! Common Lisp malformed-iteration-spec detection: a `dolist` or `dotimes`
//! binding spec that is not a `(var form [result])` list. `dolist`'s spec is
//! `(var list-form [result-form])` and `dotimes`'s is
//! `(var count-form [result-form])` — each takes exactly two or three elements.
//! A non-list spec (`(dolist x …)`), a one-element spec (`(dolist (x) …)`,
//! missing the list/count form), or a four-plus-element spec is a program
//! error, caught only at macroexpansion rather than by the reader.
//!
//! Scoped to `dolist`/`dotimes` on purpose: `do`/`do*` step bindings take a
//! third `step` form and a different overall shape, so they are not inspected
//! here.
//!
//! Forms whose spec arity is not statically visible are skipped to avoid false
//! positives: a quoted/quasiquoted form (data or a template), and any spec
//! whose value is or contains a `#+`/`#-` reader conditional or `,@` splice,
//! where the written element count differs from the evaluated one.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

const ITERATION_HEADS: [&str; 2] = ["dolist", "dotimes"];

/// Whether a form's value can change how many elements actually reach the
/// evaluator, making a static arity count unreliable: a `,@` splice or Clojure
/// reader conditional (modeled as a prefix), or a Common Lisp `#+`/`#-`
/// conditional (modeled as an atom whose text begins `#+`/`#-`).
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

#[derive(Debug, Clone)]
pub struct MalformedIterationSpecItem {
    /// The span of the offending spec.
    pub span: ByteSpan,
    /// The 1-based line the spec starts on.
    pub line: usize,
    /// The iteration operator (`dolist`, `dotimes`) as written.
    pub head: String,
    /// The spec, re-rendered from the tree.
    pub spec: String,
    /// How many elements the spec has, against the two or three it needs.
    pub element_count: usize,
}

impl Finding for MalformedIterationSpecItem {
    /// The rule's own name. The iteration operator is a per-finding string
    /// rather than a fixed vocabulary, so it is a field instead of a variant.
    fn kind(&self) -> &'static str {
        "malformed-iteration-spec"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("head={}", self.head),
            format!("elements={}", self.element_count),
            format!("spec={}", self.spec),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            ("element_count", json!(self.element_count)),
            ("spec", json!(self.spec)),
        ]
    }

    /// The same sentence the `malformed-iteration-spec` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} spec {} must be (var form [result])",
            self.head, self.spec
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_iteration(
    view: &ExpressionView,
    source: &str,
    iteration_form_count: &mut usize,
    violations: &mut Vec<MalformedIterationSpecItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !ITERATION_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted iteration form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    let Some(spec) = view.children.get(1) else {
        return;
    };
    *iteration_form_count += 1;

    // A spec that reads as a `#+`/`#-`-guarded node has no statically visible
    // shape.
    if is_arity_ambiguous(spec) {
        return;
    }

    if !is_paren_list(spec) {
        // A bare atom (or bracket/brace form) is never a valid spec.
        violations.push(MalformedIterationSpecItem {
            span: spec.span,
            line: line_of(source, spec.span.start().get()),
            head: head.to_owned(),
            spec: render_expression(spec),
            element_count: spec.children.len(),
        });
        return;
    }

    // A `#+`/`#-` element inside the spec makes its written arity unreliable.
    if spec.children.iter().any(is_arity_ambiguous) {
        return;
    }

    let element_count = spec.children.len();
    if !(2..=3).contains(&element_count) {
        violations.push(MalformedIterationSpecItem {
            span: spec.span,
            line: line_of(source, spec.span.start().get()),
            head: head.to_owned(),
            spec: render_expression(spec),
            element_count,
        });
    }
}

/// Collects every malformed `dolist`/`dotimes` spec in one file, with the
/// number of `dolist`/`dotimes` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every spec is well-formed here" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_malformed_iteration_spec_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MalformedIterationSpecItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("iteration_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut iteration_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_iteration(subview, source, &mut iteration_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("iteration_form_count", json!(iteration_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MalformedIterationSpecItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_malformed_iteration_spec_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build malformed iteration spec report")
    }

    /// The `(iteration_form_count, violations)` pair the report is built from.
    fn specs(input: &str) -> (u64, Vec<MalformedIterationSpecItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "iteration_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("iteration_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_one_element_dolist_spec() {
        let (iteration_form_count, violations) = specs("(dolist (x) (print x))");
        assert_eq!(iteration_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].element_count, 1);
        assert_eq!(violations[0].head, "dolist");
    }

    #[test]
    fn flags_a_four_element_dotimes_spec() {
        let (_, violations) = specs("(dotimes (i n r extra) (print i))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].element_count, 4);
        assert_eq!(violations[0].head, "dotimes");
    }

    #[test]
    fn flags_a_non_list_spec() {
        let (_, violations) = specs("(dolist x (print x))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].spec, "x");
    }

    #[test]
    fn does_not_flag_a_two_element_spec() {
        let (iteration_form_count, violations) = specs("(dolist (x items) (print x))");
        assert_eq!(iteration_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_three_element_spec() {
        let (_, violations) = specs("(dolist (x items result) (print x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_valid_dotimes() {
        let (_, violations) = specs("(dotimes (i 10 done) (print i))");
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_feature_conditional_spec_element() {
        // `#+a n1 #-a n2 result` reads as feature-conditional atoms; only one
        // count form survives, so the written arity is not reliable.
        let (iteration_form_count, violations) =
            specs("(dotimes (i #+sbcl n1 #-sbcl n2 result) (print i))");
        assert_eq!(iteration_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_quoted_iteration_form() {
        let (iteration_form_count, violations) = specs("(list '(dolist (x) x))");
        assert_eq!(iteration_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_an_iteration_nested_in_a_function_body() {
        let (iteration_form_count, violations) = specs("(defun f (xs) (dolist (x) (print x)))");
        assert_eq!(iteration_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(dolist (x) (print x))", Dialect::Clojure)
            .expect("parse input");
        let report =
            build_malformed_iteration_spec_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build malformed iteration spec report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("iteration_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(dolist (x items) (print x))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_head_its_arity_and_its_spec() {
        let report = report("(defun f (xs)\n  (dolist (x) (print x)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "malformed-iteration-spec");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("head", json!("dolist")),
                ("element_count", json!(1)),
                ("spec", json!("(x)")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "head=dolist".to_owned(),
                "elements=1".to_owned(),
                "spec=(x)".to_owned(),
            ]
        );
        assert_eq!(
            finding.message(),
            "dolist spec (x) must be (var form [result])"
        );
    }

    #[test]
    fn the_summary_counts_every_iteration_form_scanned_not_only_the_flagged_ones() {
        let report = report("(dolist (x) (print x))\n(dolist (y items) (print y))\n");
        assert_eq!(report.summary, vec![("iteration_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
