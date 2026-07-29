use crate::introduce_let::usecase::{IntroduceLetPlan, IntroduceLetRequest, plan_introduce_let};
use clap::Args;
use paredit_core_cli::CliResult;
use paredit_core_cli::args::CompactSelectorArgs;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use paredit_core_cli::shared::read_input_dialect_and_tree;
use paredit_core_cli::shared::require_output_file;
use paredit_core_cli::shared::resolve_compact_target;
use paredit_core_cli::shared::write_file_with_rollback;
use paredit_core_syntax::sexpr::SymbolName;
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct IntroduceLetArgs {
    #[arg(short, long)]
    file: Option<PathBuf>,
    #[arg(long)]
    dialect: Option<DialectArg>,
    #[command(flatten)]
    selector: CompactSelectorArgs,
    #[arg(long)]
    name: SymbolName,
    #[arg(long)]
    all_occurrences: bool,
    #[arg(long)]
    write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub fn introduce_let(args: IntroduceLetArgs) -> CliResult<()> {
    if args.write && args.file.is_none() {
        return Err(paredit_core_cli::ArgumentError::WriteRequiresFile.into());
    }

    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let target = resolve_compact_target(&tree, dialect, &args.selector, "refactor introduce-let")?;
    let selection = tree.select_path(&target.path)?;
    let enclosing_span = selection.enclosing_list_span()?;
    let plan = plan_introduce_let(IntroduceLetRequest {
        input: &input.text,
        dialect,
        path: Some(target.path),
        target: selection.view(),
        enclosing_span,
        name: args.name,
        all_occurrences: args.all_occurrences,
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_introduce_let_plan(&plan, written, args.output)
}

fn print_introduce_let_plan(
    plan: &IntroduceLetPlan,
    written: bool,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            if let Some(path) = &plan.path {
                println!("path\t{}", safe_text!(path));
            }
            println!(
                "selected_span\t{}..{}",
                plan.selected_span.start().get(),
                plan.selected_span.end().get()
            );
            println!(
                "enclosing_span\t{}..{}",
                plan.enclosing_span.start().get(),
                plan.enclosing_span.end().get()
            );
            println!("name\t{}", safe_text!(plan.name));
            println!("binding_value\t{}", safe_text!(plan.binding_value));
            println!("occurrence_count\t{}", plan.occurrence_spans.len());
            for span in &plan.occurrence_spans {
                println!(
                    "occurrence_span\t{}..{}",
                    span.start().get(),
                    span.end().get()
                );
            }
            println!(
                "skipped_shadowed_occurrence_count\t{}",
                plan.skipped_shadowed_occurrence_spans.len()
            );
            for span in &plan.skipped_shadowed_occurrence_spans {
                println!(
                    "skipped_shadowed_occurrence_span\t{}..{}",
                    span.start().get(),
                    span.end().get()
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
                "selected_span": {
                    "start": plan.selected_span.start().get(),
                    "end": plan.selected_span.end().get(),
                },
                "enclosing_span": {
                    "start": plan.enclosing_span.start().get(),
                    "end": plan.enclosing_span.end().get(),
                },
                "name": plan.name.as_str(),
                "binding_value": plan.binding_value,
                "occurrence_spans": plan.occurrence_spans.iter().map(|span| {
                    json!({
                        "start": span.start().get(),
                        "end": span.end().get(),
                    })
                }).collect::<Vec<_>>(),
                "occurrence_count": plan.occurrence_spans.len(),
                "skipped_shadowed_occurrence_spans": plan.skipped_shadowed_occurrence_spans.iter().map(|span| {
                    json!({
                        "start": span.start().get(),
                        "end": span.end().get(),
                    })
                }).collect::<Vec<_>>(),
                "skipped_shadowed_occurrence_count": plan.skipped_shadowed_occurrence_spans.len(),
                "replacement": plan.replacement,
                "changed": plan.changed,
                "written": written,
                "rewritten": plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
