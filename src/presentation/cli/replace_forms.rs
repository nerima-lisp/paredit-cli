use super::*;

use crate::application::usecase::replace_forms::{
    ReplaceFormsPlan, ReplaceFormsRequest, plan_replace_forms,
};
use crate::domain::form_shape::FormShape;

#[derive(Debug, Args)]
pub(super) struct ReplaceFormsArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    dialect: Option<DialectArg>,
    /// Selected form path. Pass repeatedly, for example --path 0 --path 3.
    #[arg(long = "path", required = true)]
    paths: Vec<Path>,
    /// Replacement S-expression text. Must contain exactly one top-level form.
    #[arg(long)]
    with: String,
    /// Require all selected forms to share the same structural shape.
    #[arg(long)]
    require_same_shape: bool,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub(super) fn replace_forms(args: ReplaceFormsArgs) -> Result<()> {
    let ReplaceFormsArgs {
        file,
        dialect,
        paths,
        with,
        require_same_shape,
        write,
        output,
    } = args;

    if write && file.is_none() {
        anyhow::bail!("replace-forms --write requires --file");
    }

    let (input, dialect, tree) = read_input_dialect_and_tree(file.clone(), dialect)?;
    let plan = plan_replace_forms(ReplaceFormsRequest {
        input: &input.text,
        tree: &tree,
        dialect,
        paths,
        replacement: &with,
        require_same_shape,
    })?;
    let written = write && plan.changed;
    if written {
        let Some(path) = file.as_ref() else {
            anyhow::bail!("replace-forms --write requires --file");
        };
        write_file_with_rollback(path.clone(), plan.rewritten.clone())?;
    }

    print_replace_forms_plan(&plan, file.as_ref(), dialect, written, output)
}

fn print_replace_forms_plan(
    plan: &ReplaceFormsPlan,
    path: Option<&PathBuf>,
    dialect: Dialect,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!(
                "file\t{}",
                safe_text!(
                    path.map(|path| path.display().to_string())
                        .unwrap_or_else(|| "<stdin>".to_owned())
                )
            );
            println!("dialect\t{}", dialect.label());
            println!("path_count\t{}", plan.targets.len());
            println!("require_same_shape\t{}", plan.require_same_shape);
            println!(
                "original_shape\t{}",
                plan.original_shape
                    .as_ref()
                    .map(FormShape::as_str)
                    .unwrap_or("")
            );
            println!("replacement_shape\t{}", plan.replacement_shape);
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
            for target in &plan.targets {
                println!(
                    "target\t{}\t{}..{}\t{}",
                    safe_text!(target.form_path),
                    target.span.start().get(),
                    target.span.end().get(),
                    target.shape
                );
            }
            println!("rewritten\n{}", safe_text!(plan.rewritten));
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "file": path.map(|path| path.display().to_string()),
                "dialect": dialect.label(),
                "path_count": plan.targets.len(),
                "require_same_shape": plan.require_same_shape,
                "original_shape": plan.original_shape.as_ref().map(FormShape::as_str),
                "replacement": plan.replacement.as_str(),
                "replacement_shape": plan.replacement_shape.as_str(),
                "changed": plan.changed,
                "written": written,
                "targets": plan
                    .targets
                    .iter()
                    .map(|target| json!({
                        "path": target.form_path.to_string(),
                        "span": {
                            "start": target.span.start().get(),
                            "end": target.span.end().get(),
                        },
                        "shape": target.shape.as_str(),
                        "text": target.text.as_str(),
                    }))
                    .collect::<Vec<_>>(),
                "rewritten": plan.rewritten.as_str(),
            }))?
        ),
    }

    Ok(())
}
