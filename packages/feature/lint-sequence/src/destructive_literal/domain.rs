//! Common Lisp destructive-operation-on-a-literal detection: a destructive
//! sequence function whose sequence argument is a *quoted list literal* —
//! `(nreverse '(a b c))`, `(sort '(3 1 2) #'<)`, `(nbutlast '(1 2 3))`. These
//! functions are permitted to modify (and reuse the cells of) their argument,
//! but a quoted literal is a constant: the standard leaves the effect of
//! modifying it undefined, and implementations may coalesce identical literals
//! or place them in read-only memory. The result is a latent bug that "works"
//! until the literal is shared — the fix is to build a fresh list with
//! `(list …)` or `copy-list`.
//!
//! Destructive functions are covered with the argument position(s) where a
//! quoted literal would be modified — first-argument sequences (`nreverse`,
//! `nreconc`, `sort`, `stable-sort`, `nbutlast`, `delete-duplicates`,
//! `rplaca`, `rplacd`), later-argument sequences (`delete`/`delete-if`,
//! `nsublis`, `nsubstitute`/`nsubst` and their variants), the two set-operation
//! lists (`nunion`, `nintersection`, `nset-difference`, `nset-exclusive-or`),
//! and every argument but the last of `nconc`. Only a quoted, *non-empty* list
//! literal is flagged (`'()` is `nil`, harmless); both the reader form (`'(…)`)
//! and the explicit `(quote (…))` are recognized. A fresh list (`(list …)`), a
//! variable, or a `copy-list` result is left alone.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};

/// The argument indices at which a destructive function may modify its
/// argument (so a quoted literal there is undefined behavior), or `None` if the
/// head is not a covered destructive function. Required positional arguments sit
/// at fixed indices — keyword arguments follow them — so these positions are
/// stable regardless of `:test`/`:key`/etc.
fn sequence_indices(head: &str, child_count: usize) -> Option<Vec<usize>> {
    let eq = |name: &str| head.eq_ignore_ascii_case(name);
    if eq("nreverse")
        || eq("nreconc")
        || eq("sort")
        || eq("stable-sort")
        || eq("nbutlast")
        || eq("delete-duplicates")
        || eq("rplaca")
        || eq("rplacd")
    {
        Some(vec![1])
    } else if eq("delete") || eq("delete-if") || eq("delete-if-not") || eq("nsublis") {
        Some(vec![2])
    } else if eq("nsubstitute")
        || eq("nsubstitute-if")
        || eq("nsubstitute-if-not")
        || eq("nsubst")
        || eq("nsubst-if")
        || eq("nsubst-if-not")
    {
        Some(vec![3])
    } else if eq("nunion")
        || eq("nintersection")
        || eq("nset-difference")
        || eq("nset-exclusive-or")
    {
        Some(vec![1, 2])
    } else if eq("nconc") {
        // nconc modifies every argument except the last.
        Some((1..child_count.saturating_sub(1)).collect())
    } else {
        None
    }
}

/// Whether `view` is a quoted, non-empty list — either `'(a b)` (a paren list
/// carrying a `Quote` reader prefix) or `(quote (a b))`. An empty quoted list
/// (`'()`) is `nil` and is not a modifiable list literal.
fn is_quoted_list_literal(view: &ExpressionView) -> bool {
    if is_paren_list(view)
        && view.reader_prefixes.contains(&ReaderPrefix::Quote)
        && !view.children.is_empty()
    {
        return true;
    }

    if list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("quote")) {
        if let Some(quoted) = view.children.get(1) {
            return is_paren_list(quoted) && !quoted.children.is_empty();
        }
    }

    false
}

#[derive(Debug, Clone)]
pub struct DestructiveLiteralItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The destructive operator (`nreverse`, `sort`, …).
    pub operator: String,
    /// A rendered form of the quoted literal being modified.
    pub literal: String,
}

