use anyhow::Result;
use serde_json::json;

use super::super::super::OutputFormat;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{SymbolName, SyntaxTree};
use crate::presentation::cli::shared::matching_symbol_occurrences;

pub(in crate::presentation::cli::rename) fn print_rename_plan(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
    output: OutputFormat,
) -> Result<()> {
    let occurrences = matching_symbol_occurrences(tree, from);
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", dialect.label());
            println!("from\t{}", safe_text!(from));
            println!("to\t{}", safe_text!(to));
            println!("count\t{}", occurrences.len());
            for occurrence in occurrences {
                println!(
                    "{}\t{}..{}",
                    safe_text!(occurrence.path),
                    occurrence.span.start().get(),
                    occurrence.span.end().get()
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": dialect.label(),
                "from": from.as_str(),
                "to": to.as_str(),
                "count": occurrences.len(),
                "occurrences": occurrences
                    .into_iter()
                    .map(|occurrence| json!({
                        "path": occurrence.path.to_string(),
                        "span": {
                            "start": occurrence.span.start().get(),
                            "end": occurrence.span.end().get(),
                        },
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}
