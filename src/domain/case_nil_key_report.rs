//! Common Lisp `case`-`nil`-key detection: a `case`, `ccase`, or `ecase` clause
//! whose key designator is the bare atom `nil` — `(case x (nil 1) …)`. In
//! `case`, a key designator is a *designator for a list of objects*, and `nil`
//! designates the **empty** list, so the clause has no keys and can never be
//! selected. Authors almost always mean "match the value `nil`", which requires
//! the one-element key list `((nil) …)`; the bare `nil` is a silent dead clause.
//!
//! Only the bare, unquoted `nil` atom is flagged:
//!
//!   - `(nil …)`   → flagged: `nil` is the empty key list, never matches.
//!   - `((nil) …)` → correct: a key list containing the symbol `nil`.
//!   - `('nil …)`  → the `quoted-case-key` rule's concern, not this one.
//!
//! Scoped to `case`/`ccase`/`ecase` — the `eql`-key forms. `typecase`'s clause
//! heads are type specifiers, a different shape, and are not inspected here.
//! This rule does not rewrite anything (whether the clause is a typo or dead
//! vestige is the author's call), so it is report-only.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

const CASE_HEADS: [&str; 3] = ["case", "ccase", "ecase"];

/// Whether a clause's key designator is the bare, unquoted atom `nil` (the empty
/// key list). A `(nil)` key *list* and a quoted `'nil` are both excluded.
fn is_bare_nil_key(key_designator: &ExpressionView) -> bool {
    key_designator.reader_prefixes.is_empty()
        && atom_text(key_designator).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct CaseNilKeyItem {
    pub path: PathBuf,
    /// The span of the offending `nil` key designator.
    pub span: ByteSpan,
    /// The case operator (`case`/`ccase`/`ecase`), for the finding message.
    pub head: String,
}

#[derive(Debug)]
pub struct CaseNilKeySummary {
    pub case_form_count: usize,
    pub violations: Vec<CaseNilKeyItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct CaseNilKeyPolicyOptions {
    fail_on_violation: bool,
}

impl CaseNilKeyPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct CaseNilKeyPolicy {
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
    violations: &mut Vec<CaseNilKeyItem>,
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
        if is_bare_nil_key(key_designator) {
            violations.push(CaseNilKeyItem {
                path: path.to_path_buf(),
                span: key_designator.span,
                head: head.to_owned(),
            });
        }
    }
}

/// Collects every `case`/`ccase`/`ecase` clause with a bare `nil` key designator
/// across a whole file, along with the total number of such forms scanned.
pub fn collect_case_nil_keys(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<CaseNilKeyItem>)> {
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

pub fn summarize_case_nil_keys(
    case_form_count: usize,
    violations: Vec<CaseNilKeyItem>,
) -> CaseNilKeySummary {
    CaseNilKeySummary {
        case_form_count,
        violations,
    }
}

pub fn evaluate_case_nil_key_policy(
    options: CaseNilKeyPolicyOptions,
    summary: &CaseNilKeySummary,
) -> CaseNilKeyPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    CaseNilKeyPolicy {
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

    fn keys(input: &str) -> (usize, Vec<CaseNilKeyItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_case_nil_keys(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect case nil keys")
    }

    #[test]
    fn flags_a_bare_nil_key() {
        let (case_form_count, items) = keys("(case x (nil 1) (t 2))");
        assert_eq!(case_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "case");
    }

    #[test]
    fn does_not_flag_a_nil_key_list() {
        // ((nil) …) is the correct way to match the value nil.
        let (_, items) = keys("(case x ((nil) 1) (t 2))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_nil() {
        // 'nil is quoted-case-key's concern.
        let (_, items) = keys("(case x ('nil 1))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_ordinary_keys() {
        let (case_form_count, items) = keys("(case x (a 1) (b 2) (t 3))");
        assert_eq!(case_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn flags_an_ecase_nil_key() {
        let (_, items) = keys("(ecase x (nil 1))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "ecase");
    }

    #[test]
    fn case_folds_the_nil_key() {
        let (_, items) = keys("(case x (NIL 1))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_nil_key_in_any_clause_position() {
        // nil is never a catch-all, so a trailing nil clause is still dead.
        let (_, items) = keys("(case x (a 1) (nil 2))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn finds_a_case_nested_in_a_function_body() {
        let (case_form_count, items) = keys("(defun f (x) (case x (nil 1)))");
        assert_eq!(case_form_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(case x (nil 1))", Dialect::Clojure).expect("parse");
        let (case_form_count, items) =
            collect_case_nil_keys(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect case nil keys");
        assert_eq!(case_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (case_form_count, items) = keys("(case x (nil 1))");
        let summary = summarize_case_nil_keys(case_form_count, items);

        let quiet = evaluate_case_nil_key_policy(CaseNilKeyPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_case_nil_key_policy(CaseNilKeyPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
