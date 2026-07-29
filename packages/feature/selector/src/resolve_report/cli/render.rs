use paredit_core_cli::CliResult;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use paredit_core_syntax::selector::Capture;

use crate::resolve_report::usecase::{ResolveReport, ResolvedMatch};

pub fn print_resolve_report(report: &ResolveReport, output: OutputFormat) -> CliResult<()> {
    match output {
        OutputFormat::Text => print_text(report),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": report.dialect.label(),
                "selector": report.selector,
                "matchCount": report.matches.len(),
                "matches": report
                    .matches
                    .iter()
                    .map(match_json)
                    .collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}

/// Tab-separated so the output stays greppable and cuttable, with captures on
/// indented continuation lines. The header line carries the match count first
/// because that is the number a caller checks before doing anything else.
fn print_text(report: &ResolveReport) {
    println!(
        "matches\t{}\tselector\t{}",
        report.matches.len(),
        safe_text!(report.selector)
    );
    for found in &report.matches {
        println!(
            "{}\t{}-{}\t{}\t{}\t{}",
            safe_text!(found.path),
            found.start,
            found.end,
            found.kind,
            found.id.as_deref().unwrap_or("-"),
            safe_text!(found.preview)
        );
        for capture in &found.captures {
            println!(
                "\t?{}\t{}\t{}",
                safe_text!(capture.name),
                capture
                    .paths
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
                safe_text!(capture.text)
            );
        }
    }
}

fn match_json(found: &ResolvedMatch) -> serde_json::Value {
    json!({
        "path": found.path.to_string(),
        "id": found.id,
        "kind": found.kind,
        "head": found.head,
        "formCount": found.form_count,
        "span": {
            "start": found.span.start().get(),
            "end": found.span.end().get(),
        },
        "start": { "line": found.start.line(), "column": found.start.column() },
        "end": { "line": found.end.line(), "column": found.end.column() },
        "preview": found.preview,
        "captures": found
            .captures
            .iter()
            .map(capture_json)
            .collect::<Vec<_>>(),
    })
}

fn capture_json(capture: &Capture) -> serde_json::Value {
    json!({
        "name": capture.name,
        "paths": capture
            .paths
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        "span": capture.span.map(|span| json!({
            "start": span.start().get(),
            "end": span.end().get(),
        })),
        "text": capture.text,
    })
}
