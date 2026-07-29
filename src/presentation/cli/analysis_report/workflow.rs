use std::path::Path;

use paredit_core_cli::{CliError, CliResult};
use serde_json::json;

use paredit_core_cli::report::budget::Budget;

use crate::presentation::cli::args::{AnalyzeArgs, OutputFormat};
use crate::presentation::cli::shared::{
    check_deadline_for_read, read_input_and_dialect, read_input_dialect_and_tree,
};
use paredit_core_syntax::sexpr::SyntaxTree;

use super::args::AgentReportArgs;
use super::render::{
    AgentReportOptions, print_agent_report, print_dialect, print_outline, print_stats,
};

/// Validates that input is a balanced S-expression document.
///
/// Reports every syntax problem `SyntaxTree::find_parse_errors` finds, not
/// only the first — a file with three unrelated syntax errors used to cost
/// three round trips: fix one, re-run, hit the next. The singular `error`
/// field is unchanged, still the first problem found, so an existing
/// consumer reading only that field sees exactly what it always has; `errors`
/// is the new, additive way to see all of them at once.
pub(in crate::presentation::cli) fn check(args: AnalyzeArgs) -> CliResult<()> {
    check_deadline_for_read(args.file.as_deref())?;
    let (input, dialect) = read_input_and_dialect(args.file, args.dialect)?;
    let mut errors = SyntaxTree::find_parse_errors(&input.text, dialect);

    match args.output {
        OutputFormat::Text => {
            if errors.is_empty() {
                println!("ok");
                return Ok(());
            }
            for error in &errors {
                println!("error: {error}");
            }
            Err(CliError::Parse {
                origin: "input".to_owned(),
                location: "input is not a balanced S-expression document".to_owned(),
                source: errors.remove(0),
            })
        }
        OutputFormat::Json => {
            let file = input.file.as_deref().map(|path| path.display().to_string());
            let errors_json: Vec<_> = errors
                .iter()
                .map(|error| {
                    json!({
                        "message": error.to_string(),
                        "offset": error.position(),
                    })
                })
                .collect();
            let report = json!({
                "schema_version": 1,
                "status": if errors.is_empty() { "ok" } else { "error" },
                "file": file,
                "dialect": dialect.label(),
                "error": errors.first().map(ToString::to_string),
                "errors": errors_json,
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            if errors.is_empty() {
                Ok(())
            } else {
                Err(CliError::Parse {
                    origin: "input".to_owned(),
                    location: "input is not a balanced S-expression document".to_owned(),
                    source: errors.remove(0),
                })
            }
        }
    }
}

pub(in crate::presentation::cli) fn dialect(args: AnalyzeArgs) -> CliResult<()> {
    let (_, dialect, _) = read_input_dialect_and_tree(args.file, args.dialect)?;
    print_dialect(dialect, args.output)
}

pub(in crate::presentation::cli) fn stats(args: AnalyzeArgs) -> CliResult<()> {
    let (_, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    print_stats(&tree, dialect, args.output)
}

pub(in crate::presentation::cli) fn agent_report(args: AgentReportArgs) -> CliResult<()> {
    let runtime = paredit_core_cli::runtime::current();
    let analyze = args.analyze;

    // A flag beats the configuration, which beats the built-in default. The
    // same order every other setting follows.
    let verbosity = args.verbosity.map_or(runtime.verbosity, Into::into);
    let budget = Budget(args.max_tokens.unwrap_or(runtime.max_tokens));

    let previous = args
        .since
        .as_deref()
        .map(read_previous_report)
        .transpose()?;

    let (input, dialect, tree) = read_input_dialect_and_tree(analyze.file, analyze.dialect)?;
    print_agent_report(
        &tree,
        dialect,
        analyze.output,
        &AgentReportOptions {
            verbosity,
            budget,
            previous: previous.as_ref(),
            file: input.file.as_deref(),
        },
    )
}

/// Reads a previous `agent-report --output json`.
///
/// Refused rather than ignored when it is not one: comparing against a file
/// that happens to be JSON but is not this report would produce a delta that
/// says everything changed, which is indistinguishable from a real answer.
fn read_previous_report(path: &Path) -> CliResult<serde_json::Value> {
    let text = std::fs::read_to_string(path).map_err(CliError::io(format!(
        "failed to read --since {}",
        path.display()
    )))?;
    let report: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| CliError::Json {
            context: format!("--since {} is not JSON", path.display()),
            source,
        })?;

    if report.get("outline").is_none() || report.get("metrics").is_none() {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputUnparsable,
            format!(
                "--since {} is not an `inspect agent-report --output json` report \
                 (no `outline` or `metrics`)",
                path.display()
            ),
        )
        .into());
    }
    Ok(report)
}

pub(in crate::presentation::cli) fn outline(args: AnalyzeArgs) -> CliResult<()> {
    let (_, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    print_outline(&tree, dialect, args.output)
}
