//! Common Lisp duplicate-slot-name detection: a `defclass`,
//! `define-condition`, or `defstruct` form declaring the same slot name
//! more than once within that single form. CLOS signals an error for a
//! class with more than one direct slot of the same name (CLHS 7.5.3);
//! `define-condition` shares the same slot-list shape and the same rule
//! since condition types are classes under the hood. `defstruct`'s slot
//! list has no such standard-mandated error, but a repeated slot name there
//! silently redefines the earlier slot's accessor and initializer — no
//! less a bug for being unenforced by the reader.
//!
//! Scope: Common Lisp only. Only literal, unqualified slot-name symbols
//! are collected — a slot name computed by a macro this tool cannot expand
//! is invisible here, the same limitation every other syntactic report in
//! this tool already carries.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use paredit_core_cli::CliResult;

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_needle;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path as SexprPath, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_child, atom_text, list_head};

/// A slot specifier is either a bare symbol (`slot-name`) or a list whose
/// head is the slot name (`(slot-name :initform 0)`) — the same shape CLOS
/// slots and defstruct slots both use.
fn slot_name(view: &ExpressionView) -> Option<&str> {
    atom_text(view).or_else(|| list_head(view))
}

/// `defclass`/`define-condition`'s slot list is the single list at
/// position 3, after the name (1) and superclass list (2):
/// `(defclass name (superclass*) (slot*) option*)`.
fn class_like_slot_names(view: &ExpressionView) -> Vec<&str> {
    let Some(slots) = view.children.get(3) else {
        return Vec::new();
    };
    if slots.kind != ExpressionKind::List {
        return Vec::new();
    }
    slots.children.iter().filter_map(slot_name).collect()
}

/// `defstruct`'s slots are each a direct child after the name-and-options
/// position: `(defstruct name-and-options slot*)`.
fn defstruct_slot_names(view: &ExpressionView) -> Vec<&str> {
    view.children.iter().skip(2).filter_map(slot_name).collect()
}

#[derive(Debug, Clone)]
pub struct DuplicateSlotItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub owner: String,
    pub slot: String,
    pub occurrence_count: usize,
}

#[derive(Debug)]
pub struct DuplicateSlotSummary {
    pub definition_count: usize,
    pub duplicates: Vec<DuplicateSlotItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateSlotPolicyOptions {
    fail_on_duplicate: bool,
}

impl DuplicateSlotPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_duplicate: bool) -> Self {
        Self { fail_on_duplicate }
    }

    #[must_use]
    pub const fn fail_on_duplicate(self) -> bool {
        self.fail_on_duplicate
    }
}

#[derive(Debug)]
pub struct DuplicateSlotPolicy {
    pub fail_on_duplicate: bool,
    pub definition_count: usize,
    pub duplicate_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every duplicated slot name from every `defclass`,
/// `define-condition`, and `defstruct` form in one file, along with the
/// total number of such forms scanned.
pub fn collect_duplicate_slots(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> CliResult<(usize, Vec<DuplicateSlotItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut definition_count = 0;
    let mut duplicates = Vec::new();

    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };

        let (owner, slot_names) = if head.eq_ignore_ascii_case("defclass")
            || head.eq_ignore_ascii_case("define-condition")
        {
            let Some(owner) = atom_child(&view, 1) else {
                continue;
            };
            (owner, class_like_slot_names(&view))
        } else if head.eq_ignore_ascii_case("defstruct") {
            let Some(name_and_options) = view.children.get(1) else {
                continue;
            };
            let Some(owner) =
                atom_text(name_and_options).or_else(|| atom_child(name_and_options, 0))
            else {
                continue;
            };
            (owner, defstruct_slot_names(&view))
        } else {
            continue;
        };

        definition_count += 1;

        let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for slot in slot_names {
            let entry = counts
                .entry(common_lisp_symbol_reference_needle(slot))
                .or_insert_with(|| (slot.to_owned(), 0));
            entry.1 += 1;
        }

        for (slot, occurrence_count) in counts.into_values() {
            if occurrence_count < 2 {
                continue;
            }
            duplicates.push(DuplicateSlotItem {
                path: path.to_path_buf(),
                span: view.span,
                owner: owner.to_owned(),
                slot,
                occurrence_count,
            });
        }
    }

    Ok((definition_count, duplicates))
}

#[must_use]
pub const fn summarize_duplicate_slots(
    definition_count: usize,
    duplicates: Vec<DuplicateSlotItem>,
) -> DuplicateSlotSummary {
    DuplicateSlotSummary {
        definition_count,
        duplicates,
    }
}

#[must_use]
pub fn evaluate_duplicate_slot_policy(
    options: DuplicateSlotPolicyOptions,
    summary: &DuplicateSlotSummary,
) -> DuplicateSlotPolicy {
    let duplicate_count = summary.duplicates.len();
    let mut violations = Vec::new();
    if options.fail_on_duplicate() && duplicate_count > 0 {
        violations.push(format!("duplicate_count {duplicate_count} exceeds 0"));
    }

    DuplicateSlotPolicy {
        fail_on_duplicate: options.fail_on_duplicate(),
        definition_count: summary.definition_count,
        duplicate_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duplicates(input: &str) -> (usize, Vec<DuplicateSlotItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_duplicate_slots(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect duplicate slots")
    }

    #[test]
    fn flags_a_defclass_with_a_duplicate_slot_name() {
        let (definition_count, duplicates) = duplicates("(defclass foo () (a a b))");
        assert_eq!(definition_count, 1);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].slot, "a");
        assert_eq!(duplicates[0].occurrence_count, 2);
        assert_eq!(duplicates[0].owner, "foo");
    }

    #[test]
    fn flags_a_defstruct_with_a_duplicate_slot_name() {
        let (_, duplicates) = duplicates("(defstruct foo (a 0) (a 1))");
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].slot, "a");
    }

    #[test]
    fn flags_a_define_condition_with_a_duplicate_slot_name() {
        let (_, duplicates) = duplicates("(define-condition foo (error) (a a))");
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].slot, "a");
    }

    #[test]
    fn does_not_flag_distinct_slot_names() {
        let (definition_count, duplicates) = duplicates("(defclass foo () (a b c))");
        assert_eq!(definition_count, 1);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn does_not_confuse_slots_across_two_different_definitions() {
        let (definition_count, duplicates) = duplicates(
            "(defclass foo () (a))\n\
             (defclass bar () (a))",
        );
        assert_eq!(definition_count, 2);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(defclass foo () (a a))").expect("parse input");
        let (definition_count, duplicates) =
            collect_duplicate_slots(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect duplicate slots");
        assert_eq!(definition_count, 0);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (definition_count, duplicates) = duplicates("(defclass foo () (a a))");
        let summary = summarize_duplicate_slots(definition_count, duplicates);

        let quiet =
            evaluate_duplicate_slot_policy(DuplicateSlotPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.duplicate_count, 1);

        let strict =
            evaluate_duplicate_slot_policy(DuplicateSlotPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
