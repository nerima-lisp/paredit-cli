//! Common Lisp default-`eql`-search-for-a-literal detection: a sequence search
//! or membership function whose searched item is a *string* or *quoted list*
//! literal and which passes no `:test`. These functions default their test to
//! `eql`, and `eql` compares strings and conses by object identity — two string
//! or list literals are distinct objects — so `(member "x" list)`,
//! `(assoc '(a) alist)`, or `(substitute y "x" seq)` silently never matches,
//! regardless of contents. The fix is an explicit `:test #'equal`
//! (or `#'string=`).
//!
//! The searched item's argument position differs per function — first for
//! `member`/`assoc`/`rassoc`/`find`/`position`/`count`/`remove`/`delete`/
//! `adjoin`/`pushnew`, second (the `old` value) for `substitute`/`nsubstitute`/
//! `subst`/`nsubst`. The item must be a string literal or a quoted, non-empty
//! list literal (numbers, characters, keywords, and symbols are compared
//! correctly by `eql` and are never flagged). A call that already passes
//! `:test` or `:test-not` is left alone.
//!
//! This is the search-function sibling of `eql-string-comparison` and
//! `eql-list-comparison`, which cover the same identity pitfall for `eq`/`eql`.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::expression_equality::render_expression;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// The 0-based index of the searched item argument for a default-`eql` search
/// function, or `None` if the head is not one of them. Required positionals sit
/// at fixed indices (keyword arguments follow), so the position is stable.
fn item_index(head: &str) -> Option<usize> {
    let eq = |name: &str| head.eq_ignore_ascii_case(name);
    if eq("member")
        || eq("assoc")
        || eq("rassoc")
        || eq("find")
        || eq("position")
        || eq("count")
        || eq("remove")
        || eq("delete")
        || eq("adjoin")
        || eq("pushnew")
    {
        Some(1)
    } else if eq("substitute") || eq("nsubstitute") || eq("subst") || eq("nsubst") {
        Some(2)
    } else {
        None
    }
}

/// Whether `view` is a string literal (`"…"`).
fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

/// Whether `view` is a quoted, non-empty list literal — `'(a b)` or
/// `(quote (a b))`. An empty quoted list (`'()`) is `nil` and eql-comparable.
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

/// Whether any argument is a `:test` or `:test-not` keyword.
fn has_explicit_test(view: &ExpressionView) -> bool {
    view.children.iter().skip(1).any(|child| {
        atom_text(child).is_some_and(|text| {
            text.eq_ignore_ascii_case(":test") || text.eq_ignore_ascii_case(":test-not")
        })
    })
}

#[derive(Debug, Clone)]
pub struct EqlSearchLiteralItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The search function (`member`, `assoc`, …).
    pub operator: String,
    /// A rendered form of the string/list literal being searched for.
    pub literal: String,
}

#[derive(Debug)]
pub struct EqlSearchLiteralSummary {
    pub search_call_count: usize,
    pub violations: Vec<EqlSearchLiteralItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct EqlSearchLiteralPolicyOptions {
    fail_on_violation: bool,
}

impl EqlSearchLiteralPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct EqlSearchLiteralPolicy {
    pub fail_on_violation: bool,
    pub search_call_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_call(
    view: &ExpressionView,
    path: &Path,
    search_call_count: &mut usize,
    violations: &mut Vec<EqlSearchLiteralItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(index) = item_index(head) else {
        return;
    };
    *search_call_count += 1;

    let Some(item) = view.children.get(index) else {
        return;
    };
    if (is_string_literal(item) || is_quoted_list_literal(item)) && !has_explicit_test(view) {
        violations.push(EqlSearchLiteralItem {
            path: path.to_path_buf(),
            span: view.span,
            operator: head.to_ascii_lowercase(),
            literal: render_expression(item),
        });
    }
}

/// Collects every default-eql search for a string/list literal across a whole
/// file, along with the total number of search calls scanned.
pub fn collect_eql_search_literals(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<EqlSearchLiteralItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut search_call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(subview, path, &mut search_call_count, &mut violations)
        });
    }
    Ok((search_call_count, violations))
}

pub fn summarize_eql_search_literals(
    search_call_count: usize,
    violations: Vec<EqlSearchLiteralItem>,
) -> EqlSearchLiteralSummary {
    EqlSearchLiteralSummary {
        search_call_count,
        violations,
    }
}

pub fn evaluate_eql_search_literal_policy(
    options: EqlSearchLiteralPolicyOptions,
    summary: &EqlSearchLiteralSummary,
) -> EqlSearchLiteralPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    EqlSearchLiteralPolicy {
        fail_on_violation: options.fail_on_violation(),
        search_call_count: summary.search_call_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn searches(input: &str) -> (usize, Vec<EqlSearchLiteralItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_eql_search_literals(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect eql search literals")
    }

    #[test]
    fn flags_member_of_a_string_literal() {
        let (count, violations) = searches("(member \"x\" items)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "member");
    }

    #[test]
    fn flags_assoc_of_a_quoted_list() {
        let (_, violations) = searches("(assoc '(a b) alist)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "assoc");
    }

    #[test]
    fn flags_find_and_position_of_a_string() {
        let (_, violations) = searches("(find \"a\" xs)");
        assert_eq!(violations.len(), 1);
        let (_, violations) = searches("(position \"a\" xs)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_when_test_is_given() {
        let (count, violations) = searches("(member \"x\" items :test #'equal)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_test_not() {
        let (_, violations) = searches("(assoc '(a) alist :test-not #'equal)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_substitute_of_a_string_old_value() {
        // substitute searches for its SECOND argument (old).
        let (_, violations) = searches("(substitute new \"x\" seq)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "substitute");
    }

    #[test]
    fn does_not_flag_substitute_new_value_literal() {
        // The literal is the NEW value (index 1), not the searched old value.
        let (_, violations) = searches("(substitute \"n\" old seq)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_adjoin_and_pushnew_of_a_string() {
        let (_, violations) = searches("(adjoin \"x\" set)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "adjoin");
        let (_, violations) = searches("(pushnew \"x\" places)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "pushnew");
    }

    #[test]
    fn does_not_flag_a_number_or_keyword_item() {
        // eql compares numbers and keywords correctly.
        let (_, violations) = searches("(member 5 items)");
        assert!(violations.is_empty());
        let (_, violations) = searches("(find :k items)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_variable_item() {
        let (_, violations) = searches("(member x items)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_quoted_list() {
        let (_, violations) = searches("(member '() items)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_search_functions() {
        let (count, violations) = searches("(list \"x\" items)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = searches("(MEMBER \"x\" items)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(member \"x\" items)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_eql_search_literals(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect eql search literals");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = searches("(member \"x\" items)");
        let summary = summarize_eql_search_literals(count, items);

        let quiet =
            evaluate_eql_search_literal_policy(EqlSearchLiteralPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_eql_search_literal_policy(EqlSearchLiteralPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
