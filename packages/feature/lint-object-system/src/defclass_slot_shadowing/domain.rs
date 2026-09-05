//! `defclass-slot-shadowing` detection: a subclass slot that redeclares a slot
//! one of its same-file superclasses already declares, with no `:allocation`
//! difference.
//!
//! Redeclaring an inherited slot is legal and often deliberate — it is how a
//! subclass narrows a `:type`, supplies a different `:initform`, or adds an
//! accessor. It is reported because the merge is **per slot option, with a
//! different rule for each**, and no single one of those rules is what a reader
//! skimming two `defclass` forms assumes. Per CLHS 7.5.3:
//!
//! - the `:initarg` sets are **unioned** across the whole chain, so a subclass
//!   cannot take an initarg away;
//! - `:reader`/`:writer`/`:accessor` define *methods*, so they accumulate too —
//!   the superclass's accessor keeps working on the subclass;
//! - `:type` is **conjoined**: the slot holds `(and T1 … Tn)`, so a
//!   redeclaration whose type is not a subtype of the inherited one narrows the
//!   slot to their intersection rather than replacing it;
//! - `:initform` and `:documentation` come from *the most specific declaration
//!   that supplies one*, so leaving them out here inherits them rather than
//!   clearing them.
//!
//! Nothing is silently replaced, and the finding does not say it is. What the
//! finding says is that two declarations of one slot are in play and their
//! options merge by four different rules, which is worth a second look.
//!
//! Two deliberate limits, both in the direction of not reporting:
//!
//! - **Same file only.** A superclass this file does not define is not
//!   resolved, and its slots are not compared. Resolving across files would
//!   need the whole system's source, which a lint pass over one file does not
//!   have.
//! - **Same allocation only.** A slot redeclared with a different
//!   `:allocation` — `:class` over an inherited `:instance`, say — is a
//!   deliberate change of storage, not an accident, so it is left alone.
//!
//! # Overlap with `inspect class-hierarchy`
//!
//! `paredit-feature-lisp-analysis`'s `class_hierarchy_report` covers the same
//! ground: it carries a `shadowed_slots` list per class and surfaces any class
//! with a non-empty one as the finding kind `shadowing-class`. It is not
//! registered as a lint rule, and the two **disagree in both directions** — a
//! user running both today gets different answers, which is expected rather
//! than a bug in either.
//!
//! - *What each answers.* This rule answers "is this redeclaration likely to be
//!   an accident", as a lint finding with a severity and a suppression comment.
//!   The report answers "where does the slot at this name come from", as part
//!   of a survey of the file's class graph.
//! - *Where this rule is quieter.* The report matches on the slot **name**
//!   alone and never reads `:allocation` at all, so it flags any
//!   redeclaration; this rule flags one only when the two declarations agree on
//!   `:allocation`, on the grounds that changing storage is deliberate.
//! - *Where the report is broader.* It reads `define-condition` and `defstruct`
//!   hierarchies as well; this rule reads `defclass` only.
//!
//! Neither is a superset. Consolidating them is a separate change.
//!
//! Scope: Common Lisp only.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, symbol_is, unqualified};
use serde_json::{Value, json};

use crate::support::{self, ClassForm};

#[derive(Debug, Clone)]
pub struct DefclassSlotShadowingItem {
    /// The span of the shadowing slot spec, not of the whole `defclass`.
    pub span: ByteSpan,
    pub slot: String,
    pub subclass: String,
    /// The nearest same-file superclass that already declares `slot`.
    pub superclass: String,
    /// The allocation both declarations share, which is why this is shadowing
    /// rather than a deliberate change of storage.
    pub allocation: String,
}

impl Finding for DefclassSlotShadowingItem {
    fn kind(&self) -> &'static str {
        "defclass-slot-shadowing"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("slot={}", self.slot),
            format!("subclass={}", self.subclass),
            format!("superclass={}", self.superclass),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("slot", json!(self.slot)),
            ("subclass", json!(self.subclass)),
            ("superclass", json!(self.superclass)),
            ("allocation", json!(self.allocation)),
        ]
    }

    /// Says what CLHS 7.5.3 actually does, which is *not* "replaces".
    ///
    /// The earlier wording claimed the subclass definition replaced the
    /// inherited one wholesale. It does not: the `:initarg` sets and the
    /// accessor methods accumulate across the chain, `:type` is conjoined, and
    /// `:initform` is only overridden when this declaration supplies one.
    fn message(&self) -> String {
        format!(
            "slot {} in {} redeclares the slot {} declares, at the same allocation {}: per CLHS \
             7.5.3 this does not replace the inherited declaration — the :initarg and accessor \
             sets are unioned, :type is conjoined, and :initform comes from the most specific \
             declaration that supplies one",
            self.slot, self.subclass, self.superclass, self.allocation
        )
    }
}

