//! Common Lisp CLOS class-inheritance cycle detection: two or more
//! `defclass` or `define-condition` forms whose direct-superclass (or
//! parent-condition-type) lists form a loop — a genuine class-finalization
//! failure in every conforming CLOS implementation, since the class
//! precedence list cannot be linearized when two classes each
//! (transitively) inherit from each other.
//!
//! `define-condition` shares this check because condition types *are*
//! classes under the hood in Common Lisp — `(define-condition name
//! (parent-type*) (slot*) option*)` has exactly the same second-position
//! parent-list shape as `(defclass name (superclass*) (slot*) option*)`,
//! backed by the same CLOS class-precedence-list computation, so a cycle
//! there fails to finalize for the identical reason. A class and a
//! condition sharing a name would already collide in the shared CLOS class
//! namespace, so folding both declaration forms into one graph does not
//! risk conflating unrelated identifiers.
//!
//! Same graph algorithm as [`crate::domain::call_cycle_report`],
//! [`crate::domain::package_cycle_report`], and
//! [`crate::domain::system_cycle_report`]: [`crate::domain::graph::tarjan_scc`]
//! over an edge list built for these two declaration forms.
//!
//! Scope: Common Lisp only. Only literal, unqualified superclass/parent
//! symbols in the second-position list are collected — a superclass
//! defined outside the analyzed fileset (`standard-object`, `condition`, or
//! a class from an unanalyzed library) never contributes an edge back into
//! the graph and so cannot itself form or hide a cycle.

use anyhow::Result;

use crate::domain::common_lisp::common_lisp_symbol_reference_needle;
use crate::domain::dialect::Dialect;
use crate::domain::graph::string_edge_cycles;
use crate::domain::sexpr::{ExpressionKind, Path, SyntaxTree};
use crate::domain::view_query::{atom_child, atom_text, list_head};

#[derive(Debug, Clone)]
pub struct ClassCycleItem {
    /// Members of this strongly connected component, in first-seen order.
    pub members: Vec<String>,
}

#[derive(Debug)]
pub struct ClassCycleSummary {
    pub class_count: usize,
    pub cycles: Vec<ClassCycleItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ClassCyclePolicyOptions {
    fail_on_cycle: bool,
}

impl ClassCyclePolicyOptions {
    #[must_use]
    pub const fn new(fail_on_cycle: bool) -> Self {
        Self { fail_on_cycle }
    }

    #[must_use]
    pub const fn fail_on_cycle(self) -> bool {
        self.fail_on_cycle
    }
}

#[derive(Debug)]
pub struct ClassCyclePolicy {
    pub fail_on_cycle: bool,
    pub class_count: usize,
    pub cycle_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects `(class, direct_superclass)` edges from every top-level
/// `defclass` or `define-condition` form in one file. Only Common Lisp
/// declares classes this way, so non-Common-Lisp files contribute no edges
/// (a documented no-op, mirroring how
/// [`crate::domain::package_cycle_report`] scopes itself).
pub fn collect_class_inheritance_edges(
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<Vec<(String, String)>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let mut edges = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&Path::root_child(index))?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        if !head.eq_ignore_ascii_case("defclass") && !head.eq_ignore_ascii_case("define-condition")
        {
            continue;
        }
        let Some(class_name) = atom_child(&view, 1) else {
            continue;
        };
        let Some(superclasses) = view.children.get(2) else {
            continue;
        };
        if superclasses.kind != ExpressionKind::List {
            continue;
        }
        for superclass in &superclasses.children {
            let Some(superclass_name) = atom_text(superclass) else {
                continue;
            };
            edges.push((class_name.to_owned(), superclass_name.to_owned()));
        }
    }
    Ok(edges)
}

pub fn analyze_class_cycles(edges: &[(String, String)]) -> ClassCycleSummary {
    let (class_count, cycles) = string_edge_cycles(edges, common_lisp_symbol_reference_needle);
    ClassCycleSummary {
        class_count,
        cycles: cycles
            .into_iter()
            .map(|members| ClassCycleItem { members })
            .collect(),
    }
}

#[must_use]
pub fn evaluate_class_cycle_policy(
    options: ClassCyclePolicyOptions,
    summary: &ClassCycleSummary,
) -> ClassCyclePolicy {
    let cycle_count = summary.cycles.len();
    let mut violations = Vec::new();
    if options.fail_on_cycle() && cycle_count > 0 {
        violations.push(format!("cycle_count {cycle_count} exceeds 0"));
    }

    ClassCyclePolicy {
        fail_on_cycle: options.fail_on_cycle(),
        class_count: summary.class_count,
        cycle_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(input: &str) -> Vec<(String, String)> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_class_inheritance_edges(Dialect::CommonLisp, &tree)
            .expect("collect class inheritance edges")
    }

    #[test]
    fn collects_superclass_edges_from_defclass() {
        let found = edges("(defclass app (base mixin) ())");
        assert!(found.contains(&("app".to_owned(), "base".to_owned())));
        assert!(found.contains(&("app".to_owned(), "mixin".to_owned())));
    }

    #[test]
    fn collects_parent_type_edges_from_define_condition() {
        let found = edges("(define-condition app-error (error) ())");
        assert!(found.contains(&("app-error".to_owned(), "error".to_owned())));
    }

    #[test]
    fn flags_a_cycle_mixing_defclass_and_define_condition() {
        let mut found = edges("(defclass app (app-error) ())");
        found.extend(edges("(define-condition app-error (app) ())"));

        let summary = analyze_class_cycles(&found);
        assert_eq!(summary.cycles.len(), 1);
        let mut members = summary.cycles[0].members.clone();
        members.sort();
        assert_eq!(members, vec!["app".to_owned(), "app-error".to_owned()]);
    }

    #[test]
    fn does_not_flag_a_simple_inheritance_chain() {
        let mut found = edges("(defclass base () ())");
        found.extend(edges("(defclass derived (base) ())"));

        let summary = analyze_class_cycles(&found);
        assert!(summary.cycles.is_empty());
    }

    #[test]
    fn flags_a_direct_circular_inheritance_between_two_classes() {
        let mut found = edges("(defclass a (b) ())");
        found.extend(edges("(defclass b (a) ())"));

        let summary = analyze_class_cycles(&found);
        assert_eq!(summary.cycles.len(), 1);
        let mut members = summary.cycles[0].members.clone();
        members.sort();
        assert_eq!(members, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn flags_a_longer_cycle_across_three_classes() {
        let mut found = edges("(defclass a (b) ())");
        found.extend(edges("(defclass b (c) ())"));
        found.extend(edges("(defclass c (a) ())"));

        let summary = analyze_class_cycles(&found);
        assert_eq!(summary.cycles.len(), 1);
        assert_eq!(summary.cycles[0].members.len(), 3);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(defclass app (base) [])").expect("parse input");
        let found = collect_class_inheritance_edges(Dialect::Hy, &tree)
            .expect("collect class inheritance edges");
        assert!(found.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let mut found = edges("(defclass a (b) ())");
        found.extend(edges("(defclass b (a) ())"));
        let summary = analyze_class_cycles(&found);

        let quiet = evaluate_class_cycle_policy(ClassCyclePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.cycle_count, 1);

        let strict = evaluate_class_cycle_policy(ClassCyclePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
