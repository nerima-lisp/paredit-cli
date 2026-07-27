use anyhow::Result;
use serde_json::json;

use super::super::super::OutputFormat;
use super::super::types::RenameFileReport;
use crate::domain::sexpr::SymbolName;

pub(in crate::presentation::cli::rename) fn print_rename_symbols_report(
    reports: &[RenameFileReport],
    from: &SymbolName,
    to: &SymbolName,
    write: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("from\t{}", safe_text!(from));
            println!("to\t{}", safe_text!(to));
            println!("write\t{write}");
            for report in reports {
                println!(
                    "{}\t{}\tcount={}\tchanged={}\twritten={}",
                    safe_text!(report.path.display()),
                    report.dialect.label(),
                    report.occurrences.len(),
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
                "count": reports.iter().map(|report| report.occurrences.len()).sum::<usize>(),
                "files": reports.iter().map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "count": report.occurrences.len(),
                    "changed": report.changed,
                    "written": report.written,
                    "occurrences": report.occurrences.iter().map(|span| json!({
                        "span": {
                            "start": span.start().get(),
                            "end": span.end().get(),
                        },
                    })).collect::<Vec<_>>(),
                    "rewritten": report.rewritten.as_str(),
                })).collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}
