//! Common Lisp `nthcdr`-zero detection: an `(nthcdr 0 list)` whose count
//! operand is the literal `0`. Per CLHS, `nthcdr` with `n` equal to `0` returns
//! the list itself, so `(nthcdr 0 list)` is exactly `list` — same value, no
//! traversal. Dropping the wrapper leaves the plain list the call already
//! yields.
//!
//! Only the bare integer literal `0` is matched. A float `0.0` is left alone:
//! it is not an integer index, so `(nthcdr 0.0 x)` is not the recognized shape.
//! A non-`0` count, a `#x0`/prefixed spelling, a variable count, and a
//! reader-conditional operand are all left alone.
//!
//! The fix rewrites `(nthcdr 0 list)` as `list`, copying the list operand from
//! its exact source, so the rule is auto-fixable.
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

/// Whether `view` is the bare integer `0` literal (no reader prefixes, so `#x0`
/// and a prefixed `,0` are excluded; `0.0` is a different spelling, excluded).
fn is_zero_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("0")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NthcdrZeroItem {
    pub path: PathBuf,
    /// The span of the whole `(nthcdr 0 list)` form.
    pub span: ByteSpan,
    /// The span of the list operand (for reconstructing the fix).
    pub list_span: ByteSpan,
}

#[derive(Debug)]
pub struct NthcdrZeroSummary {
    pub nthcdr_form_count: usize,
    pub violations: Vec<NthcdrZeroItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NthcdrZeroPolicyOptions {
    fail_on_violation: bool,
}

impl NthcdrZeroPolicyOptions {
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
pub struct NthcdrZeroPolicy {
    pub fail_on_violation: bool,
    pub nthcdr_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    nthcdr_form_count: &mut usize,
    violations: &mut Vec<NthcdrZeroItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("nthcdr") {
        return;
    }
    *nthcdr_form_count += 1;

    // children: [nthcdr, count, list] — require exactly the two-operand shape.
    if view.children.len() != 3 {
        return;
    }
    let count = &view.children[1];
    let list = &view.children[2];
    if is_reader_conditional(count) || is_reader_conditional(list) {
        return;
    }
    if !is_zero_literal(count) {
        return;
    }

    violations.push(NthcdrZeroItem {
        path: path.to_path_buf(),
        span: view.span,
        list_span: list.span,
    });
}

/// Collects every `(nthcdr 0 list)` across a whole file, along with the total
/// number of `nthcdr` forms scanned.
pub fn collect_nthcdr_zeros(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NthcdrZeroItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut nthcdr_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut nthcdr_form_count, &mut violations);
        });
    }
    Ok((nthcdr_form_count, violations))
}

#[must_use]
pub const fn summarize_nthcdr_zeros(
    nthcdr_form_count: usize,
    violations: Vec<NthcdrZeroItem>,
) -> NthcdrZeroSummary {
    NthcdrZeroSummary {
        nthcdr_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_nthcdr_zero_policy(
    options: NthcdrZeroPolicyOptions,
    summary: &NthcdrZeroSummary,
) -> NthcdrZeroPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NthcdrZeroPolicy {
        fail_on_violation: options.fail_on_violation(),
        nthcdr_form_count: summary.nthcdr_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nthcdrs(input: &str) -> (usize, Vec<NthcdrZeroItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_nthcdr_zeros(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect nthcdr zeros")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_nthcdr_zero() {
        let source = "(nthcdr 0 items)";
        let (count, violations) = nthcdrs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].list_span), "items");
    }

    #[test]
    fn preserves_a_compound_list() {
        let source = "(nthcdr 0 (rest xs))";
        let (_, violations) = nthcdrs(source);
        assert_eq!(slice(source, violations[0].list_span), "(rest xs)");
    }

    #[test]
    fn does_not_flag_nonzero_index() {
        let (count, violations) = nthcdrs("(nthcdr 1 x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_zero() {
        // (nthcdr 0.0 x) is not an integer index; not the recognized shape.
        let (_, violations) = nthcdrs("(nthcdr 0.0 x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_variable_index() {
        let (_, violations) = nthcdrs("(nthcdr n x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_two_children() {
        // (nthcdr 0) is missing the list operand; not the recognized shape.
        let (count, violations) = nthcdrs("(nthcdr 0)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = nthcdrs("(NTHCDR 0 x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = nthcdrs("(defun f (x) (nthcdr 0 x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(nthcdr 0 x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_nthcdr_zeros(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect nthcdr zeros");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = nthcdrs("(nthcdr 0 x)");
        let summary = summarize_nthcdr_zeros(count, items);

        let quiet = evaluate_nthcdr_zero_policy(NthcdrZeroPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_nthcdr_zero_policy(NthcdrZeroPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