/// The comparison key for a class or slot name: the reader upcases unescaped
/// symbols and a package prefix does not change which name is meant.
fn key(name: &str) -> String {
    unqualified(name).to_ascii_lowercase()
}

/// Every top-level `defclass` the reader evaluates, materialized once so the
/// borrowed [`ClassForm`]s below have something to borrow from.
fn same_file_class_views(tree: &SyntaxTree) -> Vec<ExpressionView> {
    support::top_level_forms(tree)
        .filter(|form| symbol_is(form.head, "defclass"))
        .filter_map(|form| support::top_level_view(tree, form.index))
        .collect()
}

/// Each ancestor slot reachable from `class`, as `slot -> (allocation,
/// declaring class)`.
///
/// Breadth-first from the direct superclasses in written order, so the *nearest*
/// declarer is the one reported and the answer does not depend on hash order.
/// The visited set is what makes a mutually-referential pair of classes — which
/// is invalid CLOS but perfectly parseable text — terminate.
fn inherited_slots(
    classes: &HashMap<String, ClassForm<'_>>,
    class: &ClassForm<'_>,
) -> HashMap<String, (String, String)> {
    let mut inherited: HashMap<String, (String, String)> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(key(class.name));
    let mut queue: VecDeque<String> = class.superclasses.iter().map(|name| key(name)).collect();

    while let Some(name) = queue.pop_front() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(ancestor) = classes.get(&name) else {
            continue;
        };
        for slot in &ancestor.slots {
            inherited
                .entry(key(slot.name))
                .or_insert_with(|| (slot.allocation(), ancestor.name.to_owned()));
        }
        queue.extend(ancestor.superclasses.iter().map(|name| key(name)));
    }
    inherited
}

pub fn examine_defclass_slot_shadowing(
    tree: &SyntaxTree,
    view: &ExpressionView,
    defclass_form_count: &mut usize,
    violations: &mut Vec<DefclassSlotShadowingItem>,
) {
    let Some(class) = support::class_form(view) else {
        return;
    };
    // A `(defclass …)` inside `'(…)` is a list, and one nested in another form
    // is not what this file's other top-level classes inherit from.
    let Some(site) = support::locate(tree, view.span) else {
        return;
    };
    if site.quoted || !site.top_level {
        return;
    }
    *defclass_form_count += 1;
    if class.slots.is_empty() || class.superclasses.is_empty() {
        return;
    }

    let class_views = same_file_class_views(tree);
    let mut classes: HashMap<String, ClassForm<'_>> = HashMap::new();
    for class_view in &class_views {
        if let Some(other) = support::class_form(class_view) {
            classes.entry(key(other.name)).or_insert(other);
        }
    }

    let inherited = inherited_slots(&classes, &class);
    for slot in &class.slots {
        let Some((allocation, declarer)) = inherited.get(&key(slot.name)) else {
            continue;
        };
        if *allocation != slot.allocation() {
            continue;
        }
        violations.push(DefclassSlotShadowingItem {
            span: slot.span,
            slot: slot.name.to_owned(),
            subclass: class.name.to_owned(),
            superclass: declarer.clone(),
            allocation: allocation.clone(),
        });
    }
}

