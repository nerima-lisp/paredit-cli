use crate::thread_expression::usecase::{
    ThreadExpressionPlan, ThreadExpressionRequest, ThreadStyle as ApplicationThreadStyle,
    plan_thread_expression,
};
use anyhow::{Context, Result};
use clap::Args;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::args::ThreadStyleArg;
use paredit_core_cli::safe_text;
use paredit_core_cli::shared::read_input_dialect_and_tree;
use paredit_core_cli::shared::resolve_target;
use paredit_core_cli::shared::write_file_with_rollback;
use paredit_core_syntax::sexpr::Path;
use paredit_core_syntax::sexpr::SymbolName;
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ThreadExpressionArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    dialect: Option<DialectArg>,
    /// Select by child index path, for example 0.2.1.
    #[arg(long, conflicts_with = "at")]
    path: Option<Path>,
    /// Select the smallest expression containing byte offset.
    #[arg(long, conflicts_with = "path")]
    at: Option<usize>,
    /// Threading style: first inserts into the first argument, last into the final argument.
    #[arg(long, value_enum)]
    style: ThreadStyleArg,
    /// Threading operator to emit. Defaults to -> for first and ->> for last.
    #[arg(long)]
    operator: Option<SymbolName>,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

/// Maps the CLI's thread style onto the use case's.
///
/// A free function rather than an inherent method: `ThreadStyleArg` now lives
/// in `paredit-core-cli`, and Rust does not allow an inherent impl on a type
/// from another crate. Keeping the conversion here is also the right side of
/// the boundary - core/cli must not name a use case.
const fn application_style(style: ThreadStyleArg) -> ApplicationThreadStyle {
    match style {
        ThreadStyleArg::First => ApplicationThreadStyle::First,
        ThreadStyleArg::Last => ApplicationThreadStyle::Last,
    }
}

pub fn thread_expression(args: ThreadExpressionArgs) -> Result<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let selection = resolve_target(&tree, args.path.as_ref(), args.at)?;
    let selected = selection.view();
    let path = args.path.clone();
    let style = application_style(args.style);
    let operator = match args.operator {
        Some(operator) => operator,
        None => SymbolName::new(style.default_operator())?,
    };
    let plan = plan_thread_expression(ThreadExpressionRequest {
        input: &input.text,
        tree: &tree,
        dialect,
        path,
        target: selected,
        style,
        operator,
    })?;
    let mut written = false;

    if args.write {
        let file = input.file.as_ref().context("--write requires --file")?;
        if plan.changed {
            write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
        }
        written = true;
    }

    print_thread_expression_plan(&plan, written, args.output)
}

fn print_thread_expression_plan(
    plan: &ThreadExpressionPlan,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            if let Some(path) = &plan.path {
                println!("path\t{}", safe_text!(path));
            }
            println!("style\t{}", plan.style.label());
            println!("operator\t{}", safe_text!(plan.operator));
            println!(
                "span\t{}..{}",
                plan.span.start().get(),
                plan.span.end().get()
            );
            println!("base\t{}", safe_text!(plan.base));
            for step in &plan.steps {
                println!(
                    "step\t{}\targs={}\tthreaded_arg={}\tspan={}..{}\t{}",
                    safe_text!(step.head),
                    step.argument_count,
                    step.threaded_argument_index,
                    step.span.start().get(),
                    step.span.end().get(),
                    safe_text!(step.step)
                );
            }
            println!("replacement\t{}", safe_text!(plan.replacement));
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": plan.dialect.label(),
                "path": plan.path.as_ref().map(ToString::to_string),
                "style": plan.style.label(),
                "operator": plan.operator.as_str(),
                "span": {
                    "start": plan.span.start().get(),
                    "end": plan.span.end().get(),
                },
                "base": &plan.base,
                "steps": plan.steps.iter().map(|step| json!({
                    "head": &step.head,
                    "argument_count": step.argument_count,
                    "threaded_argument_index": step.threaded_argument_index,
                    "span": {
                        "start": step.span.start().get(),
                        "end": step.span.end().get(),
                    },
                    "step": &step.step,
                })).collect::<Vec<_>>(),
                "replacement": &plan.replacement,
                "changed": plan.changed,
                "written": written,
                "rewritten": &plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
