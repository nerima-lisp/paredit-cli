use anyhow::Result;
use serde_json::json;

use super::super::super::OutputFormat;
use super::super::args::ReplaceFunctionCallsArgs;
use super::super::types::{CallSitePolicy, ReplaceFunctionCallsFileReport};
use super::shared::replace_call_sites_json;

pub(in crate::presentation::cli::rename) fn print_replace_function_calls_report(
    reports: &[ReplaceFunctionCallsFileReport],
    args: &ReplaceFunctionCallsArgs,
    policy: &CallSitePolicy,
    output: OutputFormat,
) -> Result<()> {
    let call_count = reports
        .iter()
        .map(|report| report.calls.len())
        .sum::<usize>();
    match output {
        OutputFormat::Text => {
            println!("from\t{}", safe_text!(args.from));
            println!("to\t{}", safe_text!(args.to));
            println!("callCount\t{call_count}");
            println!("passed\t{}", policy.passed);
            for report in reports {
                println!(
                    "{}\t{}\tcalls={}\tchanged={}\twritten={}",
                    safe_text!(report.path.display()),
                    report.dialect.label(),
                    report.calls.len(),
                    report.changed,
                    report.written
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "from": args.from.as_str(),
                "to": args.to.as_str(),
                "allCalls": args.all_calls,
                "callPaths": args.call_paths.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "write": args.write,
                "callCount": call_count,
                "policy": {
                    "failOnNoChange": policy.fail_on_no_change,
                    "requireCalls": policy.require_calls,
                    "passed": policy.passed,
                    "violations": policy.violations,
                },
                "files": reports.iter().map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "callCount": report.calls.len(),
                    "changed": report.changed,
                    "written": report.written,
                    "calls": replace_call_sites_json(&report.calls),
                    "rewritten": report.rewritten.as_str(),
                })).collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}
