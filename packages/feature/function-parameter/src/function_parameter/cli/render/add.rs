use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::function_parameter::usecase::AddFunctionParameterPlan;
use paredit_core_cli::args::OutputFormat;

pub fn print_add_function_parameter_plan(
    plan: &AddFunctionParameterPlan,
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
            println!("argument\t{}", safe_text!(plan.argument));
            println!("insert\t{}", plan.insert.label());
            println!("parameter_section\t{}", plan.section.label());
            for (path, span) in plan.call_paths.iter().zip(&plan.call_spans) {
                println!(
                    "call\t{}\t{}..{}",
                    safe_text!(path),
                    span.start().get(),
                    span.end().get()
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
                "argument": &plan.argument,
                "insert": plan.insert.label(),
                "parameter_section": plan.section.label(),
                "changed": plan.changed,
                "written": written,
                "rewritten": &plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
