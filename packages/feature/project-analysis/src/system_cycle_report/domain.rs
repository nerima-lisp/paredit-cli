//! ASDF system-dependency cycle detection: two or more `defsystem` forms
//! whose `:depends-on` clauses form a loop.
//!
//! This is the file/system-level analog of
//! [`crate::package_cycle_report::domain`]'s package `:use` cycle check —
//! same underlying problem (a load-order dependency that cannot be
//! satisfied by any ordering), a different declaration form. A circular
//! ASDF dependency typically surfaces as a confusing build failure only
//! once a project grows past a handful of systems; catching it here is
//! cheaper than debugging it from an ASDF error message.
//!
//! Built on [`paredit_feature_package::dependency_report::domain::build_system_dependency_edges`]
//! (which already knows how to flatten a `:depends-on` clause's designators —
//! a bare name, a nested list of names, or a `(:version name "1.0")` triple —
//! since it reuses the exact same designator collector `inspect dependencies`
//! relies on) and the same [`paredit_core_syntax::graph::tarjan_scc`] cycle search
//! used by [`crate::call_cycle_report::domain`] and
//! [`crate::package_cycle_report::domain`].
//!
//! Scope: a `:depends-on` entry written as a bare keyword (`:lib`, with no
//! surrounding quotes or `#:`) is not collected, because the shared
//! designator collector this report reuses deliberately excludes bare
//! keywords there — a decision this report does not override, so its notion
//! of "a depends-on edge" stays identical to `inspect dependencies`. String
//! (`"lib"`) and uninterned-symbol (`#:lib`) designators, by far the more
//! common ASDF idioms, are fully supported.

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_needle;
use paredit_core_syntax::graph::string_edge_cycles;

#[derive(Debug, Clone)]
pub struct SystemCycleItem {
    /// Members of this strongly connected component, in first-seen order.
    pub members: Vec<String>,
}

#[derive(Debug)]
pub struct SystemCycleSummary {
    pub system_count: usize,
    pub cycles: Vec<SystemCycleItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SystemCyclePolicyOptions {
    fail_on_cycle: bool,
}

impl SystemCyclePolicyOptions {
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
pub struct SystemCyclePolicy {
    pub fail_on_cycle: bool,
    pub system_count: usize,
    pub cycle_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[must_use]
pub fn analyze_system_cycles(edges: &[(String, String)]) -> SystemCycleSummary {
    let (system_count, cycles) = string_edge_cycles(edges, common_lisp_symbol_reference_needle);
    SystemCycleSummary {
        system_count,
        cycles: cycles
            .into_iter()
            .map(|members| SystemCycleItem { members })
            .collect(),
    }
}

#[must_use]
pub fn evaluate_system_cycle_policy(
    options: SystemCyclePolicyOptions,
    summary: &SystemCycleSummary,
) -> SystemCyclePolicy {
    let cycle_count = summary.cycles.len();
    let mut violations = Vec::new();
    if options.fail_on_cycle() && cycle_count > 0 {
        violations.push(format!("cycle_count {cycle_count} exceeds 0"));
    }

    SystemCyclePolicy {
        fail_on_cycle: options.fail_on_cycle(),
        system_count: summary.system_count,
        cycle_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use paredit_feature_package::dependency_report::domain::build_system_dependency_edges;

    fn edges(input: &str) -> Vec<(String, String)> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_system_dependency_edges(&tree, Dialect::CommonLisp).expect("build system edges")
    }

    #[test]
    fn does_not_flag_a_simple_depends_on_chain() {
        let mut found = edges(r#"(asdf:defsystem "app" :depends-on ("util"))"#);
        found.extend(edges(
            r#"(asdf:defsystem "util" :depends-on ("alexandria"))"#,
        ));

        let summary = analyze_system_cycles(&found);
        assert!(summary.cycles.is_empty());
    }

    #[test]
    fn flags_a_direct_circular_depends_on_between_two_systems() {
        let mut found = edges(r#"(asdf:defsystem "app" :depends-on ("lib"))"#);
        found.extend(edges(r#"(asdf:defsystem "lib" :depends-on ("app"))"#));

        let summary = analyze_system_cycles(&found);
        assert_eq!(summary.cycles.len(), 1);
        let mut members = summary.cycles[0].members.clone();
        members.sort();
        assert_eq!(members, vec!["app".to_owned(), "lib".to_owned()]);
    }

    #[test]
    fn flags_a_longer_cycle_across_three_systems() {
        let mut found = edges(r#"(asdf:defsystem "a" :depends-on ("b"))"#);
        found.extend(edges(r#"(asdf:defsystem "b" :depends-on ("c"))"#));
        found.extend(edges(r#"(asdf:defsystem "c" :depends-on ("a"))"#));

        let summary = analyze_system_cycles(&found);
        assert_eq!(summary.cycles.len(), 1);
        assert_eq!(summary.cycles[0].members.len(), 3);
    }

    #[test]
    fn treats_string_and_uninterned_symbol_spellings_as_the_same_system() {
        // A `defsystem` string name and a `:depends-on` list element written
        // as an uninterned `#:` symbol (the common ASDF idiom for avoiding
        // interning the referenced system's name) resolve to the same
        // normalized name, so the cycle is still found across both
        // designator spellings. A bare-keyword depends-on entry (`:lib`)
        // is not exercised here: the shared designator collector this
        // report builds on (`collect_dependency_designators`) deliberately
        // excludes bare keywords, since `inspect dependencies` reserves that
        // syntax for lambda-list-style option markers rather than treating
        // every keyword atom in a depends-on list as a system name.
        let mut found = edges(r#"(asdf:defsystem "app" :depends-on ("lib"))"#);
        found.extend(edges(r#"(asdf:defsystem "lib" :depends-on (#:app))"#));

        let summary = analyze_system_cycles(&found);
        assert_eq!(summary.cycles.len(), 1);
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let mut found = edges(r#"(asdf:defsystem "app" :depends-on ("lib"))"#);
        found.extend(edges(r#"(asdf:defsystem "lib" :depends-on ("app"))"#));
        let summary = analyze_system_cycles(&found);

        let quiet = evaluate_system_cycle_policy(SystemCyclePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.cycle_count, 1);

        let strict = evaluate_system_cycle_policy(SystemCyclePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
