//! Graphviz and Mermaid rendering for the reports whose result is a graph.
//!
//! `call-graph`, `dependencies`, and `class-hierarchy` all compute the same
//! kind of answer — nodes joined by directed edges — and all three previously
//! could only print it as a flat edge list, which is the one shape a reader
//! cannot see the structure in. Two lines of `dot` or `mermaid` output turn the
//! same data into a picture, in a CI comment or a docs page, with no new
//! analysis behind it.
//!
//! Node identifiers are generated (`n0`, `n1`, …) rather than derived from the
//! symbol. Mermaid identifiers may not contain most punctuation, and Lisp
//! symbols are mostly punctuation — `*foo*`, `:keyword`, `car/cdr`, `<=` — so
//! deriving one would mean an escaping scheme with a collision story. Numbering
//! has neither problem, and the real name is the label.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::shared::terminal_safe;

/// What a node is, as far as the picture is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeShape {
    /// Something defined in the scanned sources.
    Definition,
    /// Something referenced but not defined here — an external callee, a
    /// package from another system. Drawn open so the boundary is visible.
    External,
    /// A container: a file, a package, a system.
    Container,
}

/// Whether an edge is a fact or an inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeStyle {
    /// Both ends are defined in the scanned sources.
    Internal,
    /// One end is outside the scanned set, so the edge is only half-verified.
    External,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub label: String,
    pub shape: NodeShape,
    /// An optional subgraph to draw this node inside, such as its file.
    pub group: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub style: EdgeStyle,
}

/// A directed graph, built by a report and rendered by this module.
///
/// Nodes are keyed by their label, so a report that discovers the same symbol
/// in three files adds it once. `add_edge` interns both endpoints, which is
/// what lets a report walk its edge list without first computing a node set.
#[derive(Debug, Clone)]
pub struct Graph {
    pub title: String,
    nodes: BTreeMap<String, Node>,
    edges: Vec<Edge>,
}

impl Graph {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            nodes: BTreeMap::new(),
            edges: Vec::new(),
        }
    }

    /// Records a node, upgrading its shape if it was first seen as external.
    ///
    /// Order of discovery must not decide how a node is drawn: a symbol called
    /// before its `defun` is read is the same symbol, and seeing the definition
    /// later is strictly more information.
    pub fn add_node(&mut self, label: impl Into<String>, shape: NodeShape, group: Option<String>) {
        let label = label.into();
        self.nodes
            .entry(label.clone())
            .and_modify(|node| {
                if node.shape == NodeShape::External && shape != NodeShape::External {
                    node.shape = shape;
                }
                if node.group.is_none() {
                    node.group.clone_from(&group);
                }
            })
            .or_insert(Node {
                label,
                shape,
                group,
            });
    }

    pub fn add_edge(
        &mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        label: Option<String>,
        style: EdgeStyle,
    ) {
        let from = from.into();
        let to = to.into();
        self.add_node(from.clone(), NodeShape::Definition, None);
        self.add_node(
            to.clone(),
            match style {
                EdgeStyle::Internal => NodeShape::Definition,
                EdgeStyle::External => NodeShape::External,
            },
            None,
        );
        self.edges.push(Edge {
            from,
            to,
            label,
            style,
        });
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The generated identifier for each label, assigned in sorted order so the
    /// rendering is byte-identical across runs.
    fn ids(&self) -> BTreeMap<&str, String> {
        self.nodes
            .keys()
            .enumerate()
            .map(|(index, label)| (label.as_str(), format!("n{index}")))
            .collect()
    }

    /// The nodes that declared a group, keyed by it, plus the ungrouped rest.
    fn grouped(&self) -> (BTreeMap<&str, Vec<&Node>>, Vec<&Node>) {
        let mut grouped: BTreeMap<&str, Vec<&Node>> = BTreeMap::new();
        let mut loose = Vec::new();
        for node in self.nodes.values() {
            match &node.group {
                Some(group) => grouped.entry(group.as_str()).or_default().push(node),
                None => loose.push(node),
            }
        }
        (grouped, loose)
    }
}

