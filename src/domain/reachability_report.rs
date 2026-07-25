//! Call-graph reachability analysis: finds callable definitions that are
//! referenced by other internal calls yet are never reachable from any real
//! entry point — dead-code islands that direct-reference checks such as
//! `unused-definitions` cannot see, because every member of the island is
//! called by some *other* dead member.
//!
//! An entry point is either a top-level call (invoked by simply loading the
//! file, outside any enclosing definition) or a callable definition with no
//! internal caller at all — the latter is either a genuine external
//! entry/export, or a fully unreferenced definition already caught by
//! `unused-definitions`. Reachability is the forward closure of internal
//! call edges from that root set; anything outside the closure, despite
//! having at least one inbound edge, is unreachable dead code.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::domain::call_graph_report::CallGraphFile;
use crate::domain::common_lisp::common_lisp_symbol_reference_needle;
use crate::domain::definition::DefinitionCategory;
use crate::domain::sexpr::ByteSpan;

#[derive(Debug, Clone)]
pub struct ReachabilityReportItem {
    pub path: String,
    pub span: ByteSpan,
    pub name: String,
    pub category: DefinitionCategory,
    /// Number of internal call edges pointing to this definition. Always
    /// `>= 1`: a definition with zero inbound edges is a root by
    /// construction and therefore never appears here.
    pub inbound_edge_count: usize,
}

#[derive(Debug)]
pub struct ReachabilityReportSummary {
    pub callable_definition_count: usize,
    pub root_count: usize,
    pub unreachable: Vec<(usize, ReachabilityReportItem)>,
}

#[derive(Debug, Clone, Copy)]
pub struct ReachabilityReportPolicyOptions {
    fail_on_unreachable: bool,
}

impl ReachabilityReportPolicyOptions {
    pub fn new(fail_on_unreachable: bool) -> Self {
        Self {
            fail_on_unreachable,
        }
    }

    pub const fn fail_on_unreachable(self) -> bool {
        self.fail_on_unreachable
    }
}

