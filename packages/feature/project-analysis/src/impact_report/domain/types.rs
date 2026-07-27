use std::path::PathBuf;

use crate::domain::call_graph_report::CallGraphEdge;
use crate::domain::definition::DefinitionCategory;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, SyntaxTree};
use crate::domain::signature_report::SignatureCallItem;

#[derive(Debug)]
pub struct ImpactReportSource {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub tree: SyntaxTree,
}

impl ImpactReportSource {
    #[must_use]
    pub const fn new(path: PathBuf, dialect: Dialect, tree: SyntaxTree) -> Self {
        Self {
            path,
            dialect,
            tree,
        }
    }
}

#[derive(Debug)]
pub struct ImpactReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub package: Option<String>,
    pub definitions: Vec<ImpactDefinitionItem>,
    pub references: Vec<ImpactSymbolOccurrence>,
    pub calls: Vec<SignatureCallItem>,
    pub inbound_edges: Vec<CallGraphEdge>,
    pub outbound_edges: Vec<CallGraphEdge>,
    pub non_call_reference_count: usize,
}

impl ImpactReportFile {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        path: PathBuf,
        dialect: Dialect,
        package: Option<String>,
        definitions: Vec<ImpactDefinitionItem>,
        references: Vec<ImpactSymbolOccurrence>,
        calls: Vec<SignatureCallItem>,
        inbound_edges: Vec<CallGraphEdge>,
        outbound_edges: Vec<CallGraphEdge>,
        non_call_reference_count: usize,
    ) -> Self {
        Self {
            path,
            dialect,
            package,
            definitions,
            references,
            calls,
            inbound_edges,
            outbound_edges,
            non_call_reference_count,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImpactDefinitionItem {
    pub path: String,
    pub span: ByteSpan,
    pub head: String,
    pub name: Option<String>,
    pub category: DefinitionCategory,
    pub parameter_count: Option<usize>,
    pub parameter_arity: Option<(usize, Option<usize>)>,
    pub body_form_count: Option<usize>,
    pub package: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImpactSymbolOccurrence {
    pub path: String,
    pub span: ByteSpan,
    pub context: Option<ImpactSymbolOccurrenceContext>,
}

#[derive(Debug, Clone)]
pub struct ImpactSymbolOccurrenceContext {
    pub path: String,
    pub span: ByteSpan,
    pub head: Option<String>,
    pub definition_like: bool,
}
