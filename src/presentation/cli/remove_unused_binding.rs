use super::*;
use crate::application::usecase::remove_unused_binding::{
    RemoveUnusedBindingPlan, RemoveUnusedBindingRequest, plan_remove_unused_binding,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;

#[derive(Debug, Args)]
pub(super) struct RemoveUnusedBindingArgs {
    #[arg(short, long)]
    file: Option<PathBuf>,
    #[arg(long)]
    dialect: Option<DialectArg>,
    #[arg(long)]
    path: Option<Path>,
    #[arg(long)]
    at: Option<usize>,
    #[arg(long)]
    name: Option<SymbolName>,
    #[arg(long)]
    all_bindings: bool,
    #[arg(long)]
    allow_drop_value: bool,
    #[arg(long)]
    write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub(super) fn remove_unused_binding(args: RemoveUnusedBindingArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }
    if args.name.is_some() && args.all_bindings {
        anyhow::bail!("remove-unused-binding accepts either --name or --all-bindings, not both");
    }
    if args.name.is_none() && !args.all_bindings {
        anyhow::bail!("remove-unused-binding requires --name or --all-bindings");
    }
    if args.write && !args.allow_drop_value {
        anyhow::bail!(
            "remove-unused-binding drops the binding value expression; pass --allow-drop-value to write"
        );
    }

    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let selection = resolve_target(&tree, args.path.as_ref(), args.at)?;
    let plan = plan_remove_unused_binding(RemoveUnusedBindingRequest {
        input: &input.text,
        dialect,
        path: args.path,
        target: selection.view(),
        name: args.name.as_ref(),
        all_bindings: args.all_bindings,
        allow_drop_value: args.allow_drop_value,
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_remove_unused_binding_plan(&plan, written, args.output)
}

fn print_remove_unused_binding_plan(
    plan: &RemoveUnusedBindingPlan,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            if let Some(path) = &plan.path {
                println!("path\t{}", safe_text!(path));
            }
            println!("form\t{}", safe_text!(plan.form));
            println!(
                "form_span\t{}..{}",
                plan.form_span.start().get(),
                plan.form_span.end().get()
            );
            if let Some(binding_name) = &plan.binding_name {
                println!("binding_name\t{}", safe_text!(binding_name));
            }
            if let Some(binding_span) = plan.binding_span {
                println!(
                    "binding_span\t{}..{}",
                    binding_span.start().get(),
                    binding_span.end().get()
                );
            }
            if let Some(binding_value) = &plan.binding_value {
                println!("binding_value\t{}", safe_text!(binding_value));
            }
            if let Some(reference_count) = plan.reference_count {
                println!("reference_count\t{reference_count}");
            }
            println!("binding_count\t{}", plan.bindings.len());
            for binding in &plan.bindings {
                println!(
                    "\t{}\tbinding_span={}..{}\treferences={}",
                    safe_text!(binding.binding_name),
                    binding.binding_span.start().get(),
                    binding.binding_span.end().get(),
                    binding.reference_count
                );
            }
            println!(
                "dropped_value_requires_review\t{}",
                plan.dropped_value_requires_review
            );
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
                "form": plan.form.as_str(),
                "form_span": {
                    "start": plan.form_span.start().get(),
                    "end": plan.form_span.end().get(),
                },
                "binding_name": plan.binding_name.as_deref(),
                "binding_span": plan.binding_span.map(|span| json!({
                    "start": span.start().get(),
                    "end": span.end().get(),
                })),
                "binding_value": plan.binding_value.as_deref(),
                "reference_count": plan.reference_count,
                "binding_count": plan.bindings.len(),
                "bindings": plan
                    .bindings
                    .iter()
                    .map(|binding| json!({
                        "binding_name": binding.binding_name.as_str(),
                        "binding_span": {
                            "start": binding.binding_span.start().get(),
                            "end": binding.binding_span.end().get(),
                        },
                        "binding_value": binding.binding_value.as_str(),
                        "reference_count": binding.reference_count,
                    }))
                    .collect::<Vec<_>>(),
                "dropped_value_requires_review": plan.dropped_value_requires_review,
                "replacement": plan.replacement.as_str(),
                "changed": plan.changed,
                "written": written,
                "rewritten": plan.rewritten.as_str(),
            }))?
        ),
    }
    Ok(())
}
