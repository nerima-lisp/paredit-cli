//! Common Lisp duplicate-setf-place detection: a `setq`/`psetq`/`setf`/`psetf`
//! that assigns the same *variable* more than once in a single form —
//! `(setf a 1 a 2)`, `(setq x 1 y 2 x 3)`. For `setq`/`setf` the earlier
//! assignment's value is computed and then immediately overwritten (dead work,
//! and its net effect lost); for the parallel `psetq`/`psetf` assigning the same
//! place twice has undefined consequences. Either way a repeated place is almost
//! always a copy-paste slip or a typo'd variable name.
//!
//! Only *symbol* places are compared, by exact name. A compound `setf` place
//! (`(aref a i)`) is not compared — detecting an accidental duplicate there
//! needs full structural equality and is far rarer — and a trailing unpaired
//! argument (a malformed, odd-arity form) is left to `setf-arity`.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

const ASSIGNMENT_HEADS: [&str; 4] = ["setq", "psetq", "setf", "psetf"];

/// The bare variable name of a symbol place (no reader prefixes), or `None` for
/// a compound place or prefixed atom.
fn symbol_place(view: &ExpressionView) -> Option<&str> {
    view.reader_prefixes
        .is_empty()
        .then(|| atom_text(view))
        .flatten()
}

#[derive(Debug, Clone)]
pub struct DuplicateSetfPlaceItem {
    pub path: PathBuf,
    /// The span of the whole assignment form.
    pub span: ByteSpan,
    /// The operator, lowercased (`setf`/`setq`/`psetf`/`psetq`).
    pub operator: String,
    /// The variable name assigned more than once.
    pub place: String,
}

#[derive(Debug)]
pub struct DuplicateSetfPlaceSummary {
    pub assignment_form_count: usize,
    pub violations: Vec<DuplicateSetfPlaceItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateSetfPlacePolicyOptions {
    fail_on_violation: bool,
}

impl DuplicateSetfPlacePolicyOptions {
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
pub struct DuplicateSetfPlacePolicy {
    pub fail_on_violation: bool,
    pub assignment_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_assignment(
    view: &ExpressionView,
    path: &Path,
    assignment_form_count: &mut usize,
    violations: &mut Vec<DuplicateSetfPlaceItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !ASSIGNMENT_HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    *assignment_form_count += 1;

    // Arguments are place/value pairs after the operator; report each variable
    // that is assigned a second time (once per form per duplicated name).
    let mut seen: HashSet<String> = HashSet::new();
    let mut reported: HashSet<String> = HashSet::new();
    let mut pair = view.children.iter().skip(1);
    while let (Some(place), Some(_value)) = (pair.next(), pair.next()) {
        let Some(name) = symbol_place(place) else {
            continue;
        };
        let key = name.to_ascii_lowercase();
        if !seen.insert(key.clone()) && reported.insert(key) {
            violations.push(DuplicateSetfPlaceItem {
                path: path.to_path_buf(),
                span: view.span,
                operator: head.to_ascii_lowercase(),
                place: name.to_owned(),
            });
        }
    }
}

/// Collects every duplicated symbol place across a whole file, along with the
/// total number of assignment forms scanned.
pub fn collect_duplicate_setf_places(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DuplicateSetfPlaceItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut assignment_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_assignment(subview, path, &mut assignment_form_count, &mut violations);
        });
    }
    Ok((assignment_form_count, violations))
}

#[must_use]
pub const fn summarize_duplicate_setf_places(
    assignment_form_count: usize,
    violations: Vec<DuplicateSetfPlaceItem>,
) -> DuplicateSetfPlaceSummary {
    DuplicateSetfPlaceSummary {
        assignment_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_duplicate_setf_place_policy(
    options: DuplicateSetfPlacePolicyOptions,
    summary: &DuplicateSetfPlaceSummary,
) -> DuplicateSetfPlacePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    DuplicateSetfPlacePolicy {
        fail_on_violation: options.fail_on_violation(),
        assignment_form_count: summary.assignment_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places(input: &str) -> (usize, Vec<DuplicateSetfPlaceItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_duplicate_setf_places(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect duplicate setf places")
    }

    #[test]
    fn flags_repeated_setf_place() {
        let (count, violations) = places("(setf a 1 a 2)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].place, "a");
        assert_eq!(violations[0].operator, "setf");
    }

    #[test]
    fn flags_repeat_among_other_places() {
        let (_, violations) = places("(setq x 1 y 2 x 3)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].place, "x");
    }

    #[test]
    fn reports_each_duplicated_name_once() {
        // a and b both repeat; each reported a single time.
        let (_, violations) = places("(setf a 1 b 2 a 3 b 4)");
        assert_eq!(violations.len(), 2);
        let names: Vec<&str> = violations.iter().map(|v| v.place.as_str()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
    }

    #[test]
    fn does_not_flag_distinct_places() {
        let (count, violations) = places("(setf a 1 b 2 c 3)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_single_pair() {
        let (_, violations) = places("(setf a 1)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_compound_place() {
        // Compound places are not compared.
        let (_, violations) = places("(setf (aref v i) 1 (aref v i) 2)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_place_names() {
        // CL symbols are case-insensitive; A and a are the same variable.
        let (_, violations) = places("(setf A 1 a 2)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_psetq() {
        let (_, violations) = places("(psetq n 1 n 2)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "psetq");
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = places("(let ((a 1) (a 2)) a)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_duplicate() {
        let (_, violations) = places("(defun f () (setf total 0 total 1))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(setf a 1 a 2)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_duplicate_setf_places(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect duplicate setf places");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = places("(setf a 1 a 2)");
        let summary = summarize_duplicate_setf_places(count, items);

        let quiet = evaluate_duplicate_setf_place_policy(
            DuplicateSetfPlacePolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_duplicate_setf_place_policy(
            DuplicateSetfPlacePolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
