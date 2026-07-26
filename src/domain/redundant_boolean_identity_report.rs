//! Common Lisp redundant-boolean-identity detection: a boolean operator whose
//! operand list contains its *identity element*, which contributes nothing —
//! `t` in an `and` (`(and a t b)` is `(and a b)`) or `nil` in an `or`
//! (`(or a nil b)` is `(or a b)`). The identity operand is evaluated for no
//! effect and just clutters the form.
//!
//! This is the complement of [`crate::domain::dead_boolean_operand_report`],
//! which handles the *dominant* elements (`nil` in `and`, `t` in `or`) that make
//! later operands dead. Here the constant changes nothing and is simply dropped.
//!
//! The operators differ about the *last* operand:
//!
//!   - `or`: `nil` is removable at any position — an exhausted `or` yields `nil`
//!     anyway, so a trailing `nil` is redundant too.
//!   - `and`: `t` is removable only when it is *not* the last operand. A trailing
//!     `t` is `and`'s return value (`(and a t)` yields `t`, not `a`), so it must
//!     be kept.
//!
//! Only a bare `t`/`nil` symbol counts (a reader-prefixed atom is left alone),
//! and a single-operand form (`(and t)`, `(or nil)`) is left to
//! `single-operand-boolean`. The fix reconstructs `(op kept…)` from the surviving
//! operands' source, collapsing to the bare identity (`t`/`nil`) when every
//! operand was removed, so the rule is auto-fixable.
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

/// The canonical operator name (`and`/`or`) and its identity element (`t`/`nil`)
/// for a boolean head, or `None` otherwise.
fn boolean_identity(head: &str) -> Option<(&'static str, &'static str)> {
    if head.eq_ignore_ascii_case("and") {
        Some(("and", "t"))
    } else if head.eq_ignore_ascii_case("or") {
        Some(("or", "nil"))
    } else {
        None
    }
}

/// Whether `view` is the bare literal `identity` symbol (no reader prefixes).
fn is_identity_literal(view: &ExpressionView, identity: &str) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(identity))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct RedundantBooleanIdentityItem {
    pub path: PathBuf,
    /// The span of the whole `(and …)`/`(or …)` form.
    pub span: ByteSpan,
    /// The operator, lowercased (`and` or `or`).
    pub operator: &'static str,
    /// The removed identity element (`t` for `and`, `nil` for `or`).
    pub identity: &'static str,
    /// The spans of the operands to keep, in order (empty means the form
    /// collapses to the bare identity).
    pub kept_spans: Vec<ByteSpan>,
}

#[derive(Debug)]
pub struct RedundantBooleanIdentitySummary {
    pub boolean_form_count: usize,
    pub violations: Vec<RedundantBooleanIdentityItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantBooleanIdentityPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantBooleanIdentityPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct RedundantBooleanIdentityPolicy {
    pub fail_on_violation: bool,
    pub boolean_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_boolean(
    view: &ExpressionView,
    path: &Path,
    boolean_form_count: &mut usize,
    violations: &mut Vec<RedundantBooleanIdentityItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some((operator, identity)) = boolean_identity(head) else {
        return;
    };
    *boolean_form_count += 1;

    let operands = &view.children[1..];
    // A single-operand form is single-operand-boolean's job; an empty form is
    // the bare identity already.
    if operands.len() < 2 {
        return;
    }
    if operands.iter().any(is_reader_conditional) {
        return;
    }

    let last_index = operands.len() - 1;
    let mut kept = Vec::new();
    let mut removed_any = false;
    for (index, operand) in operands.iter().enumerate() {
        // `t` in `and` is kept when it is the last operand (the return value);
        // `nil` in `or` is always removable.
        let removable =
            is_identity_literal(operand, identity) && (operator == "or" || index != last_index);
        if removable {
            removed_any = true;
        } else {
            kept.push(operand.span);
        }
    }

    if removed_any {
        violations.push(RedundantBooleanIdentityItem {
            path: path.to_path_buf(),
            span: view.span,
            operator,
            identity,
            kept_spans: kept,
        });
    }
}

/// Collects every boolean form with a redundant identity operand across a whole
/// file, along with the total number of `and`/`or` forms scanned.
pub fn collect_redundant_boolean_identities(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantBooleanIdentityItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut boolean_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_boolean(subview, path, &mut boolean_form_count, &mut violations)
        });
    }
    Ok((boolean_form_count, violations))
}

