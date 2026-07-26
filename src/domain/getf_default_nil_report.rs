//! Common Lisp `getf`-explicit-`nil`-default detection: a four-argument
//! `(getf plist indicator nil)` whose default value is the literal `nil`. The
//! `default` argument of `getf` *defaults to* `nil`, so `(getf p k nil)` is
//! exactly `(getf p k)` — same value when the indicator is absent. The explicit
//! `nil` restates the default and adds only noise. (Even in a `setf` place a
//! trailing `nil` default is ignored, so dropping it is safe there too.)
//!
//! Only the bare literal `nil` in the default slot is matched. A non-`nil`
//! default (`(getf p k 0)`) is meaningful and left alone, as is a
//! reader-conditional operand.
//!
//! The fix deletes the redundant ` nil` default argument (from the end of the
//! indicator operand through the `nil`), leaving the rest byte-identical, so the
//! rule is auto-fixable.
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

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct GetfDefaultNilItem {
    pub path: PathBuf,
    /// The span of the whole `(getf …)` call form.
    pub span: ByteSpan,
    /// The span to delete: the trailing ` nil` default argument.
    pub removal_span: ByteSpan,
}

#[derive(Debug)]
pub struct GetfDefaultNilSummary {
    pub call_form_count: usize,
    pub violations: Vec<GetfDefaultNilItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct GetfDefaultNilPolicyOptions {
    fail_on_violation: bool,
}

impl GetfDefaultNilPolicyOptions {
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
pub struct GetfDefaultNilPolicy {
    pub fail_on_violation: bool,
    pub call_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    call_form_count: &mut usize,
    violations: &mut Vec<GetfDefaultNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("getf") {
        return;
    }
    *call_form_count += 1;

    // children: [getf, plist, indicator, nil] — exactly the explicit default.
    if view.children.len() != 4 {
        return;
    }
    if !is_nil_literal(&view.children[3]) {
        return;
    }
    let removal_span = ByteSpan::new(view.children[2].span.end(), view.children[3].span.end());
    violations.push(GetfDefaultNilItem {
        path: path.to_path_buf(),
        span: view.span,
        removal_span,
    });
}

/// Collects every `(getf plist indicator nil)` across a whole file, along with
/// the total number of `getf` calls scanned.
pub fn collect_getf_default_nils(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<GetfDefaultNilItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut call_form_count, &mut violations);
        });
    }
    Ok((call_form_count, violations))
}

#[must_use]
pub const fn summarize_getf_default_nils(
    call_form_count: usize,
    violations: Vec<GetfDefaultNilItem>,
) -> GetfDefaultNilSummary {
    GetfDefaultNilSummary {
        call_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_getf_default_nil_policy(
    options: GetfDefaultNilPolicyOptions,
    summary: &GetfDefaultNilSummary,
) -> GetfDefaultNilPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    GetfDefaultNilPolicy {
        fail_on_violation: options.fail_on_violation(),
        call_form_count: summary.call_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(input: &str) -> (usize, Vec<GetfDefaultNilItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_getf_default_nils(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect getf default nils")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_explicit_nil_default() {
        let source = "(getf plist :key nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " nil");
    }

    #[test]
    fn does_not_flag_non_nil_default() {
        let (count, violations) = calls("(getf plist :key 0)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_bare_getf() {
        let (_, violations) = calls("(getf plist :key)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(GETF plist :key nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(when (getf plist :key nil) (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(getf plist :key nil)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_getf_default_nils(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect getf default nils");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(getf plist :key nil)");
        let summary = summarize_getf_default_nils(count, items);

        let quiet =
            evaluate_getf_default_nil_policy(GetfDefaultNilPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_getf_default_nil_policy(GetfDefaultNilPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
