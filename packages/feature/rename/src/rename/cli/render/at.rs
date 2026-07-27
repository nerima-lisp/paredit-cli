use anyhow::Result;
use serde_json::json;

use super::super::super::OutputFormat;
use crate::application::usecase::rename::RenameAtPlan;

pub(in crate::presentation::cli::rename) fn print_rename_at_plan(
    plan: &RenameAtPlan,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("namespace\t{}", plan.namespace.label());
            println!("from\t{}", safe_text!(plan.from));
            println!("to\t{}", safe_text!(plan.to));
            println!("occurrence_count\t{}", plan.occurrences.len());
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": plan.dialect.label(),
                "namespace": plan.namespace.label(),
                "selection_span": {"start": plan.selection_span.start().get(), "end": plan.selection_span.end().get()},
                "from": plan.from.as_str(),
                "to": plan.to.as_str(),
                "occurrences": plan.occurrences.iter().map(|span| json!({"start": span.start().get(), "end": span.end().get()})).collect::<Vec<_>>(),
                "changed": plan.changed,
                "written": written,
                "rewritten": plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
