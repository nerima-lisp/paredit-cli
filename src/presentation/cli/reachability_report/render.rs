use anyhow::Result;
use serde_json::json;

use crate::application::usecase::call_graph_report::CallGraphFile;
use crate::application::usecase::reachability_report::{
    ReachabilityReportPolicy, ReachabilityReportSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_reachability_report(
    files: &[CallGraphFile],
    summary: &ReachabilityReportSummary,
    policy: &ReachabilityReportPolicy,
    output: OutputFormat,
) -> Result<()> {
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