/// Collects every shadowed slot in one file, with the number of `defclass`
/// forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_defclass_slot_shadowing_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DefclassSlotShadowingItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("defclass_form_count", json!(0))],
        ));
    }

    let mut defclass_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_defclass_slot_shadowing(
                tree,
                subview,
                &mut defclass_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("defclass_form_count", json!(defclass_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DefclassSlotShadowingItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_defclass_slot_shadowing_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build defclass slot shadowing report")
    }

    fn violations(input: &str) -> Vec<DefclassSlotShadowingItem> {
        report(input).findings
    }

    const PARENT: &str = "(defclass shape () ((origin :initform 0 :initarg :origin)))\n";

    #[test]
    fn flags_a_slot_the_superclass_already_declares() {
        let found = violations(&format!(
            "{PARENT}(defclass circle (shape) ((origin :accessor origin-of)))"
        ));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].slot, "origin");
        assert_eq!(found[0].subclass, "circle");
        assert_eq!(found[0].superclass, "shape");
        assert_eq!(found[0].allocation, ":instance");
    }

    #[test]
    fn flags_a_slot_inherited_through_an_intermediate_class() {
        let found = violations(&format!(
            "{PARENT}(defclass round-shape (shape) ())\n(defclass circle (round-shape) (origin))"
        ));
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].superclass, "shape",
            "the nearest declarer is named"
        );
    }

    #[test]
    fn folds_case_and_package_qualification_when_matching_names() {
        let found =
            violations("(defclass Shape () (Origin))\n(defclass circle (cl-user:shape) (ORIGIN))");
        assert_eq!(found.len(), 1);
    }

    /// The near miss: a subclass declaring a slot of its own.
    #[test]
    fn does_not_flag_a_slot_the_superclass_does_not_declare() {
        let found = violations(&format!(
            "{PARENT}(defclass circle (shape) ((radius :initform 1)))"
        ));
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_redeclaration_that_changes_the_allocation() {
        let found = violations(&format!(
            "{PARENT}(defclass circle (shape) ((origin :allocation :class)))"
        ));
        assert!(
            found.is_empty(),
            "moving a slot to :class allocation is a deliberate change of storage"
        );
    }

    #[test]
    fn does_not_flag_a_superclass_this_file_does_not_define() {
        let found = violations("(defclass circle (shape) ((origin :accessor origin-of)))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_class_with_no_superclass() {
        let found = violations("(defclass shape () (origin))\n(defclass circle () (origin))");
        assert!(found.is_empty());
    }

    #[test]
    fn terminates_on_a_cycle_between_two_classes() {
        // Invalid CLOS, perfectly valid text. The visited set is what stops it.
        let found = violations("(defclass a (b) (x))\n(defclass b (a) (x))");
        assert_eq!(
            found.len(),
            2,
            "each class shadows the other's slot exactly once"
        );
    }

    #[test]
    fn does_not_flag_a_quoted_defclass() {
        let found = violations(&format!(
            "{PARENT}(setf template '(defclass circle (shape) ((origin :accessor origin-of))))"
        ));
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defclass_inside_an_unescaped_quasiquote() {
        let found = violations(&format!(
            "{PARENT}(defmacro make-it () `(defclass circle (shape) ((origin :accessor o))))"
        ));
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defclass_written_inside_a_string() {
        let found = violations(&format!(
            "{PARENT}(defparameter *doc* \"(defclass circle (shape) ((origin :accessor o)))\")"
        ));
        assert!(found.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_defclass_scanned_not_only_the_flagged_ones() {
        let scanned = report(&format!("{PARENT}(defclass circle (shape) (origin))"));
        assert_eq!(scanned.summary, vec![("defclass_form_count", json!(2))]);
        assert_eq!(scanned.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defclass a () ())", Dialect::Clojure).expect("parse");
        let built =
            build_defclass_slot_shadowing_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn a_finding_carries_its_line_and_its_columns() {
        let built = report(&format!(
            "{PARENT}(defclass circle (shape)\n  ((origin :accessor o)))"
        ));
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 3);
        assert_eq!(finding.kind(), "defclass-slot-shadowing");
        assert_eq!(
            finding.text_columns(),
            vec![
                "slot=origin".to_owned(),
                "subclass=circle".to_owned(),
                "superclass=shape".to_owned(),
            ]
        );
    }

    /// The message must not contradict CLHS 7.5.3. A redeclaration does *not*
    /// replace the inherited slot definition: `:initarg` sets and accessor
    /// methods are unioned across the chain, `:type` is conjoined, and
    /// `:initform` is taken from the most specific declaration that supplies
    /// one — so omitting it here inherits it rather than clearing it.
    #[test]
    fn the_message_states_the_inheritance_rules_the_spec_actually_gives() {
        let built = report(&format!(
            "{PARENT}(defclass circle (shape)\n  ((origin :accessor o)))"
        ));
        let message = built.findings[0].message();
        assert!(
            !message.contains("replaces the inherited one rather than adding to it"),
            "CLHS 7.5.3 unions the initarg and accessor sets: {message}"
        );
        assert!(
            message.contains("does not replace the inherited declaration"),
            "{message}"
        );
        for fact in [
            ":initarg and accessor sets are unioned",
            ":type is conjoined",
            ":initform comes from the most specific declaration that supplies one",
        ] {
            assert!(message.contains(fact), "missing {fact:?} in: {message}");
        }
    }
}
