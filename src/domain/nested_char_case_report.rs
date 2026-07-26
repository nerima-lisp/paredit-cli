//! Common Lisp nested char-case detection: a `(OUTER (INNER c))` where BOTH
//! heads are character case operations (`char-upcase`, `char-downcase`). A char
//! case operation changes only case — never character identity — so the outer
//! operation fully determines the result. So `(char-upcase (char-downcase c))`
//! is exactly `(char-upcase c)`: the inner call is dead work. This holds for any
//! two of upcase/downcase, including the idempotent
//! `(char-upcase (char-upcase c))`.
//!
//! Both `char-upcase` and `char-downcase` are non-destructive (they return a
//! fresh character; there are no `nchar-*` mutators), so there is no mutation to
//! preserve. The outer head token is kept in the fix so the result reads with
//! the dominating operation. A reader-conditional operand `c` is left alone.
//!
//! The fix rewrites `(OUTER (INNER c))` as `(OUTER c)` (keeping the outer head),
//! copying `c`'s source, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// The character case operations. Both are non-destructive.
const CASE_OPS: [&str; 2] = ["char-upcase", "char-downcase"];

fn is_case_op(head: &str) -> bool {
    CASE_OPS.iter().any(|op| head.eq_ignore_ascii_case(op))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NestedCharCaseItem {
    pub path: PathBuf,
    /// The span of the whole `(OUTER (INNER c))` form.
    pub span: ByteSpan,
    /// The span of the outer case-op head token, preserved in the fix.
    pub outer_span: ByteSpan,
    /// The span of the character operand `c`.
    pub char_span: ByteSpan,
}

#[derive(Debug)]
pub struct NestedCharCaseSummary {
    pub char_case_form_count: usize,
    pub violations: Vec<NestedCharCaseItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NestedCharCasePolicyOptions {
    fail_on_violation: bool,
}

impl NestedCharCasePolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct NestedCharCasePolicy {
    pub fail_on_violation: bool,
    pub char_case_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    char_case_form_count: &mut usize,
    violations: &mut Vec<NestedCharCaseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !is_case_op(head) {
        return;
    }
    *char_case_form_count += 1;

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
    // inner children: [inner, c] — the inner case op takes exactly one argument.
    if inner.children.len() != 2 {
        return;
    }
    let character = &inner.children[1];
    if is_reader_conditional(character) {
        return;
    }

    violations.push(NestedCharCaseItem {
        path: path.to_path_buf(),
        span: view.span,
        outer_span: view.children[0].span,
        char_span: character.span,
    });
}

/// Collects every nested `(OUTER (INNER c))` char case-op pair across a whole
/// file, along with the total number of outer case-op forms scanned.
pub fn collect_nested_char_cases(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NestedCharCaseItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut char_case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut char_case_form_count, &mut violations)
        });
    }
    Ok((char_case_form_count, violations))
}

pub fn summarize_nested_char_cases(
    char_case_form_count: usize,
    violations: Vec<NestedCharCaseItem>,
) -> NestedCharCaseSummary {
    NestedCharCaseSummary {
        char_case_form_count,
        violations,
    }
}

pub fn evaluate_nested_char_case_policy(
    options: NestedCharCasePolicyOptions,
    summary: &NestedCharCaseSummary,
) -> NestedCharCasePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NestedCharCasePolicy {
        fail_on_violation: options.fail_on_violation(),
        char_case_form_count: summary.char_case_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases(input: &str) -> (usize, Vec<NestedCharCaseItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_nested_char_cases(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect nested char cases")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_upcase_of_downcase() {
        let source = "(char-upcase (char-downcase c))";
        let (count, violations) = cases(source);
        // Both the outer upcase and the inner downcase are case-op forms scanned.
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].outer_span), "char-upcase");
        assert_eq!(slice(source, violations[0].char_span), "c");
    }

    #[test]
    fn flags_idempotent() {
        let (_, violations) = cases("(char-downcase (char-downcase c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_single_case() {
        assert!(cases("(char-upcase c)").1.is_empty());
    }

    #[test]
    fn does_not_flag_inner_non_case() {
        assert!(cases("(char-upcase (elt s 1))").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = cases("(CHAR-UPCASE (CHAR-DOWNCASE c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(char-upcase (char-downcase c))", Dialect::Clojure)
                .expect("parse");
        let (count, violations) =
            collect_nested_char_cases(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect nested char cases");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = cases("(char-upcase (char-downcase c))");
        let summary = summarize_nested_char_cases(count, items);

        let quiet =
            evaluate_nested_char_case_policy(NestedCharCasePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_nested_char_case_policy(NestedCharCasePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
