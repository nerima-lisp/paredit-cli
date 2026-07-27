//! Common Lisp `defstruct` `:include` inheritance cycle detection: two or
//! more structs whose `:include` options form a loop — a genuine
//! compile-time error (`defstruct` signals that the included structure type
//! is undefined the moment the cycle's second member tries to finalize,
//! since the first has not — and never can — finish defining).
//!
//! `defstruct`'s name-and-options position (CLHS 3.4.13) is either a bare
//! symbol (`(defstruct foo ...)`, no options, so no `:include` is possible)
//! or a list whose head is the struct name followed by options such as
//! `(:include parent-name slot-override*)` — this collector reads exactly
//! that second shape.
//!
//! Same graph algorithm as [`crate::call_cycle_report::domain`],
//! [`crate::package_cycle_report::domain`], [`crate::system_cycle_report::domain`],
//! and [`crate::class_cycle_report::domain`]: [`paredit_core_syntax::graph::tarjan_scc`]
//! over an edge list built for this declaration form.
//!
//! Scope: Common Lisp only. An `:include` target defined outside the
//! analyzed fileset never contributes an edge back into the graph and so
//! cannot itself form or hide a cycle.

use crate::error::ProjectAnalysisResult;

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_needle;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::graph::string_edge_cycles;
use paredit_core_syntax::sexpr::{ExpressionKind, Path, SyntaxTree};
use paredit_core_syntax::view_query::{atom_child, atom_text, list_head};

#[derive(Debug, Clone)]
pub struct StructCycleItem {
    /// Members of this strongly connected component, in first-seen order.
    pub members: Vec<String>,
}

#[derive(Debug)]
pub struct StructCycleSummary {
    pub struct_count: usize,
    pub cycles: Vec<StructCycleItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct StructCyclePolicyOptions {
    fail_on_cycle: bool,
}

impl StructCyclePolicyOptions {
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
pub struct StructCyclePolicy {
    pub fail_on_cycle: bool,
    pub struct_count: usize,
    pub cycle_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects `(struct, included_struct)` edges from every top-level
/// `defstruct` form in one file. Only Common Lisp declares structs this
/// way, so non-Common-Lisp files contribute no edges (a documented no-op,
/// mirroring how [`crate::class_cycle_report::domain`] scopes itself).
pub fn collect_struct_inheritance_edges(
    dialect: Dialect,
    tree: &SyntaxTree,
) -> ProjectAnalysisResult<Vec<(String, String)>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let mut edges = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&Path::root_child(index))?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        if !head.eq_ignore_ascii_case("defstruct") {
            continue;
        }
        let Some(name_and_options) = view.children.get(1) else {
            continue;
        };

        // A bare-symbol name slot (`(defstruct foo ...)`) has no options,
        // so it cannot declare an `:include` — nothing to do.
        let Some(struct_name) = atom_text(name_and_options).or_else(|| {
            (name_and_options.kind == ExpressionKind::List)
                .then(|| atom_child(name_and_options, 0))
                .flatten()
        }) else {
            continue;
        };
        if name_and_options.kind != ExpressionKind::List {
            continue;
        }

        for option in name_and_options.children.iter().skip(1) {
            let Some(option_head) = list_head(option) else {
                continue;
            };
            if !option_head.eq_ignore_ascii_case(":include") {
                continue;
            }
            let Some(parent_name) = atom_child(option, 1) else {
                continue;
            };
            edges.push((struct_name.to_owned(), parent_name.to_owned()));
        }
    }
    Ok(edges)
}

pub fn analyze_struct_cycles(edges: &[(String, String)]) -> StructCycleSummary {
    let (struct_count, cycles) = string_edge_cycles(edges, common_lisp_symbol_reference_needle);
    StructCycleSummary {
        struct_count,
        cycles: cycles
            .into_iter()
            .map(|members| StructCycleItem { members })
            .collect(),
    }
}

#[must_use]
pub fn evaluate_struct_cycle_policy(
    options: StructCyclePolicyOptions,
    summary: &StructCycleSummary,
) -> StructCyclePolicy {
    let cycle_count = summary.cycles.len();
    let mut violations = Vec::new();
    if options.fail_on_cycle() && cycle_count > 0 {
        violations.push(format!("cycle_count {cycle_count} exceeds 0"));
    }

    StructCyclePolicy {
        fail_on_cycle: options.fail_on_cycle(),
        struct_count: summary.struct_count,
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
        collect_struct_inheritance_edges(Dialect::CommonLisp, &tree)
            .expect("collect struct inheritance edges")
    }

    #[test]
    fn collects_include_edge_from_defstruct() {
        let found = edges("(defstruct (line (:include shape)) (length 0))");
        assert!(found.contains(&("line".to_owned(), "shape".to_owned())));
    }

    #[test]
    fn ignores_a_bare_symbol_defstruct_with_no_options() {
        let found = edges("(defstruct point (x 0) (y 0))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_simple_include_chain() {
        let mut found = edges("(defstruct shape (name nil))");
        found.extend(edges("(defstruct (line (:include shape)) (length 0))"));

        let summary = analyze_struct_cycles(&found);
        assert!(summary.cycles.is_empty());
    }

    #[test]
    fn flags_a_direct_circular_include_between_two_structs() {
        let mut found = edges("(defstruct (a (:include b)) (x 0))");
        found.extend(edges("(defstruct (b (:include a)) (y 0))"));

        let summary = analyze_struct_cycles(&found);
        assert_eq!(summary.cycles.len(), 1);
        let mut members = summary.cycles[0].members.clone();
        members.sort();
        assert_eq!(members, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn flags_a_longer_cycle_across_three_structs() {
        let mut found = edges("(defstruct (a (:include b)) (x 0))");
        found.extend(edges("(defstruct (b (:include c)) (y 0))"));
        found.extend(edges("(defstruct (c (:include a)) (z 0))"));

        let summary = analyze_struct_cycles(&found);
        assert_eq!(summary.cycles.len(), 1);
        assert_eq!(summary.cycles[0].members.len(), 3);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(defstruct (a (:include b)) (x 0))").expect("parse input");
        let found = collect_struct_inheritance_edges(Dialect::Clojure, &tree)
            .expect("collect struct inheritance edges");
        assert!(found.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let mut found = edges("(defstruct (a (:include b)) (x 0))");
        found.extend(edges("(defstruct (b (:include a)) (y 0))"));
        let summary = analyze_struct_cycles(&found);

        let quiet = evaluate_struct_cycle_policy(StructCyclePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.cycle_count, 1);

        let strict = evaluate_struct_cycle_policy(StructCyclePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
