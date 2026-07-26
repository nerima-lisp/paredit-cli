//! Call-graph cycle detection: strongly connected components of two or more
//! distinct callable definitions in the internal call graph.
//!
//! This is a different question from
//! [`crate::domain::reachability_report`], which asks "can any entry point
//! reach this definition at all." A cycle can exist entirely among
//! definitions that *are* reachable — mutual recursion is not dead code —
//! but an unexpected cycle is still worth surfacing: it can indicate an
//! accidental circular dependency introduced while refactoring, or a design
//! smell where two definitions should be merged or have their
//! responsibilities untangled. A single definition calling only itself
//! (ordinary self-recursion) is not reported: that pattern is the normal,
//! expected shape of a recursive function, not a cross-definition cycle.
//!
//! Built on the same [`crate::domain::call_graph_report::CallGraphFile`]
//! edge data `inspect call-graph` and `inspect reachability` already use, so
//! this report's notion of "a call edge" never drifts from theirs. Cycle
//! detection itself is [`crate::domain::graph::tarjan_scc`], the same
//! generic strongly-connected-components search
//! [`crate::domain::package_cycle_report`] uses over a different graph, so
//! there is exactly one proven cycle-detection implementation in the
//! codebase rather than one per report.

use std::collections::BTreeMap;

use crate::domain::call_graph_report::CallGraphFile;
use crate::domain::common_lisp::common_lisp_symbol_reference_needle;
use crate::domain::graph::tarjan_scc;

#[derive(Debug, Clone)]
pub struct CallCycleItem {
    /// Members of this strongly connected component, in call-graph
    /// insertion order (not necessarily alphabetical).
    pub members: Vec<String>,
}