pub fn summarize_redundant_boolean_identities(
    boolean_form_count: usize,
    violations: Vec<RedundantBooleanIdentityItem>,
) -> RedundantBooleanIdentitySummary {
    RedundantBooleanIdentitySummary {
        boolean_form_count,
        violations,
    }
}

pub fn evaluate_redundant_boolean_identity_policy(
    options: RedundantBooleanIdentityPolicyOptions,
    summary: &RedundantBooleanIdentitySummary,
) -> RedundantBooleanIdentityPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantBooleanIdentityPolicy {
        fail_on_violation: options.fail_on_violation(),
        boolean_form_count: summary.boolean_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn booleans(input: &str) -> (usize, Vec<RedundantBooleanIdentityItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_boolean_identities(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("collect redundant boolean identities")
    }

    fn kept<'a>(source: &'a str, item: &RedundantBooleanIdentityItem) -> Vec<&'a str> {
        item.kept_spans
            .iter()
            .map(|s| &source[s.start().get()..s.end().get()])
            .collect()
    }

    #[test]
    fn removes_middle_t_from_and() {
        let source = "(and a t b)";
        let (count, violations) = booleans(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "and");
        assert_eq!(kept(source, &violations[0]), vec!["a", "b"]);
    }

    #[test]
    fn removes_nil_from_or_at_any_position() {
        let source = "(or a nil)";
        let (_, violations) = booleans(source);
        assert_eq!(kept(source, &violations[0]), vec!["a"]);

        let source2 = "(or nil a)";
        let (_, v2) = booleans(source2);
        assert_eq!(kept(source2, &v2[0]), vec!["a"]);
    }

    #[test]
    fn keeps_trailing_t_in_and() {
        // (and a t) yields t, not a — the trailing t is the return value.
        let (_, violations) = booleans("(and a t)");
        assert!(violations.is_empty());
    }

    #[test]
    fn or_with_all_nil_collapses_to_empty_kept() {
        let (_, violations) = booleans("(or nil nil)");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].kept_spans.is_empty());
    }

    #[test]
    fn does_not_flag_dominant_elements() {
        // nil in and and t in or are dead-boolean-operand's job, not this rule's.
        assert!(booleans("(and a nil b)").1.is_empty());
        assert!(booleans("(or a t b)").1.is_empty());
    }

    #[test]
    fn does_not_flag_single_operand() {
        assert!(booleans("(and t)").1.is_empty());
        assert!(booleans("(or nil)").1.is_empty());
    }

    #[test]
    fn does_not_flag_without_identity_operand() {
        let (count, violations) = booleans("(and a b c)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_operator_and_identity() {
        let (_, violations) = booleans("(AND x T y)");
        assert_eq!(violations.len(), 1);
        assert_eq!(kept("(AND x T y)", &violations[0]), vec!["x", "y"]);
    }

    #[test]
    fn finds_a_nested_form() {
        let (_, violations) = booleans("(when (or ready nil) (go))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "or");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(and a t b)", Dialect::Clojure).expect("parse");
        let (count, violations) = collect_redundant_boolean_identities(
            &PathBuf::from("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("collect redundant boolean identities");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = booleans("(and a t b)");
        let summary = summarize_redundant_boolean_identities(count, items);

        let quiet = evaluate_redundant_boolean_identity_policy(
            RedundantBooleanIdentityPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_redundant_boolean_identity_policy(
            RedundantBooleanIdentityPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
