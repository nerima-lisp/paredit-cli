use anyhow::Result;
use serde_json::json;

use crate::domain::definition::DefinitionCategory;
use crate::domain::sexpr::SymbolName;
use crate::presentation::cli::args::OutputFormat;
use crate::presentation::cli::call_report::types::CallReportFile;

pub(super) fn print_call_report(
    reports: &[CallReportFile],
    symbol: Option<&SymbolName>,
    include_definitions: bool,
    output: OutputFormat,
) -> Result<()> {
    let total_count = reports
        .iter()
        .map(|report| report.calls.len())
        .sum::<usize>();

    match output {
        OutputFormat::Text => {
            println!(
                "symbol\t{}",
                safe_text!(symbol.map_or("<all>", SymbolName::as_str))
            );
            println!("include_definitions\t{include_definitions}");
            println!("files\t{}", reports.len());
            println!("total_count\t{total_count}");
            for report in reports {
                println!(
                    "{}\t{}\tcount={}",
                    safe_text!(report.path.display()),
                    report.dialect.label(),
                    report.calls.len()
                );
                for call in &report.calls {
                    let category = call
                        .category
                        .map(DefinitionCategory::label)
                        .unwrap_or("call");
                    let enclosing = call.enclosing_definition.as_deref().unwrap_or("<none>");
                    println!(
                        "\t{}\t{}\t{}..{}\targs={}\tcategory={}\tenclosing={}",
                        safe_text!(call.path),
                        safe_text!(call.head),
                        call.span.start().get(),
                        call.span.end().get(),
                        call.argument_count,
                        category,
                        safe_text!(enclosing)
                    );
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "symbol": symbol.map(SymbolName::as_str),
                "includeDefinitions": include_definitions,
                "file_count": reports.len(),
                "total_count": total_count,
                "files": reports
                    .iter()
                    .map(|report| json!({
                        "path": report.path.display().to_string(),
                        "dialect": report.dialect.label(),
                        "count": report.calls.len(),
                        "calls": report
                            .calls
                            .iter()
                            .map(|call| json!({
                                "path": call.path.as_str(),
                                "span": {
                                    "start": call.span.start().get(),
                                    "end": call.span.end().get(),
                                },
                                "head": call.head.as_str(),
                                "argumentCount": call.argument_count,
                                "category": call.category.map(DefinitionCategory::label),
                                "enclosingDefinition": call.enclosing_definition.as_deref(),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}
