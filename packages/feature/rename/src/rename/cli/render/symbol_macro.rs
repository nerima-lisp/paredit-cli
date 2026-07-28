use anyhow::Result;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use serde_json::json;

use super::super::types::RenameSymbolMacroFileReport;
use super::shared::rename_occurrences_json;
use paredit_core_syntax::sexpr::SymbolName;

pub fn print_rename_symbol_macro_report(
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
