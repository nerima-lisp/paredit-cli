//! Common Lisp `typecase`-`nil`-key detection: a `typecase`, `etypecase`, or
//! `ctypecase` clause whose head is the bare atom `nil` —
//! `(typecase x (nil 1) …)`. In `typecase`, a clause head is a *type specifier*,
//! and `nil` is the **empty** type (the `nil` type), which no object is ever of,
//! so the clause is dead and can never be selected. Authors almost always mean
//! "match the value `nil`", which requires the `null` type — `(null …)`; the
//! bare `nil` is a silent dead clause.
//!
//! Only the bare, unquoted `nil` atom is flagged:
//!
//!   - `(nil …)`  → flagged: `nil` is the empty type, matches nothing.
//!   - `(null …)` → correct: the `null` type matches the value `nil`.
//!   - `('nil …)` → a quoted datum, not a type specifier, not flagged.
//!
//! The catch-all `(t …)` clause matches every object and is deliberately not
//! flagged. Scoped to `typecase`/`etypecase`/`ctypecase` — the type-dispatch
//! forms. `case`'s clause heads are key designators, a different shape, and are
//! handled by `case-nil-key`. This rule does not rewrite anything (whether the
//! clause is a typo or dead vestige is the author's call), so it is report-only.
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

const TYPECASE_HEADS: [&str; 3] = ["typecase", "etypecase", "ctypecase"];

/// Whether a clause's head is the bare, unquoted atom `nil` (the empty type). A
/// `null` type and a quoted `'nil` are both excluded.
fn is_bare_nil_key(key_designator: &ExpressionView) -> bool {
    key_designator.reader_prefixes.is_empty()
        && atom_text(key_designator).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct TypecaseNilKeyItem {
    pub path: PathBuf,
    /// The span of the offending `nil` type specifier.
    pub span: ByteSpan,
    /// The typecase operator (`typecase`/`etypecase`/`ctypecase`), for the finding message.
    pub head: String,
}

#[derive(Debug)]
pub struct TypecaseNilKeySummary {
    pub typecase_form_count: usize,
    pub violations: Vec<TypecaseNilKeyItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct TypecaseNilKeyPolicyOptions {
    fail_on_violation: bool,
}

impl TypecaseNilKeyPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct TypecaseNilKeyPolicy {
    pub fail_on_violation: bool,
    pub typecase_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_case(
    view: &ExpressionView,
    path: &Path,
    typecase_form_count: &mut usize,
    violations: &mut Vec<TypecaseNilKeyItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !TYPECASE_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted typecase form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    *typecase_form_count += 1;

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
            violations.push(TypecaseNilKeyItem {
                path: path.to_path_buf(),
                span: key_designator.span,
                head: head.to_owned(),
            });
        }
    }
}

/// Collects every `typecase`/`etypecase`/`ctypecase` clause whose head is a bare
/// `nil` type specifier across a whole file, along with the total number of such
/// forms scanned.
pub fn collect_typecase_nil_keys(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<TypecaseNilKeyItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut typecase_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, path, &mut typecase_form_count, &mut violations)
        });
    }
    Ok((typecase_form_count, violations))
}

pub fn summarize_typecase_nil_keys(
    typecase_form_count: usize,
    violations: Vec<TypecaseNilKeyItem>,
) -> TypecaseNilKeySummary {
    TypecaseNilKeySummary {
        typecase_form_count,
        violations,
    }
}

pub fn evaluate_typecase_nil_key_policy(
    options: TypecaseNilKeyPolicyOptions,
    summary: &TypecaseNilKeySummary,
) -> TypecaseNilKeyPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    TypecaseNilKeyPolicy {
        fail_on_violation: options.fail_on_violation(),
        typecase_form_count: summary.typecase_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(input: &str) -> (usize, Vec<TypecaseNilKeyItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_typecase_nil_keys(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect typecase nil keys")
    }

    #[test]
    fn flags_a_bare_nil_type() {
        let (typecase_form_count, items) = keys("(typecase x (nil 1) (t 2))");
        assert_eq!(typecase_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "typecase");
    }

    #[test]
    fn does_not_flag_a_null_type() {
        // (null …) is the correct way to match the value nil.
        let (_, items) = keys("(typecase x (null 1) (t 2))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_nil() {
        // 'nil is a quoted datum, not a type specifier.
        let (_, items) = keys("(typecase x ('nil 1))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_ordinary_types() {
        let (typecase_form_count, items) = keys("(typecase x (integer 1) (string 2) (t 3))");
        assert_eq!(typecase_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn flags_an_etypecase_nil_type() {
        let (_, items) = keys("(etypecase x (nil 1))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "etypecase");
    }

    #[test]
    fn case_folds_the_nil_type() {
        let (_, items) = keys("(typecase x (NIL 1))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_nil_type_in_any_clause_position() {
        // nil is never a catch-all, so a trailing nil clause is still dead.
        let (_, items) = keys("(typecase x (integer 1) (nil 2))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn finds_a_typecase_nested_in_a_function_body() {
        let (typecase_form_count, items) = keys("(defun f (x) (typecase x (nil 1)))");
        assert_eq!(typecase_form_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(typecase x (nil 1))", Dialect::Clojure)
            .expect("parse");
        let (typecase_form_count, items) =
            collect_typecase_nil_keys(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect typecase nil keys");
        assert_eq!(typecase_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (typecase_form_count, items) = keys("(typecase x (nil 1))");
        let summary = summarize_typecase_nil_keys(typecase_form_count, items);

        let quiet =
            evaluate_typecase_nil_key_policy(TypecaseNilKeyPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_typecase_nil_key_policy(TypecaseNilKeyPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
