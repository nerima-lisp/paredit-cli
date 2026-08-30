use paredit_core_cli::CommandResult;

use crate::call_graph_report::cli::args::CallGraphArgs;
use crate::call_graph_report::cli::render::{call_graph_drawing, print_call_graph_report};
use crate::call_graph_report::usecase::{
    CallGraphPolicyOptions, CallGraphReportSource, build_call_graph_report,
    evaluate_call_graph_policy,
};
use paredit_core_cli::CliResult;
use paredit_core_cli::report::graph::print_graph;
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn call_graph(args: CallGraphArgs) -> CommandResult {
    let symbol = args.symbol.as_ref();

    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&args.files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(CallGraphReportSource {
            path: file.to_path_buf(),
            dialect,
            tree: tree.clone(),
        })
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let sources = analysis.succeeded;

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
