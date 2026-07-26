//! Common Lisp cleanupless-`unwind-protect` detection: an `(unwind-protect x)`
//! with no cleanup forms. `unwind-protect` evaluates the protected form and then
//! runs its cleanup forms on any exit; with zero cleanup forms there is nothing
//! to protect, so it is exactly `x` — a wrapper left behind (often after the
//! cleanup forms were deleted).
//!
//! Only the exact no-cleanup shape `(unwind-protect x)` is matched; an
//! `unwind-protect` with any cleanup form is a real construct and left alone, as
//! is a reader-conditional protected form.
//!
//! The fix replaces the whole form with the protected form's source, so the rule
//! is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct UnwindProtectNoCleanupItem {
    pub path: PathBuf,
    /// The span of the whole `(unwind-protect x)` form.
    pub span: ByteSpan,
    /// The span of the protected form `x` (for reconstructing the fix).
    pub form_span: ByteSpan,
}

#[derive(Debug)]
pub struct UnwindProtectNoCleanupSummary {
    pub unwind_protect_form_count: usize,
    pub violations: Vec<UnwindProtectNoCleanupItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct UnwindProtectNoCleanupPolicyOptions {
    fail_on_violation: bool,
}

impl UnwindProtectNoCleanupPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct UnwindProtectNoCleanupPolicy {
    pub fail_on_violation: bool,
    pub unwind_protect_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    unwind_protect_form_count: &mut usize,
    violations: &mut Vec<UnwindProtectNoCleanupItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("unwind-protect") {
        return;
    }
    *unwind_protect_form_count += 1;

    // children: [unwind-protect, protected] — no cleanup forms.
    if view.children.len() != 2 {
        return;
    }
    let form = &view.children[1];
    if is_reader_conditional(form) {
        return;
    }

    violations.push(UnwindProtectNoCleanupItem {
        path: path.to_path_buf(),
        span: view.span,
        form_span: form.span,
    });
}

/// Collects every cleanupless `(unwind-protect x)` across a whole file, along
/// with the total number of `unwind-protect` forms scanned.
pub fn collect_unwind_protect_no_cleanup(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<UnwindProtectNoCleanupItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut unwind_protect_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(
                subview,
                path,
                &mut unwind_protect_form_count,
                &mut violations,
            );
        });
    }
    Ok((unwind_protect_form_count, violations))
}

pub fn summarize_unwind_protect_no_cleanup(
    unwind_protect_form_count: usize,
    violations: Vec<UnwindProtectNoCleanupItem>,
) -> UnwindProtectNoCleanupSummary {
    UnwindProtectNoCleanupSummary {
        unwind_protect_form_count,
        violations,
    }
}

pub fn evaluate_unwind_protect_no_cleanup_policy(
    options: UnwindProtectNoCleanupPolicyOptions,
    summary: &UnwindProtectNoCleanupSummary,
) -> UnwindProtectNoCleanupPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    UnwindProtectNoCleanupPolicy {
        fail_on_violation: options.fail_on_violation(),
        unwind_protect_form_count: summary.unwind_protect_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ups(input: &str) -> (usize, Vec<UnwindProtectNoCleanupItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_unwind_protect_no_cleanup(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect unwind-protect no cleanup")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_cleanupless_unwind_protect() {
        let source = "(unwind-protect (compute))";
        let (count, violations) = ups(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].form_span), "(compute)");
    }

    #[test]
    fn does_not_flag_unwind_protect_with_cleanup() {
        assert!(ups("(unwind-protect (compute) (cleanup))").1.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = ups("(UNWIND-PROTECT x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = ups("(defun f (x) (unwind-protect x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(unwind-protect x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_unwind_protect_no_cleanup(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect unwind-protect no cleanup");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = ups("(unwind-protect x)");
        let summary = summarize_unwind_protect_no_cleanup(count, items);

        let quiet = evaluate_unwind_protect_no_cleanup_policy(
            UnwindProtectNoCleanupPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_unwind_protect_no_cleanup_policy(
            UnwindProtectNoCleanupPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
