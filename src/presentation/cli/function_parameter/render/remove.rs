use anyhow::Result;
use serde_json::json;

use crate::application::usecase::function_parameter::RemoveFunctionParameterPlan;
use crate::presentation::cli::args::OutputFormat;

pub(in crate::presentation::cli::function_parameter) fn print_remove_function_parameter_plan(
    plan: &RemoveFunctionParameterPlan,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("definition_path\t{}", safe_text!(plan.definition_path));
            println!("all_calls\t{}", plan.all_calls);
            println!(
                "definition_span\t{}..{}",
                plan.definition_span.start().get(),
                plan.definition_span.end().get()
            );
            println!(
                "parameter_list_span\t{}..{}",
                plan.parameter_list_span.start().get(),
                plan.parameter_list_span.end().get()
            );
            println!("function_name\t{}", safe_text!(plan.function_name));
            println!("parameter_name\t{}", safe_text!(plan.parameter_name));
            println!("parameter_index\t{}", plan.parameter_index);
            if let Some(keyword) = plan.parameter_keyword.as_deref() {
                println!("parameter_keyword\t{}", safe_text!(keyword));
            }
            for ((path, span), removed_argument) in plan
                .call_paths
                .iter()
                .zip(&plan.call_spans)
                .zip(&plan.removed_arguments)
            {
                println!(
                    "call\t{}\t{}..{}\tremoved_argument={}",
                    safe_text!(path),
                    span.start().get(),
                    span.end().get(),
                    safe_text!(removed_argument.as_deref().unwrap_or("<missing>"))
                );
            }
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": plan.dialect.label(),
                "definition_path": plan.definition_path.to_string(),
                "call_paths": plan.call_paths.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "all_calls": plan.all_calls,
                "definition_span": {
                    "start": plan.definition_span.start().get(),
                    "end": plan.definition_span.end().get(),
                },
                "parameter_list_span": {
                    "start": plan.parameter_list_span.start().get(),
                    "end": plan.parameter_list_span.end().get(),
                },
                "call_spans": plan.call_spans.iter().map(|span| {
                    json!({
                        "start": span.start().get(),
                        "end": span.end().get(),
                    })
                }).collect::<Vec<_>>(),
                "function_name": plan.function_name.as_str(),
                "parameter_name": plan.parameter_name.as_str(),
                "parameter_index": plan.parameter_index,
                "parameter_keyword": &plan.parameter_keyword,
                "removed_arguments": &plan.removed_arguments,
                "changed": plan.changed,
                "written": written,
                "rewritten": &plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
