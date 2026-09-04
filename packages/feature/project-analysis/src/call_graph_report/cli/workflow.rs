use paredit_core_cli::CommandResult;

use crate::call_graph_report::cli::args::CallGraphArgs;
use crate::call_graph_report::cli::render::{call_graph_drawing, print_call_graph_report};
use crate::call_graph_report::usecase::{
    CallGraphPolicyOptions, CallGraphReportSource, build_call_graph_report,
    evaluate_call_graph_policy,
};
use paredit_core_cli::report::graph::print_graph;
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn call_graph(args: CallGraphArgs) -> CommandResult {
    let symbol = args.symbol.as_ref();
    let mut sources = Vec::with_capacity(args.files.len());

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;

        sources.push(CallGraphReportSource {
            path: file.clone(),
            dialect,
            tree,
        });
    }

    let report = build_call_graph_report(sources, args.include_external, symbol)?;
    let policy = evaluate_call_graph_policy(
        &report.files,
        symbol,
        CallGraphPolicyOptions::new(
            args.fail_on_inbound_callers,
            args.require_edges,
            args.require_internal_edges,
        )
        .map_err(|message| paredit_core_cli::ArgumentError::FlagCombination { message })?,
    );
    match args.graph {
        Some(format) => print_graph(&call_graph_drawing(&report.files, symbol), format),
        None => print_call_graph_report(
            &report.files,
            &report.nodes_by_name,
            symbol,
            args.include_external,
            &policy,
            args.output,
        )?,
    }
    if !policy.passed {
        return Err(paredit_core_cli::gate::gate_failure(
            "call-graph policy failed",
        ));
    }
    Ok(())
}
