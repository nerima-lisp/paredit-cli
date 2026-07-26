use super::*;
use crate::application::usecase::inline_let::{InlineLetPlan, InlineLetRequest, plan_inline_let};
use crate::presentation::cli::shared::read_input_dialect_and_tree;

#[derive(Debug, Args)]
pub(super) struct InlineLetArgs {
    #[arg(short, long)]
    file: Option<PathBuf>,
    #[arg(long)]
    dialect: Option<DialectArg>,
    #[arg(long)]
    path: Option<Path>,
    #[arg(long)]
    at: Option<usize>,
    #[arg(long)]
    allow_duplicate_evaluation: bool,
    #[arg(long)]
    write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub(super) fn inline_let(args: InlineLetArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }

    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let selection = resolve_target(&tree, args.path.as_ref(), args.at)?;
    let plan = plan_inline_let(InlineLetRequest {
        input: &input.text,
        dialect,
        path: args.path,
        target: selection.view(),
        allow_duplicate_evaluation: args.allow_duplicate_evaluation,
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_inline_let_plan(&plan, written, args.output)
}

fn print_inline_let_plan(plan: &InlineLetPlan, written: bool, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            if let Some(path) = &plan.path {
                println!("path\t{}", safe_text!(path));
            }
            println!(
                "let_span\t{}..{}",
                plan.let_span.start().get(),
                plan.let_span.end().get()
            );
            println!("binding_name\t{}", safe_text!(plan.binding_name));
            println!("binding_value\t{}", safe_text!(plan.binding_value));
            println!("body_count\t{}", plan.body_count);
            println!("reference_count\t{}", plan.reference_count);
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
                "let_span": {
                    "start": plan.let_span.start().get(),
                    "end": plan.let_span.end().get(),
                },
                "binding_name": plan.binding_name.as_str(),
                "binding_value": plan.binding_value,
                "body_count": plan.body_count,
                "reference_count": plan.reference_count,
                "replacement": plan.replacement,
                "changed": plan.changed,
                "written": written,
                "rewritten": plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
