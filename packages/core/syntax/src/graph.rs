//! Generic graph algorithms shared by reports that analyze dependency
//! structure (call graphs, package `:use` graphs, and similar), kept
//! independent of any Lisp-specific node representation so a single proven
//! implementation backs every cycle-detection report.

use std::collections::BTreeMap;

/// Builds a directed graph from `(source, target)` string edges, finds every
/// cycle, and returns `(node_count, cycles)` where each cycle is the list of
/// original node names forming one strongly connected component of two or
/// more nodes.
///
/// Two node names are the same node when `needle` maps them to an equal key
/// (e.g. Common Lisp case-folding), so the graph is de-duplicated by that
/// canonical identity while the *first-seen* spelling is what appears in the
/// result. Parallel edges and self-loops are dropped — neither can create a
/// multi-node cycle — and the returned cycle list is sorted for
/// deterministic output. This is the shared core of every string-edge cycle
/// report (`inspect package-cycles`, `system-cycles`, `class-cycles`,
/// `struct-cycles`); each one only wraps the returned names in its own item
/// type.
pub fn string_edge_cycles(
    edges: &[(String, String)],
    needle: impl Fn(&str) -> String,
) -> (usize, Vec<Vec<String>>) {
    let mut names: Vec<String> = Vec::new();
    let mut index_of: BTreeMap<String, usize> = BTreeMap::new();
    let mut adjacency: Vec<Vec<usize>> = Vec::new();

    // `index_of` and `needle` are captured; `names`/`adjacency` are threaded
    // as parameters so the caller can still index `adjacency` between calls
    // without the closure holding a conflicting mutable borrow of them.
    let mut intern =
        |name: &str, names: &mut Vec<String>, adjacency: &mut Vec<Vec<usize>>| -> usize {
            *index_of.entry(needle(name)).or_insert_with(|| {
                names.push(name.to_owned());
                adjacency.push(Vec::new());
                names.len() - 1
            })
        };

    for (source, target) in edges {
        let source_index = intern(source, &mut names, &mut adjacency);
        let target_index = intern(target, &mut names, &mut adjacency);
        if source_index != target_index && !adjacency[source_index].contains(&target_index) {
            adjacency[source_index].push(target_index);
        }
    }

    let mut cycles: Vec<Vec<String>> = tarjan_scc(&adjacency)
        .into_iter()
        .filter(|component| component.len() > 1)
        .map(|component| {
            component
                .into_iter()
                .map(|index| names[index].clone())
                .collect()
        })
        .collect();
    cycles.sort();

    (names.len(), cycles)
}