/// Graphviz DOT.
#[must_use]
pub fn dot(graph: &Graph) -> String {
    let ids = graph.ids();
    let (grouped, loose) = graph.grouped();
    let mut out = String::new();

    let _ = writeln!(out, "digraph paredit {{");
    let _ = writeln!(out, "  label={};", dot_string(&graph.title));
    let _ = writeln!(out, "  labelloc=\"t\";");
    let _ = writeln!(out, "  rankdir=LR;");
    let _ = writeln!(out, "  node [fontname=\"monospace\"];");

    for (index, (group, nodes)) in grouped.iter().enumerate() {
        // `cluster_` is not decoration: Graphviz only draws a subgraph as a box
        // when its name carries that prefix.
        let _ = writeln!(out, "  subgraph cluster_{index} {{");
        let _ = writeln!(out, "    label={};", dot_string(group));
        let _ = writeln!(out, "    style=dotted;");
        for node in nodes {
            let _ = writeln!(out, "    {}", dot_node(&ids, node));
        }
        let _ = writeln!(out, "  }}");
    }
    for node in loose {
        let _ = writeln!(out, "  {}", dot_node(&ids, node));
    }

    for edge in &graph.edges {
        let (Some(from), Some(to)) = (ids.get(edge.from.as_str()), ids.get(edge.to.as_str()))
        else {
            continue;
        };
        let mut attributes = Vec::new();
        if let Some(label) = &edge.label {
            attributes.push(format!("label={}", dot_string(label)));
        }
        if edge.style == EdgeStyle::External {
            attributes.push("style=dashed".to_owned());
        }
        let suffix = if attributes.is_empty() {
            String::new()
        } else {
            format!(" [{}]", attributes.join(", "))
        };
        let _ = writeln!(out, "  {from} -> {to}{suffix};");
    }

    let _ = writeln!(out, "}}");
    out
}

fn dot_node(ids: &BTreeMap<&str, String>, node: &Node) -> String {
    let id = ids.get(node.label.as_str()).map_or("n?", String::as_str);
    let shape = match node.shape {
        NodeShape::Definition => "box",
        NodeShape::External => "ellipse",
        NodeShape::Container => "folder",
    };
    let style = match node.shape {
        NodeShape::External => ", style=dashed",
        _ => "",
    };
    format!(
        "{id} [label={}, shape={shape}{style}];",
        dot_string(&node.label)
    )
}

/// A DOT double-quoted string.
///
/// [`terminal_safe`] first, so a control character in a symbol name cannot
/// reach the renderer or the terminal the `dot` output is printed to.
fn dot_string(text: &str) -> String {
    format!(
        "\"{}\"",
        terminal_safe(text)
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    )
}

/// Mermaid `flowchart`, which GitHub and GitLab render inline in Markdown.
#[must_use]
pub fn mermaid(graph: &Graph) -> String {
    let ids = graph.ids();
    let (grouped, loose) = graph.grouped();
    let mut out = String::new();

    let _ = writeln!(out, "flowchart LR");
    for (index, (group, nodes)) in grouped.iter().enumerate() {
        // Quoted, like every other label here: an unquoted `[/…` is Mermaid's
        // parallelogram shape, and a file path starts with exactly that.
        let _ = writeln!(out, "  subgraph g{index}[\"{}\"]", mermaid_text(group));
        for node in nodes {
            let _ = writeln!(out, "    {}", mermaid_node(&ids, node));
        }
        let _ = writeln!(out, "  end");
    }
    for node in loose {
        let _ = writeln!(out, "  {}", mermaid_node(&ids, node));
    }

    for edge in &graph.edges {
        let (Some(from), Some(to)) = (ids.get(edge.from.as_str()), ids.get(edge.to.as_str()))
        else {
            continue;
        };
        // `-->` for a verified edge, `-.->` for one whose far end was not
        // found in the scanned sources.
        let arrow = match (&edge.label, edge.style) {
            (Some(label), EdgeStyle::Internal) => format!("-- {} -->", mermaid_text(label)),
            (Some(label), EdgeStyle::External) => format!("-. {} .->", mermaid_text(label)),
            (None, EdgeStyle::Internal) => "-->".to_owned(),
            (None, EdgeStyle::External) => "-.->".to_owned(),
        };
        let _ = writeln!(out, "  {from} {arrow} {to}");
    }

    out
}

