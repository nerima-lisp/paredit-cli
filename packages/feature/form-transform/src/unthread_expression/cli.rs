use crate::unthread_expression::usecase::{
    UnthreadExpressionPlan, UnthreadExpressionRequest, UnthreadStyle as ApplicationUnthreadStyle,
    plan_unthread_expression,
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
pub struct UnthreadExpressionArgs {
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
    /// Threading style. Required for custom operators other than -> and ->>.
    #[arg(long, value_enum)]
    style: Option<ThreadStyleArg>,
    /// Expected threading operator. Defaults to the selected form head.
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
/// from another crate.
const fn application_unthread_style(style: ThreadStyleArg) -> ApplicationUnthreadStyle {
    match style {
        ThreadStyleArg::First => ApplicationUnthreadStyle::First,
        ThreadStyleArg::Last => ApplicationUnthreadStyle::Last,
    }
}

pub fn unthread_expression(args: UnthreadExpressionArgs) -> Result<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let selection = resolve_target(&tree, args.path.as_ref(), args.at)?;
    let selected = selection.view();
    let path = args.path.clone();
    let style = args.style.map(application_unthread_style);
    let plan = plan_unthread_expression(UnthreadExpressionRequest {
        input: &input.text,
        tree: &tree,
        dialect,
        path,
        target: selected,
        style,
        operator: args.operator,
    })?;
    let mut written = false;

    if args.write {
        let file = input.file.as_ref().context("--write requires --file")?;
        if plan.changed {
            write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
        }
        written = true;
    }

    print_unthread_expression_plan(&plan, written, args.output)
}

fn print_unthread_expression_plan(
    plan: &UnthreadExpressionPlan,
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
                    "step\t{}\targs={}\tinsertion_index={}\tspan={}..{}\t{}",
                    safe_text!(step.head),
                    step.argument_count,
                    step.insertion_index,
                    step.span.start().get(),
                    step.span.end().get(),
                    safe_text!(step.form)
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
                    "insertion_index": step.insertion_index,
                    "span": {
                        "start": step.span.start().get(),
                        "end": step.span.end().get(),
                    },
                    "form": &step.form,
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