/// Tarjan's strongly-connected-components algorithm over an adjacency list
/// indexed by node number, implemented with an explicit work stack instead
/// of recursive calls so a pathologically large graph cannot overflow the
/// stack.
///
/// Each node pushes its own `Exit` frame *before* pushing its children's
/// `Enter`/`Propagate` frames, so — because the work list is a LIFO stack —
/// every child's subtree, and every `Propagate` that folds a child's
/// `low_link` back into its parent, is guaranteed to run before the parent's
/// own `Exit` frame checks whether it roots a strongly connected component.
/// Getting that ordering right is the entire correctness argument: an SCC
/// root check that runs even one child's `Propagate` too early would use a
/// stale `low_link` and could split or merge components incorrectly.
#[must_use]
pub fn tarjan_scc(adjacency: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let node_count = adjacency.len();
    let mut index: Vec<Option<usize>> = vec![None; node_count];
    let mut low_link = vec![0usize; node_count];
    let mut on_stack = vec![false; node_count];
    let mut stack = Vec::new();
    let mut next_index = 0usize;
    let mut components = Vec::new();

    enum Frame {
        Enter(usize),
        Propagate(usize, usize),
        Exit(usize),
    }

    for start in 0..node_count {
        if index[start].is_some() {
            continue;
        }

        let mut work = vec![Frame::Enter(start)];
        while let Some(frame) = work.pop() {
            match frame {
                Frame::Enter(node) => {
                    if index[node].is_some() {
                        continue;
                    }
                    index[node] = Some(next_index);
                    low_link[node] = next_index;
                    next_index += 1;
                    stack.push(node);
                    on_stack[node] = true;

                    work.push(Frame::Exit(node));
                    for &neighbor in &adjacency[node] {
                        match index[neighbor] {
                            None => {
                                work.push(Frame::Propagate(node, neighbor));
                                work.push(Frame::Enter(neighbor));
                            }
                            Some(neighbor_index) if on_stack[neighbor] => {
                                low_link[node] = low_link[node].min(neighbor_index);
                            }
                            Some(_) => {}
                        }
                    }
                }
                Frame::Propagate(node, child) => {
                    low_link[node] = low_link[node].min(low_link[child]);
                }
                Frame::Exit(node) => {
                    if index[node] == Some(low_link[node]) {
                        let mut component = Vec::new();
                        while let Some(member) = stack.pop() {
                            on_stack[member] = false;
                            component.push(member);
                            if member == node {
                                break;
                            }
                        }
                        components.push(component);
                    }
                }
            }
        }
    }

    components
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adjacency_from_edges(node_count: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
        let mut adjacency = vec![Vec::new(); node_count];
        for &(from, to) in edges {
            adjacency[from].push(to);
        }
        adjacency
    }

    fn sorted_components(mut components: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
        for component in &mut components {
            component.sort_unstable();
        }
        components.sort();
        components
    }

    #[test]
    fn single_node_with_no_edges_is_its_own_trivial_component() {
        let adjacency = adjacency_from_edges(1, &[]);
        assert_eq!(sorted_components(tarjan_scc(&adjacency)), vec![vec![0]]);
    }

    #[test]
    fn a_chain_has_no_multi_node_components() {
        let adjacency = adjacency_from_edges(3, &[(0, 1), (1, 2)]);
        let components = sorted_components(tarjan_scc(&adjacency));
        assert!(components.iter().all(|component| component.len() == 1));
    }

    #[test]
    fn a_two_node_cycle_forms_one_component() {
        let adjacency = adjacency_from_edges(2, &[(0, 1), (1, 0)]);
        assert_eq!(sorted_components(tarjan_scc(&adjacency)), vec![vec![0, 1]]);
    }

    #[test]
    fn a_three_node_cycle_forms_one_component() {
        let adjacency = adjacency_from_edges(3, &[(0, 1), (1, 2), (2, 0)]);
        assert_eq!(
            sorted_components(tarjan_scc(&adjacency)),
            vec![vec![0, 1, 2]]
        );
    }

    #[test]
    fn a_cycle_does_not_absorb_an_unrelated_acyclic_branch() {
        // 0 <-> 1 is a cycle; 1 -> 2 -> 3 is an acyclic tail hanging off it.
        let adjacency = adjacency_from_edges(4, &[(0, 1), (1, 0), (1, 2), (2, 3)]);
        let components = sorted_components(tarjan_scc(&adjacency));
        assert!(components.contains(&vec![0, 1]));
        assert!(components.contains(&vec![2]));
        assert!(components.contains(&vec![3]));
    }

    #[test]
    fn self_loop_alone_is_a_single_node_component() {
        let adjacency = adjacency_from_edges(1, &[(0, 0)]);
        assert_eq!(sorted_components(tarjan_scc(&adjacency)), vec![vec![0]]);
    }

    fn edge(from: &str, to: &str) -> (String, String) {
        (from.to_owned(), to.to_owned())
    }

    #[test]
    fn string_edge_cycles_reports_node_count_and_finds_a_two_node_cycle() {
        let edges = [edge("a", "b"), edge("b", "a")];
        let (node_count, cycles) = string_edge_cycles(&edges, str::to_owned);
        assert_eq!(node_count, 2);
        assert_eq!(cycles.len(), 1);
        let mut members = cycles[0].clone();
        members.sort();
        assert_eq!(members, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn string_edge_cycles_ignores_an_acyclic_chain() {
        let edges = [edge("a", "b"), edge("b", "c")];
        let (node_count, cycles) = string_edge_cycles(&edges, str::to_owned);
        assert_eq!(node_count, 3);
        assert!(cycles.is_empty());
    }

    #[test]
    fn string_edge_cycles_folds_nodes_by_needle_and_keeps_first_spelling() {
        // Upper- and lower-case names collapse to one node under a
        // case-folding needle; the first-seen spelling ("A") is preserved.
        let edges = [edge("A", "b"), edge("b", "a")];
        let (node_count, cycles) = string_edge_cycles(&edges, |name| name.to_ascii_uppercase());
        assert_eq!(node_count, 2);
        assert_eq!(cycles.len(), 1);
        assert!(cycles[0].contains(&"A".to_owned()));
        assert!(cycles[0].contains(&"b".to_owned()));
    }

    #[test]
    fn string_edge_cycles_drops_self_loops() {
        let edges = [edge("a", "a")];
        let (node_count, cycles) = string_edge_cycles(&edges, str::to_owned);
        assert_eq!(node_count, 1);
        assert!(cycles.is_empty());
    }
}
