//! Shared renderer for the callable rename family (rename-function,
//! rename-macrolet, rename-local-function): identical report shape,
//! identical text/JSON output keys.
use anyhow::Result;
use serde_json::json;

use super::super::super::OutputFormat;
use super::super::types::CallableRenameFileReport;
use super::shared::rename_occurrences_json;
use crate::domain::sexpr::SymbolName;

pub(in crate::presentation::cli::rename) fn print_callable_rename_report(
    reports: &[CallableRenameFileReport],
    from: &SymbolName,
    to: &SymbolName,
    write: bool,
    output: OutputFormat,
) -> Result<()> {
    let definition_count = reports
        .iter()
        .map(|report| report.definitions.len())
        .sum::<usize>();
    let call_count = reports
        .iter()
        .map(|report| report.calls.len())
        .sum::<usize>();
    match output {
        OutputFormat::Text => {
            println!("from\t{}", safe_text!(from));
            println!("to\t{}", safe_text!(to));
            println!("write\t{write}");
            println!("definitionCount\t{definition_count}");
            println!("callCount\t{call_count}");
            for report in reports {
                println!(
                    "{}\t{}\tdefinitions={}\tcalls={}\tchanged={}\twritten={}",
                    safe_text!(report.path.display()),
                    report.dialect.label(),
                    report.definitions.len(),
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
                "from": from.as_str(),
                "to": to.as_str(),
                "write": write,
                "definitionCount": definition_count,
                "callCount": call_count,
                "files": reports.iter().map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "definitionCount": report.definitions.len(),
                    "callCount": report.calls.len(),
                    "changed": report.changed,
                    "written": report.written,
                    "definitions": rename_occurrences_json(&report.definitions),
                    "calls": rename_occurrences_json(&report.calls),
                    "rewritten": report.rewritten.as_str(),
                })).collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}
