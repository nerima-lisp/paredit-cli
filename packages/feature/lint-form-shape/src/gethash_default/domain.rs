//! Common Lisp `gethash`-explicit-`nil`-default detection: a three-argument
//! `(gethash key table nil)` whose default value is the literal `nil`. The
//! `default` argument of `gethash` *defaults to* `nil`, so `(gethash k h nil)` is
//! exactly `(gethash k h)` — same primary value and same second (present-p)
//! return value. The explicit `nil` restates the default and adds only noise.
//!
//! Only the bare literal `nil` in the default slot is matched. A non-`nil`
//! default (`(gethash k h 0)`) is meaningful and left alone, as is a
//! reader-conditional operand.
//!
//! The fix deletes the redundant ` nil` default argument (from the end of the
//! table operand through the `nil`), leaving the rest byte-identical, so the rule
//! is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// Whether `view` is the bare literal `nil` (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct GethashDefaultItem {
    pub path: PathBuf,
    /// The span of the whole `(gethash k h nil)` form.
    pub span: ByteSpan,
    /// The span to delete: the ` nil` default, from the end of the table operand
    /// through the `nil`.
    pub removal_span: ByteSpan,
}

#[derive(Debug)]
pub struct GethashDefaultSummary {
    pub gethash_form_count: usize,
    pub violations: Vec<GethashDefaultItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct GethashDefaultPolicyOptions {
    fail_on_violation: bool,
}

impl GethashDefaultPolicyOptions {
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
pub struct GethashDefaultPolicy {
    pub fail_on_violation: bool,
    pub gethash_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    gethash_form_count: &mut usize,
    violations: &mut Vec<GethashDefaultItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("gethash") {
        return;
    }
    *gethash_form_count += 1;

    // children: [gethash, key, table, default] — require the three-operand shape.
    if view.children.len() != 4 {
        return;
    }
    let key = &view.children[1];
    let table = &view.children[2];
    let default = &view.children[3];
    if is_reader_conditional(key) || is_reader_conditional(table) {
        return;
    }
    if !is_nil_literal(default) {
        return;
    }

    // Delete from the end of the table operand through the `nil`, so the leading
    // whitespace before `nil` goes too.
    let removal_span = ByteSpan::new(table.span.end(), default.span.end());
    violations.push(GethashDefaultItem {
        path: path.to_path_buf(),
        span: view.span,
        removal_span,
    });
}

/// Collects every `(gethash k h nil)` across a whole file, along with the total
/// number of `gethash` forms scanned.
pub fn collect_gethash_defaults(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<GethashDefaultItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut gethash_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut gethash_form_count, &mut violations);
        });
    }
    Ok((gethash_form_count, violations))
}

#[must_use]
pub const fn summarize_gethash_defaults(
    gethash_form_count: usize,
    violations: Vec<GethashDefaultItem>,
) -> GethashDefaultSummary {
    GethashDefaultSummary {
        gethash_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_gethash_default_policy(
    options: GethashDefaultPolicyOptions,
    summary: &GethashDefaultSummary,
) -> GethashDefaultPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    GethashDefaultPolicy {
        fail_on_violation: options.fail_on_violation(),
        gethash_form_count: summary.gethash_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gethashes(input: &str) -> (usize, Vec<GethashDefaultItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_gethash_defaults(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect gethash defaults")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_gethash_nil_default() {
        let source = "(gethash k table nil)";
        let (count, violations) = gethashes(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " nil");
    }

    #[test]
    fn does_not_flag_non_nil_default() {
        let (count, violations) = gethashes("(gethash k table 0)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_two_argument_gethash() {
        let (count, violations) = gethashes("(gethash k table)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_key_named_nil_literally() {
        // nil in the key or table slot is a real argument, not the default.
        let (_, violations) = gethashes("(gethash nil table)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = gethashes("(GETHASH k table NIL)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = gethashes("(defun f (k h) (gethash k h nil))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(gethash k h nil)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_gethash_defaults(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect gethash defaults");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = gethashes("(gethash k h nil)");
        let summary = summarize_gethash_defaults(count, items);

        let quiet =
            evaluate_gethash_default_policy(GethashDefaultPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_gethash_default_policy(GethashDefaultPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
