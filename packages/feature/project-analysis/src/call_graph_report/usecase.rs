//! Call graph inventory and policy evaluation.

#[cfg(test)]
mod tests;

pub use crate::domain::call_graph_report::{
    CallGraphDefinitionItem, CallGraphEdge, CallGraphFile, CallGraphNode, CallGraphNodeIndex,
    CallGraphPolicy, CallGraphPolicyOptions, CallGraphReport, CallGraphReportSource,
    build_call_graph_edge, build_call_graph_report, call_graph_edge_matches,
    evaluate_call_graph_policy, insert_call_graph_node,
};
