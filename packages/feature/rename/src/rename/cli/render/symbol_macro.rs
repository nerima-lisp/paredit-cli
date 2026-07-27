use anyhow::Result;
use serde_json::json;

use super::super::super::OutputFormat;
use super::super::types::RenameSymbolMacroFileReport;
use super::shared::rename_occurrences_json;
use crate::domain::sexpr::SymbolName;

pub(in crate::presentation::cli::rename) fn print_rename_symbol_macro_report(
    reports: &[RenameSymbolMacroFileReport],
    from: &SymbolName,
    to: &SymbolName,
    write: bool,
    output: OutputFormat,
) -> Result<()> {
    let definition_count = reports
        .iter()
        .map(|report| report.definitions.len())
        .sum::<usize>();
    let reference_count = reports
        .iter()
        .map(|report| report.references.len())
        .sum::<usize>();
    match output {
        OutputFormat::Text => {
            println!("from\t{}", safe_text!(from));
            println!("to\t{}", safe_text!(to));
            println!("write\t{write}");
            println!("definitionCount\t{definition_count}");
            println!("referenceCount\t{reference_count}");
            for report in reports {
                println!(
                    "{}\t{}\tdefinitions={}\treferences={}\tchanged={}\twritten={}",
                    safe_text!(report.path.display()),
                    report.dialect.label(),
                    report.definitions.len(),
                    report.references.len(),
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
                "referenceCount": reference_count,
                "files": reports.iter().map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "definitionCount": report.definitions.len(),
                    "referenceCount": report.references.len(),
                    "changed": report.changed,
                    "written": report.written,
                    "definitions": rename_occurrences_json(&report.definitions),
                    "references": rename_occurrences_json(&report.references),
                    "rewritten": report.rewritten.as_str(),
                })).collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}
