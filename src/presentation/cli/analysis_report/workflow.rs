use std::path::Path;

use anyhow::{Context, Result};
use serde_json::json;

use paredit_core_cli::report::budget::Budget;

use crate::domain::sexpr::SyntaxTree;
use crate::presentation::cli::args::{AnalyzeArgs, OutputFormat};
use crate::presentation::cli::shared::{read_input_and_dialect, read_input_dialect_and_tree};

use super::args::AgentReportArgs;
use super::render::{
    AgentReportOptions, print_agent_report, print_dialect, print_outline, print_stats,
};

pub(in crate::presentation::cli) fn check(args: AnalyzeArgs) -> Result<()> {
    match args.output {
        OutputFormat::Text => {
            read_input_dialect_and_tree(args.file, args.dialect)?;
            println!("ok");
            Ok(())
        }
        OutputFormat::Json => {
            let (input, dialect) = read_input_and_dialect(args.file, args.dialect)?;
            let file = input.file.as_deref().map(|path| path.display().to_string());
            let parse_error = SyntaxTree::parse_with_dialect(&input.text, dialect).err();
            let report = json!({
                "schema_version": 1,
                "status": if parse_error.is_none() { "ok" } else { "error" },
                "file": file,
                "dialect": dialect.label(),
                "error": parse_error.as_ref().map(ToString::to_string),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            match parse_error {
                None => Ok(()),
                Some(error) => Err(error).context("input is not a balanced S-expression document"),
            }
        }
    }
}

pub(in crate::presentation::cli) fn dialect(args: AnalyzeArgs) -> Result<()> {
    let (_, dialect, _) = read_input_dialect_and_tree(args.file, args.dialect)?;
    print_dialect(dialect, args.output)
}

pub(in crate::presentation::cli) fn stats(args: AnalyzeArgs) -> Result<()> {
    let (_, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    print_stats(&tree, dialect, args.output)
}

pub(in crate::presentation::cli) fn agent_report(args: AgentReportArgs) -> Result<()> {
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
fn read_previous_report(path: &Path) -> Result<serde_json::Value> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read --since {}", path.display()))?;
    let report: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("--since {} is not JSON", path.display()))?;

    if report.get("outline").is_none() || report.get("metrics").is_none() {
        return Err(anyhow::anyhow!(
            "--since {} is not an `inspect agent-report --output json` report \
             (no `outline` or `metrics`)",
            path.display()
        ));
    }
    Ok(report)
}

pub(in crate::presentation::cli) fn outline(args: AnalyzeArgs) -> Result<()> {
    let (_, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    print_outline(&tree, dialect, args.output)
}
