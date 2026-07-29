use paredit_core_cli::CliResult;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::call_graph_report::usecase::CallGraphFile;
use crate::reachability_report::usecase::{ReachabilityReportPolicy, ReachabilityReportSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_reachability_report(
    files: &[CallGraphFile],
    summary: &ReachabilityReportSummary,
    policy: &ReachabilityReportPolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!(
                "callable_definitions\t{}",
                summary.callable_definition_count
            );
            println!("roots\t{}", summary.root_count);
            println!("unreachable\t{}", summary.unreachable.len());
            if policy.fail_on_unreachable {
                println!("policy\tfail_on_unreachable=true\tpassed={}", policy.passed);
            }
            for (file_index, item) in &summary.unreachable {
                println!(
                    "\t{}\t{}\t{}\t{}..{}\tinbound_edges={}",
                    safe_text!(files[*file_index].path.display()),
                    item.category.label(),
                    safe_text!(item.name),
                    item.span.start().get(),
                    item.span.end().get(),
                    item.inbound_edge_count,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "callable_definition_count": summary.callable_definition_count,
                    "root_count": summary.root_count,
                    "unreachable_count": summary.unreachable.len(),
                    "policy": {
                        "fail_on_unreachable": policy.fail_on_unreachable,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "unreachable": summary.unreachable
                        .iter()
                        .map(|(file_index, item)| json!({
                            "file": files[*file_index].path.display().to_string(),
                            "dialect": files[*file_index].dialect.label(),
                            "path": item.path.as_str(),
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "name": item.name.as_str(),
                            "category": item.category.label(),
                            "inbound_edge_count": item.inbound_edge_count,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
