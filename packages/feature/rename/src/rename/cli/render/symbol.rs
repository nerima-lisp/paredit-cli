use anyhow::Result;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use serde_json::json;

use paredit_core_cli::shared::matching_symbol_occurrences;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{SymbolName, SyntaxTree};

pub fn print_rename_plan(
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