#[derive(Debug)]
pub struct ReachabilityReportPolicy {
    pub fail_on_unreachable: bool,
    pub callable_definition_count: usize,
    pub unreachable_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Builds the reachability summary across every scanned file's call graph.
/// `files` and their definitions/edges come from
/// [`crate::domain::call_graph_report::build_call_graph_report`] with
/// `include_external: false`, so every edge in the input already targets a
/// definition known within the scanned file set.
pub fn analyze_reachability(files: &[CallGraphFile]) -> ReachabilityReportSummary {
    // needle -> (file_index, item) for every callable definition, keeping the
    // first-seen location when a name is (re)defined more than once.
    let mut callables: BTreeMap<String, (usize, ReachabilityReportItem)> = BTreeMap::new();
    for (file_index, file) in files.iter().enumerate() {
        for definition in &file.definitions {
            if !definition.category.is_callable() {
                continue;
            }
            let Some(name) = &definition.name else {
                continue;
            };
            let needle = common_lisp_symbol_reference_needle(name);
            callables.entry(needle).or_insert((
                file_index,
                ReachabilityReportItem {
                    path: definition.path.to_string(),
                    span: definition.span,
                    name: name.clone(),
                    category: definition.category,
                    inbound_edge_count: 0,
                },
            ));
        }
    }

    let internal_edges = files
        .iter()
        .flat_map(|file| &file.edges)
        .filter(|edge| edge.internal);

    let mut inbound = BTreeMap::<String, usize>::new();
    let mut adjacency = BTreeMap::<String, BTreeSet<String>>::new();
    let mut top_level_roots = BTreeSet::new();
    for edge in internal_edges {
        let callee_needle = common_lisp_symbol_reference_needle(&edge.callee);
        if !callables.contains_key(&callee_needle) {
            continue;
        }
        match &edge.caller {
            None => {
                // A call outside any enclosing definition runs when the file
                // loads, so its callee is reachable regardless of in-degree.
                top_level_roots.insert(callee_needle);
            }
            Some(caller) => {
                let caller_needle = common_lisp_symbol_reference_needle(caller);
                *inbound.entry(callee_needle.clone()).or_insert(0) += 1;
                adjacency
                    .entry(caller_needle)
                    .or_default()
                    .insert(callee_needle);
            }
        }
    }

    for (needle, (_, item)) in &mut callables {
        item.inbound_edge_count = inbound.get(needle).copied().unwrap_or(0);
    }

    let roots = callables
        .keys()
        .filter(|needle| !inbound.contains_key(*needle))
        .cloned()
        .chain(top_level_roots)
        .collect::<BTreeSet<_>>();

    let mut reachable = BTreeSet::new();
    let mut queue = VecDeque::from_iter(roots.iter().cloned());
    reachable.extend(roots.iter().cloned());
    while let Some(current) = queue.pop_front() {
        if let Some(callees) = adjacency.get(&current) {
            for callee in callees {
                if reachable.insert(callee.clone()) {
                    queue.push_back(callee.clone());
                }
            }
        }
    }

    let mut unreachable = callables
        .into_iter()
        .filter(|(needle, _)| !reachable.contains(needle))
        .map(|(_, (file_index, item))| (file_index, item))
        .collect::<Vec<_>>();
    unreachable.sort_by(|(left_file, left_item), (right_file, right_item)| {
        left_file
            .cmp(right_file)
            .then_with(|| left_item.path.cmp(&right_item.path))
    });

    let callable_definition_count = files
        .iter()
        .flat_map(|file| &file.definitions)
        .filter(|definition| definition.category.is_callable())
        .count();

    ReachabilityReportSummary {
        callable_definition_count,
        root_count: roots.len(),
        unreachable,
    }
}

pub fn evaluate_reachability_policy(
    options: ReachabilityReportPolicyOptions,
    summary: &ReachabilityReportSummary,
) -> ReachabilityReportPolicy {
    let unreachable_count = summary.unreachable.len();
    let mut violations = Vec::new();
    if options.fail_on_unreachable() && unreachable_count > 0 {
        violations.push(format!("unreachable_count {unreachable_count} exceeds 0"));
    }

    ReachabilityReportPolicy {
        fail_on_unreachable: options.fail_on_unreachable(),
        callable_definition_count: summary.callable_definition_count,
        unreachable_count,
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

    fn analyze(sources: Vec<(&str, &str)>) -> ReachabilityReportSummary {
        let sources = sources
            .into_iter()
            .map(|(path, input)| CallGraphReportSource {
                path: PathBuf::from(path),
                dialect: Dialect::CommonLisp,
                tree: SyntaxTree::parse(input).expect("parse input"),
            })
            .collect();
        let report = build_call_graph_report(sources, false, None).expect("build call graph");
        analyze_reachability(&report.files)
    }

    #[test]
    fn root_calling_helpers_marks_everything_reachable() {
        let summary = analyze(vec![(
            "a.lisp",
            "(defun main () (helper))\n(defun helper () (leaf))\n(defun leaf () 1)\n(main)",
        )]);

        assert_eq!(summary.callable_definition_count, 3);
        assert!(summary.unreachable.is_empty());
    }

    #[test]
    fn mutually_calling_cluster_with_no_external_caller_is_unreachable() {
        let summary = analyze(vec![(
            "a.lisp",
            "(defun entry () (used))\n\
             (defun used () 1)\n\
             (defun island-a () (island-b))\n\
             (defun island-b () (island-a))",
        )]);

        let unreachable_names = summary
            .unreachable
            .iter()
            .map(|(_, item)| item.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(unreachable_names, vec!["island-a", "island-b"]);
        for (_, item) in &summary.unreachable {
            assert!(item.inbound_edge_count >= 1);
        }
    }

    #[test]
    fn definitions_with_no_callers_are_roots_not_flagged() {
        let summary = analyze(vec![("a.lisp", "(defun exported-api () 1)")]);

        assert!(summary.unreachable.is_empty());
        assert_eq!(summary.root_count, 1);
    }

    #[test]
    fn reachability_spans_multiple_files() {
        let summary = analyze(vec![
            ("a.lisp", "(defun main () (helper))\n(main)"),
            ("b.lisp", "(defun helper () 1)"),
        ]);

        assert!(summary.unreachable.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let summary = analyze(vec![(
            "a.lisp",
            "(defun island-a () (island-b))\n(defun island-b () (island-a))",
        )]);

        let quiet =
            evaluate_reachability_policy(ReachabilityReportPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.unreachable_count, 2);

        let strict =
            evaluate_reachability_policy(ReachabilityReportPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
