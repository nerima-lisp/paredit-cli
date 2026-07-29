use paredit_core_cli::CliResult;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::rename::usecase as rename_usecase;

pub fn print_rename_in_form_plan(
    plan: &rename_usecase::RenameInFormPlan,
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
                "scope_span\t{}..{}",
                plan.scope_span.start().get(),
                plan.scope_span.end().get()
            );
            println!("from\t{}", safe_text!(plan.from));
            println!("to\t{}", safe_text!(plan.to));
            println!("count\t{}", plan.occurrences.len());
            for span in &plan.occurrences {
                println!("occurrence\t{}..{}", span.start().get(), span.end().get());
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
                "scope_span": {
                    "start": plan.scope_span.start().get(),
                    "end": plan.scope_span.end().get(),
                },
                "from": plan.from.as_str(),
                "to": plan.to.as_str(),
                "count": plan.occurrences.len(),
                "occurrences": plan
                    .occurrences
                    .iter()
                    .map(|span| json!({
                        "span": {
                            "start": span.start().get(),
                            "end": span.end().get(),
                        },
                    }))
                    .collect::<Vec<_>>(),
                "changed": plan.changed,
                "written": written,
                "rewritten": plan.rewritten.as_str(),
            }))?
        ),
    }
    Ok(())
}