fn mermaid_node(ids: &BTreeMap<&str, String>, node: &Node) -> String {
    let id = ids.get(node.label.as_str()).map_or("n0", String::as_str);
    let label = mermaid_text(&node.label);
    match node.shape {
        NodeShape::Definition => format!("{id}[\"{label}\"]"),
        NodeShape::External => format!("{id}([\"{label}\"])"),
        NodeShape::Container => format!("{id}[/\"{label}\"/]"),
    }
}

/// Mermaid label text.
///
/// Mermaid's own escape is the HTML-style `#NN;` entity, and it applies to the
/// quote that would close the label and to the `#` that would start another
/// entity. Square brackets and parentheses are safe inside a quoted label, but
/// not outside one, which is why every label this module emits is quoted.
fn mermaid_text(text: &str) -> String {
    terminal_safe(text)
        .to_string()
        .replace('#', "#35;")
        .replace('"', "#quot;")
        .replace('\n', " ")
}

/// Prints a graph in the requested drawing language.
pub fn print_graph(graph: &Graph, format: crate::args::GraphFormat) {
    match format {
        crate::args::GraphFormat::Dot => print!("{}", dot(graph)),
        crate::args::GraphFormat::Mermaid => print!("{}", mermaid(graph)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Graph {
        let mut graph = Graph::new("inspect call-graph");
        graph.add_node(
            "render-pane",
            NodeShape::Definition,
            Some("core.lisp".into()),
        );
        graph.add_edge("render-pane", "compute", None, EdgeStyle::Internal);
        graph.add_edge(
            "render-pane",
            "format",
            Some("2 args".into()),
            EdgeStyle::External,
        );
        graph
    }

    #[test]
    fn an_edge_interns_both_of_its_endpoints() {
        let graph = sample();
        assert_eq!(graph.ids().len(), 3);
    }

    #[test]
    fn a_node_first_seen_as_external_is_upgraded_when_its_definition_arrives() {
        let mut graph = Graph::new("t");
        graph.add_edge("a", "b", None, EdgeStyle::External);
        graph.add_node("b", NodeShape::Definition, None);
        let dot = dot(&graph);
        assert!(dot.contains("label=\"b\", shape=box"), "{dot}");
    }

    #[test]
    fn dot_draws_a_group_as_a_cluster_so_graphviz_boxes_it() {
        let dot = dot(&sample());
        assert!(dot.contains("subgraph cluster_0"), "{dot}");
        assert!(dot.contains("label=\"core.lisp\""), "{dot}");
    }

    #[test]
    fn an_unresolved_callee_is_drawn_open_and_dashed_in_both_formats() {
        let dot = dot(&sample());
        assert!(dot.contains("shape=ellipse, style=dashed"), "{dot}");
        let mermaid = mermaid(&sample());
        assert!(mermaid.contains("([\"format\"])"), "{mermaid}");
        assert!(mermaid.contains("-. 2 args .->"), "{mermaid}");
    }

    #[test]
    fn a_symbol_made_of_punctuation_survives_both_renderers() {
        let mut graph = Graph::new("t");
        graph.add_edge("*state*", "car/cdr\"#[]", None, EdgeStyle::Internal);
        let dot = dot(&graph);
        assert!(dot.contains(r##"label="car/cdr\"#[]""##), "{dot}");
        let mermaid = mermaid(&graph);
        assert!(mermaid.contains("car/cdr#quot;#35;[]"), "{mermaid}");
        // Identifiers are generated, so no punctuation reaches them.
        assert!(mermaid.contains("n0"), "{mermaid}");
    }

    #[test]
    fn rendering_is_byte_identical_across_runs() {
        assert_eq!(dot(&sample()), dot(&sample()));
        assert_eq!(mermaid(&sample()), mermaid(&sample()));
    }
}
