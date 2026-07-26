//! Common Lisp nth-constant-index detection: an `nth` with a small literal
//! index, which the language provides a named ordinal accessor for — `(nth 0 x)`
//! is `(first x)`, `(nth 1 x)` is `(second x)`, up to `(nth 9 x)` is
//! `(tenth x)`. The standard *defines* `first`…`tenth` as exactly these `nth`
//! calls, so the rewrite is exact (same element, `x` read once, identical
//! nil-on-overrun) and the ordinal name reads more clearly than a bare index.
//!
//! Only a bare decimal literal `0`–`9` is flagged (there is no `eleventh`). A
//! variable or computed index (`(nth i x)`) is genuinely dynamic and left alone,
//! as is any other radix or prefixed spelling.
//!
//! The fix rewrites `(nth N x)` as `(ORDINAL x)`, copying the list argument's
//! source verbatim, so the rule is auto-fixable.
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

/// The ordinal accessor for a bare decimal index `0`–`9`, or `None` otherwise.
fn ordinal_for_index(view: &ExpressionView) -> Option<&'static str> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    match atom_text(view)? {
        "0" => Some("first"),
        "1" => Some("second"),
        "2" => Some("third"),
        "3" => Some("fourth"),
        "4" => Some("fifth"),
        "5" => Some("sixth"),
        "6" => Some("seventh"),
        "7" => Some("eighth"),
        "8" => Some("ninth"),
        "9" => Some("tenth"),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct NthConstantIndexItem {
    pub path: PathBuf,
    /// The span of the whole `(nth N x)` form.
    pub span: ByteSpan,
    /// The ordinal accessor name (`first` for `(nth 0 x)`).
    pub ordinal: &'static str,
    /// The span of the list argument `x` (for reconstructing the fix).
    pub list_span: ByteSpan,
}

#[derive(Debug)]
pub struct NthConstantIndexSummary {
    pub nth_form_count: usize,
    pub violations: Vec<NthConstantIndexItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NthConstantIndexPolicyOptions {
    fail_on_violation: bool,
}

impl NthConstantIndexPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct NthConstantIndexPolicy {
    pub fail_on_violation: bool,
    pub nth_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_nth(
    view: &ExpressionView,
    path: &Path,
    nth_form_count: &mut usize,
    violations: &mut Vec<NthConstantIndexItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("nth") {
        return;
    }
    *nth_form_count += 1;

    // children: [nth, index, list] — exactly the two-argument shape.
    if view.children.len() != 3 {
        return;
    }
    let Some(ordinal) = ordinal_for_index(&view.children[1]) else {
        return;
    };

    violations.push(NthConstantIndexItem {
        path: path.to_path_buf(),
        span: view.span,
        ordinal,
        list_span: view.children[2].span,
    });
}

/// Collects every constant-index `nth` across a whole file, along with the total
/// number of `nth` forms scanned.
pub fn collect_nth_constant_indexes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NthConstantIndexItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut nth_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_nth(subview, path, &mut nth_form_count, &mut violations);
        });
    }
    Ok((nth_form_count, violations))
}

pub fn summarize_nth_constant_indexes(
    nth_form_count: usize,
    violations: Vec<NthConstantIndexItem>,
) -> NthConstantIndexSummary {
    NthConstantIndexSummary {
        nth_form_count,
        violations,
    }
}

pub fn evaluate_nth_constant_index_policy(
    options: NthConstantIndexPolicyOptions,
    summary: &NthConstantIndexSummary,
) -> NthConstantIndexPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NthConstantIndexPolicy {
        fail_on_violation: options.fail_on_violation(),
        nth_form_count: summary.nth_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nths(input: &str) -> (usize, Vec<NthConstantIndexItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_nth_constant_indexes(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect nth constant indexes")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn zero_is_first() {
        let source = "(nth 0 xs)";
        let (count, violations) = nths(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].ordinal, "first");
        assert_eq!(slice(source, violations[0].list_span), "xs");
    }

    #[test]
    fn nine_is_tenth() {
        let (_, violations) = nths("(nth 9 items)");
        assert_eq!(violations[0].ordinal, "tenth");
    }

    #[test]
    fn maps_each_small_index() {
        let expected = [
            ("(nth 1 x)", "second"),
            ("(nth 2 x)", "third"),
            ("(nth 3 x)", "fourth"),
            ("(nth 4 x)", "fifth"),
            ("(nth 5 x)", "sixth"),
            ("(nth 6 x)", "seventh"),
            ("(nth 7 x)", "eighth"),
            ("(nth 8 x)", "ninth"),
        ];
        for (src, ordinal) in expected {
            let (_, violations) = nths(src);
            assert_eq!(violations[0].ordinal, ordinal, "for {src}");
        }
    }

    #[test]
    fn preserves_compound_list_source() {
        let source = "(nth 0 (rest pairs))";
        let (_, violations) = nths(source);
        assert_eq!(slice(source, violations[0].list_span), "(rest pairs)");
    }

    #[test]
    fn does_not_flag_index_ten_or_higher() {
        let (count, violations) = nths("(nth 10 x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_variable_index() {
        let (_, violations) = nths("(nth i x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = nths("(elt x 0)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = nths("(NTH 0 xs)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].ordinal, "first");
    }

    #[test]
    fn finds_a_nested_nth() {
        let (_, violations) = nths("(list (nth 2 row))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].ordinal, "third");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(nth 0 xs)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_nth_constant_indexes(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect nth constant indexes");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = nths("(nth 0 xs)");
        let summary = summarize_nth_constant_indexes(count, items);

        let quiet =
            evaluate_nth_constant_index_policy(NthConstantIndexPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_nth_constant_index_policy(NthConstantIndexPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