#[derive(Debug)]
pub struct DestructiveLiteralSummary {
    pub destructive_call_count: usize,
    pub violations: Vec<DestructiveLiteralItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DestructiveLiteralPolicyOptions {
    fail_on_violation: bool,
}

impl DestructiveLiteralPolicyOptions {
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
pub struct DestructiveLiteralPolicy {
    pub fail_on_violation: bool,
    pub destructive_call_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_call(
    view: &ExpressionView,
    path: &Path,
    destructive_call_count: &mut usize,
    violations: &mut Vec<DestructiveLiteralItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(indices) = sequence_indices(head, view.children.len()) else {
        return;
    };
    *destructive_call_count += 1;

    // Report the first argument slot that holds a quoted literal; one call with
    // two literal arguments is still one form's bug.
    for index in indices {
        if let Some(sequence) = view.children.get(index) {
            if is_quoted_list_literal(sequence) {
                violations.push(DestructiveLiteralItem {
                    path: path.to_path_buf(),
                    span: view.span,
                    operator: head.to_ascii_lowercase(),
                    literal: render_expression(sequence),
                });
                return;
            }
        }
    }
}

/// Collects every destructive call on a quoted list literal across a whole
/// file, along with the total number of destructive calls scanned.
pub fn collect_destructive_literals(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DestructiveLiteralItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut destructive_call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(subview, path, &mut destructive_call_count, &mut violations);
        });
    }
    Ok((destructive_call_count, violations))
}

#[must_use]
pub const fn summarize_destructive_literals(
    destructive_call_count: usize,
    violations: Vec<DestructiveLiteralItem>,
) -> DestructiveLiteralSummary {
    DestructiveLiteralSummary {
        destructive_call_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_destructive_literal_policy(
    options: DestructiveLiteralPolicyOptions,
    summary: &DestructiveLiteralSummary,
) -> DestructiveLiteralPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    DestructiveLiteralPolicy {
        fail_on_violation: options.fail_on_violation(),
        destructive_call_count: summary.destructive_call_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(input: &str) -> (usize, Vec<DestructiveLiteralItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_destructive_literals(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect destructive literals")
    }

    #[test]
    fn flags_nreverse_of_a_quoted_list() {
        let (count, violations) = calls("(nreverse '(a b c))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "nreverse");
    }

    #[test]
    fn flags_sort_of_a_quoted_list() {
        let (_, violations) = calls("(sort '(3 1 2) #'<)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "sort");
    }

    #[test]
    fn flags_an_explicit_quote_form() {
        let (_, violations) = calls("(stable-sort (quote (2 1)) #'<)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "stable-sort");
    }

    #[test]
    fn does_not_flag_a_fresh_list() {
        let (count, violations) = calls("(nreverse (list a b c))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_variable_sequence() {
        let (_, violations) = calls("(sort xs #'<)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_symbol_or_empty_list() {
        let (_, violations) = calls("(nreverse 'foo)");
        assert!(violations.is_empty());
        let (_, violations) = calls("(nreverse '())");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_delete_with_a_literal_sequence() {
        // delete's sequence is the second argument.
        let (_, violations) = calls("(delete x '(1 2 3))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "delete");
    }

    #[test]
    fn does_not_flag_delete_of_a_literal_item() {
        // The literal here is the item being deleted, not the sequence.
        let (_, violations) = calls("(delete '(1 2) xs)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_nsubstitute_with_a_literal_sequence() {
        // nsubstitute's sequence is the third argument.
        let (_, violations) = calls("(nsubstitute a b '(1 2 3))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "nsubstitute");
    }

    #[test]
    fn flags_a_set_operation_literal_in_either_position() {
        let (_, violations) = calls("(nunion '(1 2) xs)");
        assert_eq!(violations.len(), 1);
        let (_, violations) = calls("(nintersection xs '(3 4))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_rplaca_of_a_literal_cons() {
        let (_, violations) = calls("(rplaca '(1 2) 9)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "rplaca");
    }

    #[test]
    fn flags_a_non_last_nconc_literal_but_not_the_last() {
        // nconc modifies all but the last argument.
        let (_, violations) = calls("(nconc '(1 2) xs)");
        assert_eq!(violations.len(), 1);
        // A literal as the LAST nconc argument is not modified.
        let (_, violations) = calls("(nconc xs '(1 2))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_destructive_functions() {
        // reverse (non-destructive) copies, so a literal is fine.
        let (count, violations) = calls("(reverse '(1 2 3))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = calls("(NREVERSE '(1 2))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_call_nested_in_a_body() {
        let (_, violations) = calls("(defun f () (nbutlast '(1 2 3)))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "nbutlast");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(nreverse '(1 2))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_destructive_literals(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect destructive literals");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(nreverse '(1 2))");
        let summary = summarize_destructive_literals(count, items);

        let quiet = evaluate_destructive_literal_policy(
            DestructiveLiteralPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_destructive_literal_policy(
            DestructiveLiteralPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
