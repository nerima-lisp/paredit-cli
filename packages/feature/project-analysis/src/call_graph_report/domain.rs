use crate::error::ProjectAnalysisResult;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use crate::call_report::domain::{CallReportItem, build_call_report};
use paredit_core_syntax::common_lisp::{
    CommonLispPackageDeclarationForm, common_lisp_symbol_reference_eq,
    common_lisp_symbol_reference_needle,
};
use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, NonEmptyExpressionPath, Path, SymbolName,
    SyntaxTree,
};

#[derive(Debug)]
pub struct CallGraphReportSource {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub tree: SyntaxTree,
}

#[derive(Debug)]
pub struct CallGraphReport {
    pub files: Vec<CallGraphFile>,
    pub nodes_by_name: BTreeMap<String, CallGraphNode>,
}

#[derive(Debug)]
pub struct CallGraphFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub definitions: Vec<CallGraphDefinitionItem>,
    pub edges: Vec<CallGraphEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallGraphDefinitionItem {
    pub name: Option<String>,
    pub category: DefinitionCategory,
    pub path: Path,
    pub span: ByteSpan,
    pub parameter_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallGraphPolicy {
    pub inbound_callers: BTreeSet<String>,
    pub inbound_edge_count: usize,
    pub edge_count: usize,
    pub internal_edge_count: usize,
    pub fail_on_inbound_callers: bool,
    pub require_edges: Option<usize>,
    pub require_internal_edges: Option<usize>,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallGraphNode {
    pub name: String,
    pub definition_count: usize,
    pub categories: BTreeSet<DefinitionCategory>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallGraphEdge {
    pub caller: Option<String>,
    pub callee: String,
    pub path: String,
    pub argument_count: usize,
    pub span: ByteSpan,
    pub internal: bool,
    pub callee_categories: BTreeSet<DefinitionCategory>,
}

#[derive(Debug, Clone, Copy)]
pub struct CallGraphPolicyOptions {
    fail_on_inbound_callers: bool,
    require_edges: Option<usize>,
    require_internal_edges: Option<usize>,
}

pub type CallGraphNodeIndex = HashMap<String, String>;

pub fn build_call_graph_report(
    sources: Vec<CallGraphReportSource>,
    include_external: bool,
    symbol: Option<&SymbolName>,
) -> ProjectAnalysisResult<CallGraphReport> {
    let mut parsed = Vec::with_capacity(sources.len());
    let mut nodes_by_name = BTreeMap::<String, CallGraphNode>::new();
    let mut node_index = CallGraphNodeIndex::new();

    for source in sources {
        let definitions = collect_call_graph_definitions(&source.tree, source.dialect)?;
        let calls = build_call_report(&source.tree, source.dialect, None, false)?;
        for definition in &definitions {
            insert_call_graph_node(
                &mut nodes_by_name,
                &mut node_index,
                definition.name.as_deref(),
                definition.category,
            );
        }
        parsed.push((source.path, source.dialect, definitions, calls));
    }

    let files = parsed
        .into_iter()
        .map(|(path, dialect, definitions, calls)| {
            let edges = calls
                .into_iter()
                .map(|call| build_call_graph_edge(call, &nodes_by_name, &node_index))
                .filter(|edge| include_external || edge.internal)
                .filter(|edge| call_graph_edge_matches(edge, symbol))
                .collect();
            CallGraphFile {
                path,
                dialect,
                definitions,
                edges,
            }
        })
        .collect();

    Ok(CallGraphReport {
        files,
        nodes_by_name,
    })
}

pub fn insert_call_graph_node(
    nodes_by_name: &mut BTreeMap<String, CallGraphNode>,
    node_index: &mut CallGraphNodeIndex,
    name: Option<&str>,
    category: DefinitionCategory,
) {
    if let Some(name) = name {
        let needle = common_lisp_symbol_reference_needle(name);
        if let Some(node) = node_index
            .get(&needle)
            .and_then(|key| nodes_by_name.get_mut(key))
        {
            node.definition_count += 1;
            node.categories.insert(category);
        } else {
            nodes_by_name.insert(
                name.to_owned(),
                CallGraphNode {
                    name: name.to_owned(),
                    definition_count: 1,
                    categories: BTreeSet::from([category]),
                },
            );
            node_index.insert(needle, name.to_owned());
        }
    }
}

#[must_use]
pub fn build_call_graph_edge(
    call: CallReportItem,
    nodes_by_name: &BTreeMap<String, CallGraphNode>,
    node_index: &CallGraphNodeIndex,
) -> CallGraphEdge {
    let categories = node_index
        .get(&common_lisp_symbol_reference_needle(&call.head))
        .and_then(|key| nodes_by_name.get(key))
        .map(|node| node.categories.clone())
        .unwrap_or_default();
    CallGraphEdge {
        caller: call.enclosing_definition,
        callee: call.head,
        path: call.path,
        argument_count: call.argument_count,
        span: call.span,
        internal: !categories.is_empty(),
        callee_categories: categories,
    }
}

#[must_use]
pub fn call_graph_edge_matches(edge: &CallGraphEdge, symbol: Option<&SymbolName>) -> bool {
    symbol
        .map(|symbol| {
            edge.caller
                .as_deref()
                .is_some_and(|caller| common_lisp_symbol_reference_eq(caller, symbol.as_str()))
                || common_lisp_symbol_reference_eq(&edge.callee, symbol.as_str())
        })
        .unwrap_or(true)
}

fn collect_call_graph_definitions(
    tree: &SyntaxTree,
    dialect: Dialect,
) -> ProjectAnalysisResult<Vec<CallGraphDefinitionItem>> {
    let mut items = Vec::new();
    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        let _ = collect_call_graph_definitions_from_view(&view, dialect, path, &mut items);
    }
    Ok(items)
}

#[derive(Clone, Copy)]
struct CallGraphDefinitionPath(usize);

struct CallGraphDefinitionPathNode {
    parent: Option<usize>,
    index: usize,
}

struct CallGraphDefinitionPathArena {
    nodes: Vec<CallGraphDefinitionPathNode>,
    edge_count: usize,
    materialized_path_count: usize,
}

impl CallGraphDefinitionPathArena {
    fn from_path(path: NonEmptyExpressionPath<'_>) -> (Self, CallGraphDefinitionPath) {
        let mut nodes = Vec::with_capacity(path.indexes().len());
        let mut parent = None;
        for index in path.indexes() {
            let node = nodes.len();
            nodes.push(CallGraphDefinitionPathNode { parent, index });
            parent = Some(node);
        }
        let root = parent.unwrap_or_else(|| unreachable!("non-empty path has a root node"));
        (
            Self {
                nodes,
                edge_count: 0,
                materialized_path_count: 0,
            },
            CallGraphDefinitionPath(root),
        )
    }

    fn child(&mut self, path: CallGraphDefinitionPath, index: usize) -> CallGraphDefinitionPath {
        let node = self.nodes.len();
        self.nodes.push(CallGraphDefinitionPathNode {
            parent: Some(path.0),
            index,
        });
        self.edge_count += 1;
        CallGraphDefinitionPath(node)
    }

    fn materialize(&mut self, path: CallGraphDefinitionPath) -> Path {
        let mut indexes = Vec::new();
        let mut cursor = Some(path.0);
        while let Some(node) = cursor {
            indexes.push(self.nodes[node].index);
            cursor = self.nodes[node].parent;
        }
        indexes.reverse();
        self.materialized_path_count += 1;
        Path::from_indexes(indexes)
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))]
struct CallGraphDefinitionTraversalStats {
    visited_count: usize,
    edge_count: usize,
    materialized_path_count: usize,
}

fn collect_call_graph_definitions_from_view(
    view: &ExpressionView,
    dialect: Dialect,
    path: Path,
    items: &mut Vec<CallGraphDefinitionItem>,
) -> Option<CallGraphDefinitionTraversalStats> {
    let path = NonEmptyExpressionPath::try_from(&path).ok()?;
    let (mut paths, root_path) = CallGraphDefinitionPathArena::from_path(path);
    let mut stack = vec![(view, root_path)];
    let mut visited_count = 0;

    while let Some((view, path)) = stack.pop() {
        visited_count += 1;
        if view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren) {
            if let Some(head) = list_head(view) {
                let is_in_package = dialect.common_lisp_package_declaration_form_for_head(head)
                    == Some(CommonLispPackageDeclarationForm::InPackage);
                if !is_in_package {
                    if let Some(shape) = definition_shape(dialect, view, head) {
                        items.push(CallGraphDefinitionItem {
                            name: shape.name(view).map(str::to_owned),
                            category: shape.category,
                            path: paths.materialize(path),
                            span: view.span,
                            parameter_count: shape.lambda_parameter_count(view).unwrap_or(0),
                        });
                    }
                }
            }
        }

        for (index, child) in view.children.iter().enumerate().rev() {
            let child_path = paths.child(path, index);
            stack.push((child, child_path));
        }
    }

    Some(CallGraphDefinitionTraversalStats {
        visited_count,
        edge_count: paths.edge_count,
        materialized_path_count: paths.materialized_path_count,
    })
}

