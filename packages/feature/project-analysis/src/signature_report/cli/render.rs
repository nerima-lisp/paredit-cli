use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::json;

use crate::application::usecase::signature_report::{
    SignatureCallStatus, SignatureReportFile, SignatureReportPolicy,
};
use crate::domain::sexpr::SymbolName;
use crate::presentation::cli::OutputFormat;

fn format_arity((min, max): (usize, Option<usize>)) -> String {
    match max {
        Some(max) if max == min => min.to_string(),
        Some(max) => format!("{min}..{max}"),
        None => format!("{min}.."),
    }
}

pub(super) fn print_signature_report(
    reports: &[SignatureReportFile],
    symbol: Option<&SymbolName>,
    policy: &SignatureReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    let mut by_status = BTreeMap::<SignatureCallStatus, usize>::new();
    for item in reports.iter().flat_map(|report| &report.calls) {
        *by_status.entry(item.status).or_default() += 1;
    }

    match output {
        OutputFormat::Text => print_text_report(reports, symbol, policy, &by_status),
        OutputFormat::Json => print_json_report(reports, symbol, policy, &by_status)?,
    }

    Ok(())
}

fn print_text_report(
    reports: &[SignatureReportFile],
    symbol: Option<&SymbolName>,
    policy: &SignatureReportPolicy,
    by_status: &BTreeMap<SignatureCallStatus, usize>,
) {
    println!(
        "symbol\t{}",
        safe_text!(symbol.map_or("<all>", SymbolName::as_str))
    );
    println!("files\t{}", reports.len());
    println!("definition_count\t{}", policy.definition_count);
    println!("call_count\t{}", policy.call_count);
    println!("mismatch_count\t{}", policy.mismatch_count);
    println!("policy_passed\t{}", policy.passed);
    for violation in &policy.violations {
        println!("policy_violation\t{}", safe_text!(violation));
    }
    for (status, count) in by_status {
        println!("status\t{}\t{count}", status.label());
    }
    for report in reports {
        println!(
            "{}\t{}\tdefinitions={}\tcalls={}",
            safe_text!(report.path.display()),
            report.dialect.label(),
            report.definitions.len(),
            report.calls.len()
        );
        for definition in &report.definitions {
            println!(
                "\tdefinition\t{}\t{}\t{}..{}\tparams={}",
                safe_text!(definition.path),
                safe_text!(definition.name.as_deref().unwrap_or("")),
                definition.span.start().get(),
                definition.span.end().get(),
                definition
                    .parameter_count
                    .map(|count| count.to_string())
                    .unwrap_or_default(),
            );
        }
        for item in &report.calls {
            let expected = item
                .expected_parameter_arity
                .map(format_arity)
                .unwrap_or_default();
            let enclosing = item
                .call
                .enclosing_definition
                .as_deref()
                .unwrap_or("<none>");
            println!(
                "\tcall\t{}\t{}\t{}..{}\targs={}\texpected={}\tstatus={}\tenclosing={}",
                safe_text!(item.call.path),
                safe_text!(item.call.head),
                item.call.span.start().get(),
                item.call.span.end().get(),
                item.call.argument_count,
                expected,
                item.status.label(),
                safe_text!(enclosing),
            );
        }
    }
}

fn print_json_report(
    reports: &[SignatureReportFile],
    symbol: Option<&SymbolName>,
    policy: &SignatureReportPolicy,
    by_status: &BTreeMap<SignatureCallStatus, usize>,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "symbol": symbol.map(SymbolName::as_str),
            "file_count": reports.len(),
            "definition_count": policy.definition_count,
            "call_count": policy.call_count,
            "mismatch_count": policy.mismatch_count,
            "policy": {
                "fail_on_mismatch": policy.fail_on_mismatch,
                "require_definitions": policy.require_definitions,
                "require_calls": policy.require_calls,
                "passed": policy.passed,
                "violations": policy.violations,
            },
            "by_status": by_status
                .iter()
                .map(|(status, count)| json!({
                    "status": status.label(),
                    "count": count,
                }))
                .collect::<Vec<_>>(),
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "definition_count": report.definitions.len(),
                    "call_count": report.calls.len(),
                    "definitions": report
                    .definitions
                    .iter()
                    .map(|definition| json!({
                            "path": definition.path.to_string(),
                            "span": {
                                "start": definition.span.start().get(),
                                "end": definition.span.end().get(),
                            },
                            "head": definition.head.as_str(),
                            "name": definition.name.as_deref(),
                            "category": definition.category.label(),
                            "parameterCount": definition.parameter_count,
                            "minParameterCount": definition.parameter_arity.map(|(min, _)| min),
                            "maxParameterCount": definition.parameter_arity.and_then(|(_, max)| max),
                        }))
                        .collect::<Vec<_>>(),
                    "calls": report
                        .calls
                        .iter()
                        .map(|item| json!({
                            "path": item.call.path.as_str(),
                            "span": {
                                "start": item.call.span.start().get(),
                                "end": item.call.span.end().get(),
                            },
                            "head": item.call.head.as_str(),
                            "argumentCount": item.call.argument_count,
                            "expectedParameterCount": item
                                .expected_parameter_arity
                                .and_then(|(min, max)| (max == Some(min)).then_some(min)),
                            "minParameterCount": item.expected_parameter_arity.map(|(min, _)| min),
                            "maxParameterCount": item.expected_parameter_arity.and_then(|(_, max)| max),
                            "status": item.status.label(),
                            "enclosingDefinition": item.call.enclosing_definition.as_deref(),
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
