//! Common Lisp duplicate parallel-`let`-binding detection: a `let` form whose
//! binding list names the same variable more than once. The standard forbids
//! this — "the consequences are undefined if more than one binding of the
//! same name appears" in a parallel `let` — because all a `let`'s
//! initializers run in the outer scope simultaneously, so a repeated name is
//! not shadowing but an outright error.
//!
//! Scoped to `let` on purpose: `let*` binds sequentially, where re-binding a
//! name is legal, intentional shadowing (each later init sees the earlier
//! binding), so it is excluded here. This report walks the whole tree via the
//! shared [`crate::domain::view_query::for_each_subview`], since a `let` can
//! appear anywhere in a body.
//!
//! Scope: Common Lisp only. A binding is read as either a bare symbol
//! (`x`, initialized to nil) or a `(name init)` list; names fold ASCII case
//! the way the reader does.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// The bound variable name of one `let` binding: a bare symbol, or the head
/// of a `(name init)` list.
fn binding_name(binding: &ExpressionView) -> Option<&str> {
    atom_text(binding).or_else(|| {
        is_paren_list(binding)
            .then(|| binding.children.first().and_then(atom_text))
            .flatten()
    })
}

#[derive(Debug, Clone)]
pub struct DuplicateLetBindingItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub name: String,
    pub occurrence_count: usize,
}

#[derive(Debug)]
pub struct DuplicateLetBindingSummary {
    pub let_form_count: usize,
    pub duplicates: Vec<DuplicateLetBindingItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateLetBindingPolicyOptions {
    fail_on_duplicate: bool,
}

impl DuplicateLetBindingPolicyOptions {
    #[must_use]
    pub fn new(fail_on_duplicate: bool) -> Self {
        Self { fail_on_duplicate }
    }

    #[must_use]
    pub const fn fail_on_duplicate(self) -> bool {
        self.fail_on_duplicate
    }
}

#[derive(Debug)]
pub struct DuplicateLetBindingPolicy {
    pub fail_on_duplicate: bool,
    pub let_form_count: usize,
    pub duplicate_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_let(
    view: &ExpressionView,
    path: &Path,
    let_form_count: &mut usize,
    duplicates: &mut Vec<DuplicateLetBindingItem>,
) {
    // Exactly `let` — `let*` binds sequentially, where re-binding is legal.
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("let")) {
        return;
    }
    let Some(binding_list) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(binding_list) {
        return;
    }
    *let_form_count += 1;

    // Preserve first-seen spelling and binding order while counting.
    let mut order: Vec<String> = Vec::new();
    let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for name in binding_list.children.iter().filter_map(binding_name) {
        let needle = name.to_ascii_uppercase();
        let entry = counts.entry(needle.clone()).or_insert_with(|| {
            order.push(needle);
            (name.to_owned(), 0)
        });
        entry.1 += 1;
    }

    for needle in order {
        let (name, occurrence_count) = &counts[&needle];
        if *occurrence_count < 2 {
            continue;
        }
        duplicates.push(DuplicateLetBindingItem {
            path: path.to_path_buf(),
            span: view.span,
            name: name.clone(),
            occurrence_count: *occurrence_count,
        });
    }
}

/// Collects every duplicated parallel-`let` binding across a whole file,
/// along with the total number of `let` forms scanned.
pub fn collect_duplicate_let_bindings(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DuplicateLetBindingItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut let_form_count = 0;
    let mut duplicates = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_let(subview, path, &mut let_form_count, &mut duplicates);
        });
    }
    Ok((let_form_count, duplicates))
}

#[must_use]
pub fn summarize_duplicate_let_bindings(
    let_form_count: usize,
    duplicates: Vec<DuplicateLetBindingItem>,
) -> DuplicateLetBindingSummary {
    DuplicateLetBindingSummary {
        let_form_count,
        duplicates,
    }
}

#[must_use]
pub fn evaluate_duplicate_let_binding_policy(
    options: DuplicateLetBindingPolicyOptions,
    summary: &DuplicateLetBindingSummary,
) -> DuplicateLetBindingPolicy {
    let duplicate_count = summary.duplicates.len();
    let mut violations = Vec::new();
    if options.fail_on_duplicate() && duplicate_count > 0 {
        violations.push(format!("duplicate_count {duplicate_count} exceeds 0"));
    }

    DuplicateLetBindingPolicy {
        fail_on_duplicate: options.fail_on_duplicate(),
        let_form_count: summary.let_form_count,
        duplicate_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duplicates(input: &str) -> (usize, Vec<DuplicateLetBindingItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_duplicate_let_bindings(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect duplicate let bindings")
    }

    #[test]
    fn flags_a_variable_bound_twice_in_a_parallel_let() {
        let (let_form_count, duplicates) = duplicates("(let ((x 1) (y 2) (x 3)) x)");
        assert_eq!(let_form_count, 1);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].name, "x");
        assert_eq!(duplicates[0].occurrence_count, 2);
    }

    #[test]
    fn flags_a_bare_symbol_binding_duplicated() {
        let (_, duplicates) = duplicates("(let (x x) x)");
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].name, "x");
    }

    #[test]
    fn folds_symbol_case() {
        let (_, duplicates) = duplicates("(let ((x 1) (X 2)) x)");
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_bindings() {
        let (let_form_count, duplicates) = duplicates("(let ((x 1) (y 2)) x)");
        assert_eq!(let_form_count, 1);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn does_not_flag_a_let_star_which_allows_shadowing() {
        let (let_form_count, duplicates) = duplicates("(let* ((x 1) (x (1+ x))) x)");
        assert_eq!(let_form_count, 0);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn finds_a_let_nested_in_a_function_body() {
        let (let_form_count, duplicates) = duplicates("(defun f () (let ((a 1) (a 2)) a))");
        assert_eq!(let_form_count, 1);
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(let ((x 1) (x 2)) x)").expect("parse input");
        let (let_form_count, duplicates) =
            collect_duplicate_let_bindings(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect duplicate let bindings");
        assert_eq!(let_form_count, 0);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (let_form_count, items) = duplicates("(let ((x 1) (x 2)) x)");
        let summary = summarize_duplicate_let_bindings(let_form_count, items);

        let quiet = evaluate_duplicate_let_binding_policy(
            DuplicateLetBindingPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.duplicate_count, 1);

        let strict = evaluate_duplicate_let_binding_policy(
            DuplicateLetBindingPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
