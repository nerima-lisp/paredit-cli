//! Common Lisp explicit-nil-return detection: a `return` or `return-from` whose
//! result form is the literal `nil`. The result argument of both defaults to
//! `nil`, so `(return nil)` is exactly `(return)` and `(return-from foo nil)` is
//! exactly `(return-from foo)` — same block exited, same `nil` produced.
//! Dropping the redundant `nil` states the unit return the way the operators
//! were designed to express it.
//!
//! The two operators are matched by their own arities, and the distinction
//! matters:
//!
//!   - `(return nil)` — exactly two elements; the `nil` is the result.
//!   - `(return-from foo nil)` — exactly three elements; the `nil` is the
//!     result and `foo` is the block name, which is preserved.
//!
//! `(return-from nil)` is *not* matched: there the `nil` is the block name (the
//! implicit `nil` block), not a result. A non-`nil` result, a reader-conditional
//! operand, and any other arity are left alone.
//!
//! The fix drops the redundant `nil`, copying the operator (and, for
//! `return-from`, the block name) from its exact source, so the rule is
//! auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

/// Whether `view` is the bare `nil` literal (case-insensitive, no reader
/// prefixes so a quoted/`,`-prefixed `nil` is excluded).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct ExplicitNilReturnItem {
    pub path: PathBuf,
    /// The span of the whole `(return nil)` / `(return-from b nil)` form.
    pub span: ByteSpan,
    /// The span of the `return`/`return-from` head symbol (exact source).
    pub head_span: ByteSpan,
    /// The span of the block name (`return-from` only; `None` for `return`).
    pub block_span: Option<ByteSpan>,
    /// The canonical operator name (`return`/`return-from`), for the message.
    pub operator: &'static str,
}

#[derive(Debug)]
pub struct ExplicitNilReturnSummary {
    pub return_form_count: usize,
    pub violations: Vec<ExplicitNilReturnItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ExplicitNilReturnPolicyOptions {
    fail_on_violation: bool,
}

impl ExplicitNilReturnPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct ExplicitNilReturnPolicy {
    pub fail_on_violation: bool,
    pub return_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_return(
    view: &ExpressionView,
    path: &Path,
    return_form_count: &mut usize,
    violations: &mut Vec<ExplicitNilReturnItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let is_return = head.eq_ignore_ascii_case("return");
    let is_return_from = head.eq_ignore_ascii_case("return-from");
    if !is_return && !is_return_from {
        return;
    }
    *return_form_count += 1;

    let head_span = view.children[0].span;
    if is_return {
        // (return nil): exactly the result operand.
        if view.children.len() != 2 {
            return;
        }
        let result = &view.children[1];
        if is_reader_conditional(result) || !is_nil_literal(result) {
            return;
        }
        violations.push(ExplicitNilReturnItem {
            path: path.to_path_buf(),
            span: view.span,
            head_span,
            block_span: None,
            operator: "return",
        });
    } else {
        // (return-from block nil): block name plus the result operand.
        if view.children.len() != 3 {
            return;
        }
        let block = &view.children[1];
        let result = &view.children[2];
        if is_reader_conditional(block) || is_reader_conditional(result) || !is_nil_literal(result)
        {
            return;
        }
        violations.push(ExplicitNilReturnItem {
            path: path.to_path_buf(),
            span: view.span,
            head_span,
            block_span: Some(block.span),
            operator: "return-from",
        });
    }
}

/// Collects every `return`/`return-from` with an explicit `nil` result across a
/// whole file, along with the total number of `return`/`return-from` forms
/// scanned.
pub fn collect_explicit_nil_returns(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<ExplicitNilReturnItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut return_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_return(subview, path, &mut return_form_count, &mut violations);
        });
    }
    Ok((return_form_count, violations))
}

pub fn summarize_explicit_nil_returns(
    return_form_count: usize,
    violations: Vec<ExplicitNilReturnItem>,
) -> ExplicitNilReturnSummary {
    ExplicitNilReturnSummary {
        return_form_count,
        violations,
    }
}

pub fn evaluate_explicit_nil_return_policy(
    options: ExplicitNilReturnPolicyOptions,
    summary: &ExplicitNilReturnSummary,
) -> ExplicitNilReturnPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ExplicitNilReturnPolicy {
        fail_on_violation: options.fail_on_violation(),
        return_form_count: summary.return_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn returns(input: &str) -> (usize, Vec<ExplicitNilReturnItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_explicit_nil_returns(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect explicit nil returns")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_return_nil() {
        let source = "(return nil)";
        let (count, violations) = returns(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "return");
        assert!(violations[0].block_span.is_none());
    }

    #[test]
    fn flags_return_from_nil() {
        let source = "(return-from search nil)";
        let (_, violations) = returns(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "return-from");
        let block = violations[0].block_span.expect("block span");
        assert_eq!(slice(source, block), "search");
    }

    #[test]
    fn does_not_flag_a_non_nil_result() {
        let (count, violations) = returns("(return 5)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_return_from_without_result() {
        // (return-from foo) already has no result to drop.
        let (count, violations) = returns("(return-from foo)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_return_from_nil_block_only() {
        // (return-from nil) exits the nil block; the nil is the block name.
        let (_, violations) = returns("(return-from nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_bare_return() {
        let (count, violations) = returns("(return)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_nil() {
        let (_, violations) = returns("(RETURN NIL)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "return");
    }

    #[test]
    fn finds_a_nested_return() {
        let (_, violations) = returns("(loop (when done (return nil)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(return nil)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_explicit_nil_returns(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect explicit nil returns");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = returns("(return nil)");
        let summary = summarize_explicit_nil_returns(count, items);

        let quiet = evaluate_explicit_nil_return_policy(
            ExplicitNilReturnPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_explicit_nil_return_policy(
            ExplicitNilReturnPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
