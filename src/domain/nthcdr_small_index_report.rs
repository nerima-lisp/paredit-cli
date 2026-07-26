//! Common Lisp `nthcdr`-small-index detection: an `(nthcdr n list)` whose count
//! operand is a literal `1`, `2`, `3`, or `4`, for which the language provides a
//! named `cdr` accessor — `(nthcdr 1 x)` is `(cdr x)`, `(nthcdr 2 x)` is
//! `(cddr x)`, `(nthcdr 3 x)` is `(cdddr x)`, and `(nthcdr 4 x)` is
//! `(cddddr x)`. The standard *defines* `cddr`…`cddddr` as exactly these nested
//! `cdr` calls, so the rewrite is exact (same tail cons, `x` read once,
//! identical nil-on-overrun) and the accessor name reads more directly than a
//! bare count.
//!
//! Only the bare decimal literals `1`–`4` are matched. The count `0` is
//! [`crate::domain::nthcdr_zero_report`]'s concern (it is the identity, not a
//! `cdr` chain), and there is no `cdddddr`, so `5` and up are left alone. A
//! float `1.0`, a `#x1`/prefixed spelling, a variable count, and a
//! reader-conditional operand are all left alone.
//!
//! The fix rewrites `(nthcdr n list)` as `(ACCESSOR list)`, copying the list
//! operand's source verbatim, so the rule is auto-fixable.
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

/// The named `cdr` accessor for a literal count `1`–`4`, or `None` for any other
/// spelling (`0`, `5`+, floats, prefixed, or non-numeric).
fn small_index_accessor(view: &ExpressionView) -> Option<&'static str> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    match atom_text(view)? {
        "1" => Some("cdr"),
        "2" => Some("cddr"),
        "3" => Some("cdddr"),
        "4" => Some("cddddr"),
        _ => None,
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NthcdrSmallIndexItem {
    pub path: PathBuf,
    /// The span of the whole `(nthcdr n list)` form.
    pub span: ByteSpan,
    /// The named accessor to rewrite to (`cdr`, `cddr`, `cdddr`, `cddddr`).
    pub accessor: &'static str,
    /// The span of the list operand (for reconstructing the fix).
    pub list_span: ByteSpan,
}

#[derive(Debug)]
pub struct NthcdrSmallIndexSummary {
    pub nthcdr_form_count: usize,
    pub violations: Vec<NthcdrSmallIndexItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NthcdrSmallIndexPolicyOptions {
    fail_on_violation: bool,
}

impl NthcdrSmallIndexPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct NthcdrSmallIndexPolicy {
    pub fail_on_violation: bool,
    pub nthcdr_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    nthcdr_form_count: &mut usize,
    violations: &mut Vec<NthcdrSmallIndexItem>,
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
    let Some(accessor) = small_index_accessor(count) else {
        return;
    };

    violations.push(NthcdrSmallIndexItem {
        path: path.to_path_buf(),
        span: view.span,
        accessor,
        list_span: list.span,
    });
}

/// Collects every `(nthcdr 1..=4 list)` across a whole file, along with the
/// total number of `nthcdr` forms scanned.
pub fn collect_nthcdr_small_indexes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NthcdrSmallIndexItem>)> {
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

pub fn summarize_nthcdr_small_indexes(
    nthcdr_form_count: usize,
    violations: Vec<NthcdrSmallIndexItem>,
) -> NthcdrSmallIndexSummary {
    NthcdrSmallIndexSummary {
        nthcdr_form_count,
        violations,
    }
}

pub fn evaluate_nthcdr_small_index_policy(
    options: NthcdrSmallIndexPolicyOptions,
    summary: &NthcdrSmallIndexSummary,
) -> NthcdrSmallIndexPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NthcdrSmallIndexPolicy {
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

    fn nthcdrs(input: &str) -> (usize, Vec<NthcdrSmallIndexItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_nthcdr_small_indexes(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect nthcdr small indexes")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_one_as_cdr() {
        let source = "(nthcdr 1 items)";
        let (count, violations) = nthcdrs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].accessor, "cdr");
        assert_eq!(slice(source, violations[0].list_span), "items");
    }

    #[test]
    fn maps_each_small_index_to_its_accessor() {
        assert_eq!(nthcdrs("(nthcdr 2 x)").1[0].accessor, "cddr");
        assert_eq!(nthcdrs("(nthcdr 3 x)").1[0].accessor, "cdddr");
        assert_eq!(nthcdrs("(nthcdr 4 x)").1[0].accessor, "cddddr");
    }

    #[test]
    fn preserves_a_compound_list() {
        let source = "(nthcdr 2 (rest xs))";
        let (_, violations) = nthcdrs(source);
        assert_eq!(slice(source, violations[0].list_span), "(rest xs)");
    }

    #[test]
    fn does_not_flag_zero() {
        // (nthcdr 0 x) is the identity, nthcdr-zero's concern.
        let (count, violations) = nthcdrs("(nthcdr 0 x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_five_or_more() {
        // There is no cdddddr, so 5 has no named accessor.
        let (_, violations) = nthcdrs("(nthcdr 5 x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_or_variable() {
        assert!(nthcdrs("(nthcdr 1.0 x)").1.is_empty());
        assert!(nthcdrs("(nthcdr n x)").1.is_empty());
    }

    #[test]
    fn does_not_flag_missing_list_operand() {
        let (count, violations) = nthcdrs("(nthcdr 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = nthcdrs("(NTHCDR 3 x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].accessor, "cdddr");
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = nthcdrs("(defun f (x) (nthcdr 1 x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(nthcdr 1 x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_nthcdr_small_indexes(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect nthcdr small indexes");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = nthcdrs("(nthcdr 1 x)");
        let summary = summarize_nthcdr_small_indexes(count, items);

        let quiet =
            evaluate_nthcdr_small_index_policy(NthcdrSmallIndexPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_nthcdr_small_index_policy(NthcdrSmallIndexPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