fn list_head(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren))
        .then_some(())
        .and_then(|_| view.children.first())
        .and_then(|child| {
            (child.kind == ExpressionKind::Atom)
                .then_some(child.text.as_deref())
                .flatten()
        })
}

impl CallGraphPolicyOptions {
    pub fn new(
        fail_on_inbound_callers: bool,
        require_edges: Option<usize>,
        require_internal_edges: Option<usize>,
    ) -> Result<Self, String> {
        Self::validate_threshold("require-edges", require_edges)?;
        Self::validate_threshold("require-internal-edges", require_internal_edges)?;

        Ok(Self {
            fail_on_inbound_callers,
            require_edges,
            require_internal_edges,
        })
    }

    fn validate_threshold(label: &str, value: Option<usize>) -> Result<(), String> {
        if matches!(value, Some(0)) {
            return Err(format!("{label} must be greater than zero"));
        }
        Ok(())
    }

    #[must_use]
    pub const fn fail_on_inbound_callers(self) -> bool {
        self.fail_on_inbound_callers
    }

    #[must_use]
    pub const fn require_edges(self) -> Option<usize> {
        self.require_edges
    }

    #[must_use]
    pub const fn require_internal_edges(self) -> Option<usize> {
        self.require_internal_edges
    }
}

#[must_use]
pub fn evaluate_call_graph_policy(
    reports: &[CallGraphFile],
    symbol: Option<&SymbolName>,
    options: CallGraphPolicyOptions,
) -> CallGraphPolicy {
    let mut policy = CallGraphPolicy {
        fail_on_inbound_callers: options.fail_on_inbound_callers(),
        require_edges: options.require_edges(),
        require_internal_edges: options.require_internal_edges(),
        passed: true,
        ..CallGraphPolicy::default()
    };

    for edge in reports.iter().flat_map(|report| &report.edges) {
        if symbol
            .map(|symbol| {
                edge.caller
                    .as_deref()
                    .is_some_and(|caller| common_lisp_symbol_reference_eq(caller, symbol.as_str()))
                    || common_lisp_symbol_reference_eq(&edge.callee, symbol.as_str())
            })
            .unwrap_or(true)
        {
            policy.edge_count += 1;
            if edge.internal {
                policy.internal_edge_count += 1;
            }
        }

        if let Some(symbol) = symbol {
            let callee_matches = common_lisp_symbol_reference_eq(&edge.callee, symbol.as_str());
            let caller_matches = edge
                .caller
                .as_deref()
                .is_some_and(|caller| common_lisp_symbol_reference_eq(caller, symbol.as_str()));
            if callee_matches && !caller_matches {
                policy.inbound_edge_count += 1;
                if let Some(caller) = &edge.caller {
                    policy.inbound_callers.insert(caller.clone());
                }
            }
        }
    }

    if options.fail_on_inbound_callers() && !policy.inbound_callers.is_empty() {
        policy.passed = false;
        policy.violations.push(format!(
            "focused symbol has inbound callers: {}",
            policy
                .inbound_callers
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if let Some(required) = options.require_edges() {
        if policy.edge_count < required {
            policy.passed = false;
            policy.violations.push(format!(
                "edge count {} is below required {}",
                policy.edge_count, required
            ));
        }
    }

    if let Some(required) = options.require_internal_edges() {
        if policy.internal_edge_count < required {
            policy.passed = false;
            policy.violations.push(format!(
                "internal edge count {} is below required {}",
                policy.internal_edge_count, required
            ));
        }
    }

    policy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deeply_nested_definition_scan_has_linear_path_work() {
        const DEPTH: usize = 6_000;
        let mut input = String::with_capacity(DEPTH * 2 + 1);
        for _ in 0..DEPTH {
            input.push('(');
        }
        input.push('x');
        for _ in 0..DEPTH {
            input.push(')');
        }

        let tree = SyntaxTree::parse(&input).expect("deep input parses");
        let path = Path::root_child(0);
        let view = tree.select_path(&path).expect("root exists").view();
        let mut definitions = Vec::new();

        let stats = collect_call_graph_definitions_from_view(
            &view,
            Dialect::CommonLisp,
            path,
            &mut definitions,
        )
        .expect("root-child path is non-empty");

        assert!(definitions.is_empty());
        assert_eq!(stats.visited_count, DEPTH + 1);
        assert_eq!(stats.edge_count, DEPTH);
        assert_eq!(stats.materialized_path_count, 0);
    }

    #[test]
    fn validates_thresholds() {
        assert!(CallGraphPolicyOptions::new(true, Some(1), Some(2)).is_ok());
        assert_eq!(
            CallGraphPolicyOptions::new(false, Some(0), None).unwrap_err(),
            "require-edges must be greater than zero"
        );
        assert_eq!(
            CallGraphPolicyOptions::new(false, None, Some(0)).unwrap_err(),
            "require-internal-edges must be greater than zero"
        );
    }
}
