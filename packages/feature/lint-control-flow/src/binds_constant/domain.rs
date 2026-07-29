//! Common Lisp binds-constant detection: a `let`, `let*`, `do`, or `do*`
//! binding whose variable is a constant that cannot be bound — `nil`, `t`, or
//! a keyword (`:foo`). The standard forbids binding a constant variable, so
//! `(let ((nil 1)) …)`, `(let ((t 1)) …)`, and `(let ((:x 1)) …)` are all
//! program errors, caught at macroexpansion rather than by the reader.
//!
//! Only the three statically-known constants are flagged. A user `defconstant`
//! such as `+limit+` is equally unbindable, but which symbols are constant is
//! not visible from a single file, so those are left alone to keep the rule
//! free of false positives.
//!
//! Complements `malformed-let-binding`, which checks a
//! binding's *shape*; this rule checks the *validity of the bound variable*.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

const BINDING_HEADS: [&str; 4] = ["let", "let*", "do", "do*"];

/// Whether a bound variable name is a constant that cannot be bound: `nil`,
/// `t`, or a keyword. A keyword is a single leading colon followed by a
/// non-colon, so `foo:bar` (package-qualified) and `::x` (internal-symbol
/// syntax) are not treated as keywords.
fn is_constant_variable(name: &str) -> bool {
    if name.eq_ignore_ascii_case("t") || name.eq_ignore_ascii_case("nil") {
        return true;
    }
    name.len() > 1 && name.starts_with(':') && name.as_bytes()[1] != b':'
}

/// The bound variable of one binding: a bare symbol, or the head symbol of a
/// `(var …)` list binding.
fn binding_variable(binding: &ExpressionView) -> Option<&str> {
    atom_text(binding).or_else(|| {
        is_paren_list(binding)
            .then(|| binding.children.first().and_then(atom_text))
            .flatten()
    })
}

#[derive(Debug, Clone)]
pub struct BindsConstantItem {
    /// The span of the offending binding.
    pub span: ByteSpan,
    /// The 1-based line the binding starts on.
    pub line: usize,
    /// The binding operator (`let`, `let*`, `do`, `do*`) as written.
    pub head: String,
    /// The constant that cannot be bound, as written.
    pub variable: String,
}

impl Finding for BindsConstantItem {
    /// The rule's own name. The binding operator and the constant are both
    /// per-finding strings rather than a fixed vocabulary, so neither can be a
    /// `&'static str` variant; both are reported as fields instead.
    fn kind(&self) -> &'static str {
        "binds-constant"
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
            format!("variable={}", self.variable),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            ("variable", json!(self.variable)),
        ]
    }

    /// The same sentence the `binds-constant` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!("{} cannot bind the constant {}", self.head, self.variable)
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_binding_form(
    view: &ExpressionView,
    source: &str,
    binding_form_count: &mut usize,
    violations: &mut Vec<BindsConstantItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !BINDING_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted binding form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    let Some(binding_list) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(binding_list) {
        return;
    }
    *binding_form_count += 1;

    for binding in &binding_list.children {
        let Some(variable) = binding_variable(binding) else {
            continue;
        };
        if is_constant_variable(variable) {
            violations.push(BindsConstantItem {
                span: binding.span,
                line: line_of(source, binding.span.start().get()),
                head: head.to_owned(),
                variable: variable.to_owned(),
            });
        }
    }
}

/// Collects every `let`/`let*`/`do`/`do*` binding of a constant variable in one
/// file, with the number of binding forms scanned as the denominator beside
/// them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no constant binding here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_binds_constant_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<BindsConstantItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("binding_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut binding_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_binding_form(subview, source, &mut binding_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("binding_form_count", json!(binding_form_count))],
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

    fn report(input: &str) -> FileFindings<BindsConstantItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_binds_constant_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build binds constant report")
    }

    /// The `(binding_form_count, violations)` pair the report is built from.
    fn violations(input: &str) -> (u64, Vec<BindsConstantItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "binding_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("binding_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_let_binding_of_nil() {
        let (binding_form_count, items) = violations("(let ((nil 1)) nil)");
        assert_eq!(binding_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].variable, "nil");
        assert_eq!(items[0].head, "let");
    }

    #[test]
    fn flags_a_let_binding_of_t() {
        let (_, items) = violations("(let ((t 1)) t)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].variable, "t");
    }

    #[test]
    fn flags_a_let_binding_of_a_keyword() {
        let (_, items) = violations("(let ((:x 1)) :x)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].variable, ":x");
    }

    #[test]
    fn flags_a_bare_symbol_nil_binding() {
        let (_, items) = violations("(let (nil) 1)");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_a_let_star_binding_of_t() {
        let (_, items) = violations("(let* ((t 1)) t)");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_a_do_binding_of_a_keyword() {
        let (binding_form_count, items) = violations("(do ((:k 0 (1+ :k))) ((done)))");
        assert_eq!(binding_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "do");
    }

    #[test]
    fn does_not_flag_ordinary_variables() {
        let (binding_form_count, items) = violations("(let ((x 1) (y 2)) (+ x y))");
        assert_eq!(binding_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_special_variable() {
        let (_, items) = violations("(let ((*foo* 1)) *foo*)");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_package_qualified_symbol() {
        let (_, items) = violations("(let ((foo:bar 1)) foo:bar)");
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_binding_form() {
        let (binding_form_count, items) = violations("(list '(let ((nil 1)) 1))");
        assert_eq!(binding_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_binding_nested_in_a_function_body() {
        let (binding_form_count, items) = violations("(defun f () (let ((t 1)) t))");
        assert_eq!(binding_form_count, 1);
        assert_eq!(items.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(let ((nil 1)) nil)", Dialect::Clojure).expect("parse");
        let report = build_binds_constant_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build binds constant report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("binding_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(let ((x 1)) x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_head_and_its_variable() {
        let report = report("(defun f ()\n  (let ((t 1)) t))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "binds-constant");
        assert_eq!(
            finding.json_fields(),
            vec![("head", json!("let")), ("variable", json!("t"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["head=let".to_owned(), "variable=t".to_owned()]
        );
        assert_eq!(finding.message(), "let cannot bind the constant t");
    }

    #[test]
    fn the_summary_counts_every_binding_form_scanned_not_only_the_flagged_ones() {
        let report = report("(let ((nil 1)) nil)\n(let ((x 1)) x)\n");
        assert_eq!(report.summary, vec![("binding_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
