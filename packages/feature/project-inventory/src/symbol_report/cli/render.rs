use paredit_core_cli::CliResult;
use paredit_core_cli::safe_text;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::shared::matching_symbol_occurrences;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{SymbolName, SyntaxTree};

use super::types::SymbolReportFile;

pub fn print_symbol_occurrences(
    tree: &SyntaxTree,
    dialect: Dialect,
    symbol: &SymbolName,
    output: OutputFormat,
) -> CliResult<()> {
    let occurrences = matching_symbol_occurrences(tree, symbol);
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", dialect.label());
            for occurrence in occurrences {
                println!(
                    "{}\t{}..{}\t{}",
                    safe_text!(occurrence.path),
                    occurrence.span.start().get(),
                    occurrence.span.end().get(),
                    safe_text!(occurrence.text)
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": dialect.label(),
                "symbol": symbol.as_str(),
                "occurrences": occurrences
                    .into_iter()
                    .map(|occurrence| json!({
                        "path": occurrence.path.to_string(),
                        "span": {
                            "start": occurrence.span.start().get(),
                            "end": occurrence.span.end().get(),
                        },
                        "text": occurrence.text,
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}

pub fn print_symbol_report(
    reports: &[SymbolReportFile],
    symbol: &SymbolName,
    output: OutputFormat,
) -> CliResult<()> {
    let total_count = reports
        .iter()
        .map(|report| report.occurrences.len())
        .sum::<usize>();

    match output {
        OutputFormat::Text => {
            println!("symbol\t{}", safe_text!(symbol));
            println!("files\t{}", reports.len());
            println!("total_count\t{total_count}");
            for report in reports {
                println!(
                    "{}\t{}\tcount={}",
                    safe_text!(report.path.display()),
                    report.dialect.label(),
                    report.occurrences.len()
                );
                for occurrence in &report.occurrences {
                    let context = occurrence
                        .context
                        .as_ref()
                        .map(|context| context.path.as_str())
                        .unwrap_or("<none>");
                    println!(
                        "\t{}\t{}..{}\tcontext={}",
                        safe_text!(occurrence.path),
                        occurrence.span.start().get(),
                        occurrence.span.end().get(),
                        safe_text!(context)
                    );
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "symbol": symbol.as_str(),
                "file_count": reports.len(),
                "total_count": total_count,
                "files": reports
                    .iter()
                    .map(|report| json!({
                        "path": report.path.display().to_string(),
                        "dialect": report.dialect.label(),
                        "count": report.occurrences.len(),
                        "occurrences": report
                            .occurrences
                            .iter()
                            .map(|occurrence| json!({
                                "path": occurrence.path.as_str(),
                                "span": {
                                    "start": occurrence.span.start().get(),
                                    "end": occurrence.span.end().get(),
                                },
                                "context": occurrence.context.as_ref().map(|context| json!({
                                    "path": context.path.as_str(),
                                    "span": {
                                        "start": context.span.start().get(),
                                        "end": context.span.end().get(),
                                    },
                                    "head": context.head.as_deref(),
                                    "definitionLike": context.definition_like,
                                })),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}
