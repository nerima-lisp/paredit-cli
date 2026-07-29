use paredit_core_cli::CliResult;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SourceContext};

pub fn print_context_report(
    source: &str,
    dialect: Dialect,
    context: &SourceContext,
    output: OutputFormat,
) -> CliResult<()> {
    let (line, column) = line_and_column(source, context.offset);
    let stack = context.delimiter_stack.iter().collect::<String>();

    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", dialect.label());
            println!("offset\t{}", context.offset);
            println!("line\t{line}");
            println!("column\t{column}");
            println!("kind\t{}", context.kind.as_str());
            println!(
                "structurally_inert\t{}",
                context.kind.is_structurally_inert()
            );
            println!("path\t{}", optional(context.path.as_ref()));
            println!("span\t{}", optional_span(context.span));
            println!(
                "enclosing_list_path\t{}",
                optional(context.enclosing_list_path.as_ref())
            );
            println!(
                "enclosing_list_span\t{}",
                optional_span(context.enclosing_list_span)
            );
            println!(
                "enclosing_head\t{}",
                safe_text!(context.enclosing_head.as_deref().unwrap_or("<none>"))
            );
            println!("depth\t{}", context.depth);
            println!("delimiter_stack\t{}", safe_text!(stack));
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": dialect.label(),
                "offset": context.offset,
                "line": line,
                "column": column,
                "kind": context.kind.as_str(),
                "structurallyInert": context.kind.is_structurally_inert(),
                "path": context.path.as_ref().map(ToString::to_string),
                "span": context.span.map(span_json),
                "enclosingListPath": context.enclosing_list_path.as_ref().map(ToString::to_string),
                "enclosingListSpan": context.enclosing_list_span.map(span_json),
                "enclosingHead": context.enclosing_head.as_deref(),
                "depth": context.depth,
                "delimiterStack": stack,
            }))?
        ),
    }
    Ok(())
}

fn optional<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "<none>".to_owned(), |value| value.to_string())
}

fn optional_span(span: Option<ByteSpan>) -> String {
    optional(span.map(|span| format!("{}..{}", span.start().get(), span.end().get())))
}

fn span_json(span: ByteSpan) -> serde_json::Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// One-based line and zero-based column, counted in characters.
fn line_and_column(source: &str, offset: usize) -> (usize, usize) {
    let before = source.get(..offset.min(source.len())).unwrap_or(source);
    let line = 1 + before.bytes().filter(|byte| *byte == b'\n').count();
    let column = before
        .rsplit_once('\n')
        .map_or(before, |(_, line)| line)
        .chars()
        .count();
    (line, column)
}
