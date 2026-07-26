use anyhow::Result;
use serde_json::json;

use crate::application::usecase::form_report::types::FormReport;
use crate::domain::sexpr::Delimiter;
use crate::presentation::cli::args::OutputFormat;

pub(super) fn print_form_report(report: &FormReport, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", report.dialect.label());
            if let Some(path) = &report.path {
                println!("path\t{}", safe_text!(path));
            }
            println!(
                "span\t{}..{}",
                report.span.start().get(),
                report.span.end().get()
            );
            println!("kind\t{}", report.kind.label());
            println!(
                "delimiter\t{}",
                report.delimiter.map(delimiter_label).unwrap_or("<none>")
            );
            println!(
                "head\t{}",
                safe_text!(report.head.as_deref().unwrap_or("<none>"))
            );
            println!("definition_like\t{}", report.definition_like);
            println!("child_count\t{}", report.child_count);
            println!("atom_count\t{}", report.atom_count);
            println!("list_count\t{}", report.list_count);
            println!("max_depth\t{}", report.max_depth);
            for symbol in &report.symbols {
                println!(
                    "symbol\t{}\tcount={}\tfirst_span={}..{}",
                    safe_text!(symbol.symbol),
                    symbol.count,
                    symbol.first_span.start().get(),
                    symbol.first_span.end().get()
                );
            }
            if let Some(source) = &report.source {
                println!("source\t{}", safe_text!(source));
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": report.dialect.label(),
                "path": report.path.as_ref().map(ToString::to_string),
                "span": {
                    "start": report.span.start().get(),
                    "end": report.span.end().get(),
                },
                "kind": report.kind.label(),
                "delimiter": report.delimiter.map(delimiter_label),
                "head": report.head.as_deref(),
                "definitionLike": report.definition_like,
                "childCount": report.child_count,
                "atomCount": report.atom_count,
                "listCount": report.list_count,
                "maxDepth": report.max_depth,
                "symbols": report
                    .symbols
                    .iter()
                    .map(|symbol| json!({
                        "symbol": symbol.symbol.as_str(),
                        "count": symbol.count,
                        "firstSpan": {
                            "start": symbol.first_span.start().get(),
                            "end": symbol.first_span.end().get(),
                        },
                    }))
                    .collect::<Vec<_>>(),
                "source": report.source.as_deref(),
            }))?
        ),
    }

    Ok(())
}

const fn delimiter_label(delimiter: Delimiter) -> &'static str {
    match delimiter {
        Delimiter::Paren => "paren",
        Delimiter::Bracket => "bracket",
        Delimiter::Brace => "brace",
    }
}