#[derive(Debug)]
pub struct CallCycleSummary {
    pub callable_definition_count: usize,
    pub cycles: Vec<CallCycleItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct CallCyclePolicyOptions {
    fail_on_cycle: bool,
}

impl CallCyclePolicyOptions {
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
pub struct CallCyclePolicy {
    pub fail_on_cycle: bool,
    pub callable_definition_count: usize,
    pub cycle_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[must_use]
pub fn analyze_call_cycles(files: &[CallGraphFile]) -> CallCycleSummary {
    // needle -> display name, first-seen order preserved via a Vec index.
    let mut names = Vec::new();
    let mut index_of = BTreeMap::<String, usize>::new();
    for file in files {
        for definition in &file.definitions {
            if !definition.category.is_callable() {
                continue;
            }
            let Some(name) = &definition.name else {
                continue;
            };
            let needle = common_lisp_symbol_reference_needle(name);
            index_of.entry(needle).or_insert_with(|| {
                names.push(name.clone());
                names.len() - 1
            });
        }
    }

    let mut adjacency = vec![Vec::new(); names.len()];
    for edge in files.iter().flat_map(|file| &file.edges) {
        if !edge.internal {
            continue;
        }
        let Some(caller) = &edge.caller else {
            continue;
        };
        let caller_needle = common_lisp_symbol_reference_needle(caller);
        let callee_needle = common_lisp_symbol_reference_needle(&edge.callee);
        let (Some(&caller_index), Some(&callee_index)) =
            (index_of.get(&caller_needle), index_of.get(&callee_needle))
        else {
            continue;
        };
        if caller_index != callee_index && !adjacency[caller_index].contains(&callee_index) {
            adjacency[caller_index].push(callee_index);
        }
    }

    let components = tarjan_scc(&adjacency);
    let mut cycles = components
        .into_iter()
        .filter(|component| component.len() > 1)
        .map(|component| CallCycleItem {
            members: component
                .into_iter()
                .map(|index| names[index].clone())
                .collect(),
        })
        .collect::<Vec<_>>();
    cycles.sort_by(|left, right| left.members.cmp(&right.members));

    CallCycleSummary {
        callable_definition_count: names.len(),
        cycles,
    }
}

#[must_use]
pub fn evaluate_call_cycle_policy(
    options: CallCyclePolicyOptions,
    summary: &CallCycleSummary,
) -> CallCyclePolicy {
    let cycle_count = summary.cycles.len();
    let mut violations = Vec::new();
    if options.fail_on_cycle() && cycle_count > 0 {
        violations.push(format!("cycle_count {cycle_count} exceeds 0"));
    }

    CallCyclePolicy {
        fail_on_cycle: options.fail_on_cycle(),
        callable_definition_count: summary.callable_definition_count,
        cycle_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::call_graph_report::{CallGraphReportSource, build_call_graph_report};
    use crate::domain::dialect::Dialect;
    use crate::domain::sexpr::SyntaxTree;
    use std::path::PathBuf;

    fn analyze(sources: Vec<(&str, &str)>) -> CallCycleSummary {
        let sources = sources
            .into_iter()
            .map(|(path, input)| CallGraphReportSource {
                path: PathBuf::from(path),
                dialect: Dialect::CommonLisp,
                tree: SyntaxTree::parse(input).expect("parse input"),
            })
            .collect();
        let report = build_call_graph_report(sources, false, None).expect("build call graph");
        analyze_call_cycles(&report.files)
    }

    #[test]
    fn does_not_flag_a_simple_call_chain() {
        let summary = analyze(vec![(
            "a.lisp",
            "(defun main () (helper))\n(defun helper () (leaf))\n(defun leaf () 1)",
        )]);

        assert_eq!(summary.callable_definition_count, 3);
        assert!(summary.cycles.is_empty());
    }

    #[test]
    fn does_not_flag_ordinary_self_recursion() {
        let summary = analyze(vec![(
            "a.lisp",
            "(defun countdown (n) (if (= n 0) 0 (countdown (- n 1))))",
        )]);

        assert!(summary.cycles.is_empty());
    }

    #[test]
    fn flags_direct_mutual_recursion_between_two_definitions() {
        let summary = analyze(vec![(
            "a.lisp",
            "(defun odd? (n) (if (= n 0) nil (even? (- n 1))))\n\
             (defun even? (n) (if (= n 0) t (odd? (- n 1))))",
        )]);

        assert_eq!(summary.cycles.len(), 1);
        let mut members = summary.cycles[0].members.clone();
        members.sort();
        assert_eq!(members, vec!["even?".to_owned(), "odd?".to_owned()]);
    }

    #[test]
    fn flags_a_longer_cycle_across_three_definitions() {
        let summary = analyze(vec![(
            "a.lisp",
            "(defun a (n) (b n))\n(defun b (n) (c n))\n(defun c (n) (a n))",
        )]);

        assert_eq!(summary.cycles.len(), 1);
        assert_eq!(summary.cycles[0].members.len(), 3);
    }

    #[test]
    fn does_not_leak_a_cycle_into_unrelated_acyclic_definitions() {
        let summary = analyze(vec![(
            "a.lisp",
            "(defun odd? (n) (if (= n 0) nil (even? (- n 1))))\n\
             (defun even? (n) (if (= n 0) t (odd? (- n 1))))\n\
             (defun main () (helper))\n\
             (defun helper () (leaf))\n\
             (defun leaf () 1)",
        )]);

        assert_eq!(summary.callable_definition_count, 5);
        assert_eq!(summary.cycles.len(), 1);
        let mut members = summary.cycles[0].members.clone();
        members.sort();
        assert_eq!(members, vec!["even?".to_owned(), "odd?".to_owned()]);
    }

    #[test]
    fn finds_cycles_spanning_multiple_files() {
        let summary = analyze(vec![
            ("a.lisp", "(defun a (n) (b n))"),
            ("b.lisp", "(defun b (n) (a n))"),
        ]);

        assert_eq!(summary.cycles.len(), 1);
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let summary = analyze(vec![(
            "a.lisp",
            "(defun odd? (n) (even? n))\n(defun even? (n) (odd? n))",
        )]);

        let quiet = evaluate_call_cycle_policy(CallCyclePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.cycle_count, 1);

        let strict = evaluate_call_cycle_policy(CallCyclePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
