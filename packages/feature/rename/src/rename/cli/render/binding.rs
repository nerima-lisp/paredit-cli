use anyhow::Result;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::rename::usecase as rename_usecase;

pub fn print_rename_binding_plan(
    plan: &rename_usecase::RenameBindingPlan,
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
            println!(
                "binding_span\t{}..{}",
                plan.binding_span.start().get(),
                plan.binding_span.end().get()
            );
            println!("from\t{}", safe_text!(plan.from));
            println!("to\t{}", safe_text!(plan.to));
            println!("reference_count\t{}", plan.references.len());
            println!("shadowed_scope_count\t{}", plan.shadowed_scope_count);
            for span in &plan.references {
                println!("reference\t{}..{}", span.start().get(), span.end().get());
            }
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
                "binding_span": {
                    "start": plan.binding_span.start().get(),
                    "end": plan.binding_span.end().get(),
                },
                "from": plan.from.as_str(),
                "to": plan.to.as_str(),
                "reference_count": plan.references.len(),
                "references": plan
                    .references
                    .iter()
                    .map(|span| json!({
                        "span": {
                            "start": span.start().get(),
                            "end": span.end().get(),
                        },
                    }))
                    .collect::<Vec<_>>(),
                "shadowed_scope_count": plan.shadowed_scope_count,
                "changed": plan.changed,
                "written": written,
                "rewritten": plan.rewritten.as_str(),
            }))?
        ),
    }
    Ok(())
}
