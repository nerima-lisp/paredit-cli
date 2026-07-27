//! ASDF system ordering: `:depends-on` edges and the analysis order they imply.
//!
//! Moved out of `domain::semantics` for the package split. It reads as project
//! analysis rather than language semantics, and more concretely it was the one
//! thing making `core/semantics` depend on `dependency_report` and
//! `system_cycle_report`, which are feature-level reports. Core must not name a
//! feature, so this stays in the root crate until the project-analysis feature
//! package exists to receive it.
//!
//! Unused by design, exactly as it was inside `semantics`: cross-file constant
//! resolution turned out not to need it, because the project table carries a
//! value only for a `defconstant` defined exactly once project-wide, and
//! "exactly once" holds however the files are visited. An analysis whose answer
//! depends on which file was seen first would need it, and none does yet. Its
//! own tests are what exercise it.
#![allow(dead_code, reason = "retained resolver, see the module docs")]

//! The order a project's systems must be analysed in.
//!
//! ASDF `:depends-on` is the only load order read here. A source-level `load`
//! or `require` is *not* tracked: either can name a pathname computed at run
//! time and neither has to appear at top level, so an order derived from them
//! would be a guess rather than a fact — and this layer records only what it
//! can prove.
//!
//! A cycle has no order at all, so it is returned rather than broken by
//! dropping an edge: silently picking one of the two possible orders would
//! make every downstream answer depend on which edge happened to lose.

use std::collections::BTreeMap;

use crate::error::ProjectAnalysisResult;

use crate::system_cycle_report::domain::analyze_system_cycles;
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_needle;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use paredit_feature_package::dependency_report::domain::build_system_dependency_edges;

/// A dependency loop, which no ordering can satisfy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemOrderCycle {
    /// One strongly connected component per entry, members in first-seen
    /// spelling.
    pub cycles: Vec<Vec<String>>,
}

/// Collects the `:depends-on` edges of every `defsystem` in `trees`.
///
/// Delegates to [`build_system_dependency_edges`], which already flattens the
/// designator spellings ASDF accepts — a bare name, a nested list, or a
/// `(:version name "1.0")` triple.
pub fn system_dependency_edges(
    trees: &[&SyntaxTree],
    dialect: Dialect,
) -> ProjectAnalysisResult<Vec<(String, String)>> {
    let mut edges = Vec::new();
    for tree in trees {
        edges.extend(build_system_dependency_edges(tree, dialect)?);
    }
    Ok(edges)
}

/// The analysis order implied by `edges`, dependencies first.
///
/// `edges` are `(system, depended_on_system)` pairs as
/// [`build_system_dependency_edges`] produces them. Two spellings of one
/// system name are the same node under the same case-folding
/// [`analyze_system_cycles`] uses, and the first-seen spelling is what the
/// order reports.
pub fn resolve_system_order(edges: &[(String, String)]) -> Result<Vec<String>, SystemOrderCycle> {
    let summary = analyze_system_cycles(edges);
    if !summary.cycles.is_empty() {
        return Err(SystemOrderCycle {
            cycles: summary
                .cycles
                .into_iter()
                .map(|cycle| cycle.members)
                .collect(),
        });
    }

    let graph = Graph::of(edges);
    Ok(graph.postorder())
}

/// The dependency graph, interned by case-folded name.
struct Graph {
    names: Vec<String>,
    adjacency: Vec<Vec<usize>>,
}

impl Graph {
    fn of(edges: &[(String, String)]) -> Self {
        let mut graph = Self {
            names: Vec::new(),
            adjacency: Vec::new(),
        };
        let mut index_of: BTreeMap<String, usize> = BTreeMap::new();

        for (source, target) in edges {
            let source = graph.intern(source, &mut index_of);
            let target = graph.intern(target, &mut index_of);
            // A self-dependency is not a cycle worth reporting and adding it
            // would make the walk below see a false back edge.
            if source != target && !graph.adjacency[source].contains(&target) {
                graph.adjacency[source].push(target);
            }
        }

        graph
    }

    fn intern(&mut self, name: &str, index_of: &mut BTreeMap<String, usize>) -> usize {
        *index_of
            .entry(common_lisp_symbol_reference_needle(name))
            .or_insert_with(|| {
                self.names.push(name.to_owned());
                self.adjacency.push(Vec::new());
                self.names.len() - 1
            })
    }

    /// Depth-first post-order: a system is emitted only once everything it
    /// depends on has been. Valid as a topological order because the caller
    /// has already proved the graph acyclic.
    ///
    /// Iterative rather than recursive so a deep dependency chain cannot
    /// overflow the stack.
    fn postorder(&self) -> Vec<String> {
        let mut seen = vec![false; self.names.len()];
        let mut order = Vec::with_capacity(self.names.len());

        // Roots are visited in first-seen order, which is what makes the
        // result stable across runs.
        for root in 0..self.names.len() {
            if seen[root] {
                continue;
            }
            let mut stack = vec![(root, false)];
            while let Some((node, emit)) = stack.pop() {
                if emit {
                    order.push(self.names[node].clone());
                    continue;
                }
                if seen[node] {
                    continue;
                }
                seen[node] = true;
                stack.push((node, true));
                // Reversed so dependencies are walked in declaration order.
                for &next in self.adjacency[node].iter().rev() {
                    if !seen[next] {
                        stack.push((next, false));
                    }
                }
            }
        }

        order
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(from, to)| ((*from).to_owned(), (*to).to_owned()))
            .collect()
    }

    fn position(order: &[String], name: &str) -> usize {
        order
            .iter()
            .position(|entry| entry.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} is missing from {order:?}"))
    }

    #[test]
    fn dependencies_are_analysed_before_their_dependents() {
        let order = resolve_system_order(&edges(&[("app", "core"), ("core", "util")]))
            .expect("acyclic graph has an order");
        assert!(position(&order, "util") < position(&order, "core"));
        assert!(position(&order, "core") < position(&order, "app"));
    }

    #[test]
    fn a_diamond_still_puts_the_shared_dependency_first() {
        let order = resolve_system_order(&edges(&[
            ("app", "left"),
            ("app", "right"),
            ("left", "base"),
            ("right", "base"),
        ]))
        .expect("acyclic graph has an order");
        assert!(position(&order, "base") < position(&order, "left"));
        assert!(position(&order, "base") < position(&order, "right"));
        assert!(position(&order, "right") < position(&order, "app"));
    }

    #[test]
    fn a_cycle_is_reported_rather_than_broken() {
        // Dropping an edge would produce an order, but which one depends on
        // which edge lost — so every downstream answer would too.
        let cycle = resolve_system_order(&edges(&[("app", "core"), ("core", "app")]))
            .expect_err("a cycle has no order");
        assert_eq!(cycle.cycles.len(), 1);
        assert_eq!(cycle.cycles[0].len(), 2);
    }

    #[test]
    fn an_empty_project_has_an_empty_order() {
        assert_eq!(resolve_system_order(&[]), Ok(Vec::new()));
    }

    #[test]
    fn depends_on_edges_come_from_the_defsystem_forms() {
        let input = r#"(defsystem "app" :depends-on ("core" (:version "util" "1.0")))"#;
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let collected =
            system_dependency_edges(&[&tree], Dialect::CommonLisp).expect("collect edges");
        let names: Vec<&str> = collected.iter().map(|(_, to)| to.as_str()).collect();
        assert!(names.iter().any(|name| name.eq_ignore_ascii_case("core")));
        assert!(names.iter().any(|name| name.eq_ignore_ascii_case("util")));
    }
}
