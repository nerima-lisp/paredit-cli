//! Common Lisp nested string-case detection: a `(OUTER (INNER s))` where BOTH
//! heads are non-destructive string case operations (`string-upcase`,
//! `string-downcase`, `string-capitalize`). Because case operations change only
//! letter case — never letter identity or word boundaries — the outer operation
//! fully determines the result. So `(string-upcase (string-downcase s))` is
//! exactly `(string-upcase s)`: the inner call is dead work. This holds for any
//! two of upcase/downcase/capitalize, including the idempotent
//! `(string-upcase (string-upcase s))`.
//!
//! Only the three non-destructive operations are matched. The destructive
//! `nstring-upcase`/`nstring-downcase`/`nstring-capitalize` are excluded —
//! dropping the inner one would drop its in-place mutation. The outer head token
//! is preserved in the fix, so the result reads with the dominating operation. A
//! reader-conditional operand `s` is left alone.
//!
//! The fix rewrites `(OUTER (INNER s))` as `(OUTER s)` (keeping the outer head),
//! copying `s`'s source, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// The non-destructive string case operations. The destructive `nstring-*`
/// counterparts are excluded because dropping the inner one would drop its
/// in-place mutation.
const CASE_OPS: [&str; 3] = ["string-upcase", "string-downcase", "string-capitalize"];

fn is_case_op(head: &str) -> bool {
    CASE_OPS.iter().any(|op| head.eq_ignore_ascii_case(op))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NestedStringCaseItem {
    pub path: PathBuf,
    /// The span of the whole `(OUTER (INNER s))` form.
    pub span: ByteSpan,
    /// The span of the outer case-op head token, preserved in the fix.
    pub outer_span: ByteSpan,
    /// The span of the string operand `s`.
    pub string_span: ByteSpan,
}

#[derive(Debug)]
pub struct NestedStringCaseSummary {
    pub string_case_form_count: usize,
    pub violations: Vec<NestedStringCaseItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NestedStringCasePolicyOptions {
    fail_on_violation: bool,
}

impl NestedStringCasePolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct NestedStringCasePolicy {
    pub fail_on_violation: bool,
    pub string_case_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    string_case_form_count: &mut usize,
    violations: &mut Vec<NestedStringCaseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !is_case_op(head) {
        return;
    }
    *string_case_form_count += 1;

    // children: [outer, inner] — the case op takes exactly one argument.
    if view.children.len() != 2 {
        return;
    }
    let inner = &view.children[1];
    if !is_paren_list(inner) {
        return;
    }
    let Some(inner_head) = list_head(inner) else {
        return;
    };
    if !is_case_op(inner_head) {
        return;
    }
    // inner children: [inner, s] — the inner case op takes exactly one argument.
    if inner.children.len() != 2 {
        return;
    }
    let string = &inner.children[1];
    if is_reader_conditional(string) {
        return;
    }

    violations.push(NestedStringCaseItem {
        path: path.to_path_buf(),
        span: view.span,
        outer_span: view.children[0].span,
        string_span: string.span,
    });
}

/// Collects every nested `(OUTER (INNER s))` case-op pair across a whole file,
/// along with the total number of outer case-op forms scanned.
pub fn collect_nested_string_cases(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NestedStringCaseItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut string_case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut string_case_form_count, &mut violations);
        });
    }
    Ok((string_case_form_count, violations))
}

#[must_use]
pub const fn summarize_nested_string_cases(
    string_case_form_count: usize,
    violations: Vec<NestedStringCaseItem>,
) -> NestedStringCaseSummary {
    NestedStringCaseSummary {
        string_case_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_nested_string_case_policy(
    options: NestedStringCasePolicyOptions,
    summary: &NestedStringCaseSummary,
) -> NestedStringCasePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NestedStringCasePolicy {
        fail_on_violation: options.fail_on_violation(),
        string_case_form_count: summary.string_case_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases(input: &str) -> (usize, Vec<NestedStringCaseItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_nested_string_cases(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect nested string cases")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_upcase_of_downcase() {
        let source = "(string-upcase (string-downcase s))";
        let (count, violations) = cases(source);
        // Both the outer upcase and the inner downcase are case-op forms scanned.
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].outer_span), "string-upcase");
        assert_eq!(slice(source, violations[0].string_span), "s");
    }

    #[test]
    fn flags_downcase_of_capitalize() {
        let (_, violations) = cases("(string-downcase (string-capitalize s))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_idempotent() {
        let (_, violations) = cases("(string-upcase (string-upcase s))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_single_case() {
        assert!(cases("(string-upcase s)").1.is_empty());
    }

    #[test]
    fn does_not_flag_inner_non_case() {
        assert!(cases("(string-upcase (subseq s 1))").1.is_empty());
    }

    #[test]
    fn does_not_flag_destructive_inner() {
        // (string-upcase (nstring-downcase s)) mutates s in place; dropping the
        // inner call would drop that mutation, so it is not equivalent.
        assert!(cases("(string-upcase (nstring-downcase s))").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = cases("(STRING-UPCASE (STRING-DOWNCASE s))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(string-upcase (string-downcase s))", Dialect::Clojure)
                .expect("parse");
        let (count, violations) =
            collect_nested_string_cases(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect nested string cases");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = cases("(string-upcase (string-downcase s))");
        let summary = summarize_nested_string_cases(count, items);

        let quiet =
            evaluate_nested_string_case_policy(NestedStringCasePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_nested_string_case_policy(NestedStringCasePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
