//! Common Lisp quoted-`case`-key detection: a `case`, `ccase`, or `ecase`
//! clause whose key designator is quoted — `(case x ('a 1) …)`. `case` keys
//! are **not** evaluated, and `'a` reads as `(quote a)`, so the key designator
//! `(quote a)` is a *list* of the two keys `quote` and `a`. The clause
//! therefore matches the symbols `quote` or `a`, never the value the author
//! meant — almost always a bug where `(case x (a 1) …)` was intended.
//!
//! Both surface forms are detected: the reader-sugar `'a` (an atom carrying a
//! `Quote` prefix) and the explicit `(quote a)` list, whether the key
//! designator is a single key or one element of a key list (`('a 'b)`).
//!
//! Scoped to `case`/`ccase`/`ecase` — the `eql`-key forms. `typecase`'s clause
//! heads are type specifiers, a different shape, and are not inspected here.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`] and the display rendering
//! from [`crate::domain::expression_equality`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::expression_equality::render_expression;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree};
use crate::domain::view_query::{for_each_subview, is_paren_list, list_head};

const CASE_HEADS: [&str; 3] = ["case", "ccase", "ecase"];

/// Whether one key form is quoted — the reader-sugar `'x` (a `Quote` prefix)
/// or the explicit `(quote x)` list.
fn is_quoted_form(view: &ExpressionView) -> bool {
    view.reader_prefixes.contains(&ReaderPrefix::Quote)
        || list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("quote"))
}

/// Whether a clause's key designator is quoted: either the designator itself is
/// a quoted form, or it is a key list with a quoted element.
fn key_designator_is_quoted(key_designator: &ExpressionView) -> bool {
    if is_quoted_form(key_designator) {
        return true;
    }
    is_paren_list(key_designator) && key_designator.children.iter().any(is_quoted_form)
}

#[derive(Debug, Clone)]
pub struct QuotedCaseKeyItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub head: String,
    pub key: String,
}

#[derive(Debug)]
pub struct QuotedCaseKeySummary {
    pub case_form_count: usize,
    pub violations: Vec<QuotedCaseKeyItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct QuotedCaseKeyPolicyOptions {
    fail_on_violation: bool,
}

impl QuotedCaseKeyPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct QuotedCaseKeyPolicy {
    pub fail_on_violation: bool,
    pub case_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_case(
    view: &ExpressionView,
    path: &Path,
    case_form_count: &mut usize,
    violations: &mut Vec<QuotedCaseKeyItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !CASE_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted case form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    *case_form_count += 1;

    // The keyform is child 1; clauses start at child 2. A feature-conditional
    // clause reads as an opaque atom (not a list) and is skipped.
    for clause in view.children.iter().skip(2) {
        if !is_paren_list(clause) {
            continue;
        }
        let Some(key_designator) = clause.children.first() else {
            continue;
        };
        if key_designator_is_quoted(key_designator) {
            violations.push(QuotedCaseKeyItem {
                path: path.to_path_buf(),
                span: key_designator.span,
                head: head.to_owned(),
                key: render_expression(key_designator),
            });
        }
    }
}

/// Collects every `case`/`ccase`/`ecase` clause with a quoted key designator
/// across a whole file, along with the total number of such forms scanned.
pub fn collect_quoted_case_keys(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<QuotedCaseKeyItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, path, &mut case_form_count, &mut violations);
        });
    }
    Ok((case_form_count, violations))
}

pub fn summarize_quoted_case_keys(
    case_form_count: usize,
    violations: Vec<QuotedCaseKeyItem>,
) -> QuotedCaseKeySummary {
    QuotedCaseKeySummary {
        case_form_count,
        violations,
    }
}

pub fn evaluate_quoted_case_key_policy(
    options: QuotedCaseKeyPolicyOptions,
    summary: &QuotedCaseKeySummary,
) -> QuotedCaseKeyPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    QuotedCaseKeyPolicy {
        fail_on_violation: options.fail_on_violation(),
        case_form_count: summary.case_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(input: &str) -> (usize, Vec<QuotedCaseKeyItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_quoted_case_keys(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect quoted case keys")
    }

    #[test]
    fn flags_a_reader_sugar_quoted_key() {
        let (case_form_count, items) = keys("(case x ('a 1) (b 2))");
        assert_eq!(case_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "case");
    }

    #[test]
    fn flags_an_explicit_quote_key() {
        let (_, items) = keys("(case x ((quote a) 1))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_a_quoted_element_in_a_key_list() {
        let (_, items) = keys("(case x (('a 'b) 1))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn does_not_flag_ordinary_keys() {
        let (case_form_count, items) = keys("(case x (a 1) (b 2) (t 3))");
        assert_eq!(case_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_plain_key_list() {
        let (_, items) = keys("(case x ((a b) 1) (otherwise 2))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_catch_all() {
        let (_, items) = keys("(case x (a 1) (otherwise 2))");
        assert!(items.is_empty());
    }

    #[test]
    fn flags_an_ecase_quoted_key() {
        let (_, items) = keys("(ecase x ('a 1))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "ecase");
    }

    #[test]
    fn skips_a_feature_conditional_clause() {
        let (case_form_count, items) = keys("(case x (a 1) #+sbcl ('b 2))");
        assert_eq!(case_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_case_form() {
        let (case_form_count, items) = keys("(list '(case x ('a 1)))");
        assert_eq!(case_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_case_nested_in_a_function_body() {
        let (case_form_count, items) = keys("(defun f (x) (case x ('a 1)))");
        assert_eq!(case_form_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(case x ('a 1))", Dialect::Clojure).expect("parse");
        let (case_form_count, items) =
            collect_quoted_case_keys(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect quoted case keys");
        assert_eq!(case_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (case_form_count, items) = keys("(case x ('a 1))");
        let summary = summarize_quoted_case_keys(case_form_count, items);

        let quiet =
            evaluate_quoted_case_key_policy(QuotedCaseKeyPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_quoted_case_key_policy(QuotedCaseKeyPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
