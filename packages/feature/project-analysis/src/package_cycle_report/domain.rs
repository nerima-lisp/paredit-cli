//! Common Lisp package-dependency cycle detection: two or more `defpackage`
//! forms whose `:use`/`:import-from` clauses form a loop (package A uses
//! package B, and B — directly or transitively — uses A back).
//!
//! A circular package dependency is a genuine cross-module initialization
//! problem, not a style nit: packages must be defined in an order where
//! every package a `defpackage` `:use`s already exists, so a cycle among
//! `:use` clauses cannot be satisfied by *any* load order and typically
//! fails to compile as soon as the second package in the cycle is loaded.
//!
//! Built on the same [`crate::domain::dependency_report::build_dependency_report`]
//! extraction `inspect dependencies` already uses for `DefpackageUse` and
//! `DefpackageImportFrom` items (each carries the declaring package as
//! `source` and the used/imported-from package as `target`), and the same
//! [`crate::domain::graph::tarjan_scc`] cycle search
//! [`crate::domain::call_cycle_report`] uses over the call graph — a
//! different graph, the same proven algorithm.

use anyhow::Result;

use crate::domain::common_lisp::{
    common_lisp_symbol_reference_needle, normalize_common_lisp_package_designator,
};
use crate::domain::dependency_report::{DependencyKind, build_dependency_report};
use crate::domain::dialect::Dialect;
use crate::domain::graph::string_edge_cycles;
use crate::domain::sexpr::SyntaxTree;

#[derive(Debug, Clone)]
pub struct PackageCycleItem {
    /// Members of this strongly connected component, in first-seen order.
    pub members: Vec<String>,
}

#[derive(Debug)]
pub struct PackageCycleSummary {
    pub package_count: usize,
    pub cycles: Vec<PackageCycleItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct PackageCyclePolicyOptions {
    fail_on_cycle: bool,
}

impl PackageCyclePolicyOptions {
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
pub struct PackageCyclePolicy {
    pub fail_on_cycle: bool,
    pub package_count: usize,
    pub cycle_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects `defpackage` `:use`/`:import-from` edges from one file. Only
/// Common Lisp declares packages this way, so non-Common-Lisp files
/// contribute no edges (a documented no-op, mirroring how
/// `crate::domain::package_boundary_report` scopes itself to Common Lisp).
pub fn collect_package_dependency_edges(
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<Vec<(String, String)>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let dependencies = build_dependency_report(tree, dialect)?;
    Ok(dependencies
        .dependencies
        .into_iter()
        .filter(|item| {
            matches!(
                item.kind,
                DependencyKind::DefpackageUse | DependencyKind::DefpackageImportFrom
            )
        })
        .filter_map(|item| {
            item.source.map(|source| {
                (
                    normalize_common_lisp_package_designator(&source).to_owned(),
                    normalize_common_lisp_package_designator(&item.target).to_owned(),
                )
            })
        })
        .collect())
}

pub fn analyze_package_cycles(edges: &[(String, String)]) -> PackageCycleSummary {
    let (package_count, cycles) = string_edge_cycles(edges, common_lisp_symbol_reference_needle);
    PackageCycleSummary {
        package_count,
        cycles: cycles
            .into_iter()
            .map(|members| PackageCycleItem { members })
            .collect(),
    }
}

#[must_use]
pub fn evaluate_package_cycle_policy(
    options: PackageCyclePolicyOptions,
    summary: &PackageCycleSummary,
) -> PackageCyclePolicy {
    let cycle_count = summary.cycles.len();
    let mut violations = Vec::new();
    if options.fail_on_cycle() && cycle_count > 0 {
        violations.push(format!("cycle_count {cycle_count} exceeds 0"));
    }

    PackageCyclePolicy {
        fail_on_cycle: options.fail_on_cycle(),
        package_count: summary.package_count,
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
        collect_package_dependency_edges(Dialect::CommonLisp, &tree)
            .expect("collect package dependency edges")
    }

    #[test]
    fn collects_use_edges_from_defpackage() {
        let found = edges("(defpackage :app (:use :cl :lib))");
        assert!(found.contains(&("app".to_owned(), "cl".to_owned())));
        assert!(found.contains(&("app".to_owned(), "lib".to_owned())));
    }

    #[test]
    fn does_not_flag_a_simple_use_chain() {
        let mut found = edges("(defpackage :app (:use :lib))");
        found.extend(edges("(defpackage :lib (:use :cl))"));

        let summary = analyze_package_cycles(&found);
        assert!(summary.cycles.is_empty());
    }

    #[test]
    fn flags_a_direct_circular_use_between_two_packages() {
        let mut found = edges("(defpackage :app (:use :lib))");
        found.extend(edges("(defpackage :lib (:use :app))"));

        let summary = analyze_package_cycles(&found);
        assert_eq!(summary.cycles.len(), 1);
        let mut members = summary.cycles[0].members.clone();
        members.sort();
        assert_eq!(members, vec!["app".to_owned(), "lib".to_owned()]);
    }

    #[test]
    fn flags_a_longer_cycle_across_three_packages() {
        let mut found = edges("(defpackage :a (:use :b))");
        found.extend(edges("(defpackage :b (:use :c))"));
        found.extend(edges("(defpackage :c (:use :a))"));

        let summary = analyze_package_cycles(&found);
        assert_eq!(summary.cycles.len(), 1);
        assert_eq!(summary.cycles[0].members.len(), 3);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(ns app (:require [lib]))").expect("parse input");
        let found = collect_package_dependency_edges(Dialect::Clojure, &tree)
            .expect("collect package dependency edges");
        assert!(found.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let mut found = edges("(defpackage :app (:use :lib))");
        found.extend(edges("(defpackage :lib (:use :app))"));
        let summary = analyze_package_cycles(&found);

        let quiet = evaluate_package_cycle_policy(PackageCyclePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.cycle_count, 1);

        let strict = evaluate_package_cycle_policy(PackageCyclePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
