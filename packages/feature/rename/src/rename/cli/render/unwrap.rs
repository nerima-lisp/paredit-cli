use anyhow::Result;
use serde_json::json;

use super::super::super::OutputFormat;
use super::super::args::UnwrapFunctionCallsArgs;
use super::super::types::{CallSitePolicy, UnwrapFunctionCallsFileReport};
use super::shared::unwrap_call_sites_json;

pub(in crate::presentation::cli::rename) fn print_unwrap_function_calls_report(
    reports: &[UnwrapFunctionCallsFileReport],
    args: &UnwrapFunctionCallsArgs,
    policy: &CallSitePolicy,
    output: OutputFormat,
) -> Result<()> {
    let call_count = reports
        .iter()
        .map(|report| report.calls.len())
        .sum::<usize>();
    let skipped_non_unary_wrapper_count = reports
        .iter()
        .map(|report| report.skipped_non_unary_wrapper.len())
        .sum::<usize>();
    let skipped_nested_count = reports
        .iter()
        .map(|report| report.skipped_nested.len())
        .sum::<usize>();
    match output {
        OutputFormat::Text => {
            println!("function\t{}", safe_text!(args.function));
            println!("wrapper\t{}", safe_text!(args.wrapper));
            println!("callCount\t{call_count}");
            println!("skippedNonUnaryWrapperCount\t{skipped_non_unary_wrapper_count}");
            println!("skippedNestedCount\t{skipped_nested_count}");
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
                "function": args.function.as_str(),
                "wrapper": args.wrapper.as_str(),
                "allCalls": args.all_calls,
                "callPaths": args.call_paths.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "write": args.write,
                "callCount": call_count,
                "skippedNonUnaryWrapperCount": skipped_non_unary_wrapper_count,
                "skippedNestedCount": skipped_nested_count,
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
                    "skippedNonUnaryWrapperCount": report.skipped_non_unary_wrapper.len(),
                    "skippedNestedCount": report.skipped_nested.len(),
                    "changed": report.changed,
                    "written": report.written,
                    "calls": unwrap_call_sites_json(&report.calls),
                    "skippedNonUnaryWrapper": unwrap_call_sites_json(&report.skipped_non_unary_wrapper),
                    "skippedNested": unwrap_call_sites_json(&report.skipped_nested),
                    "rewritten": report.rewritten.as_str(),
                })).collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}
