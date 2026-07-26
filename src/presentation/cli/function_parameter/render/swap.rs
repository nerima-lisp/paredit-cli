use anyhow::Result;
use serde_json::json;

use crate::application::usecase::function_parameter::SwapFunctionParametersPlan;
use crate::presentation::cli::args::OutputFormat;

pub(in crate::presentation::cli::function_parameter) fn print_swap_function_parameters_plan(
    plan: &SwapFunctionParametersPlan,
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
            println!("left_name\t{}", safe_text!(plan.left_name));
            println!("right_name\t{}", safe_text!(plan.right_name));
            println!("left_index\t{}", plan.left_index);
            println!("right_index\t{}", plan.right_index);
            for ((path, span), (left_argument, right_argument)) in plan
                .call_paths
                .iter()
                .zip(&plan.call_spans)
                .zip(&plan.swapped_arguments)
            {
                println!(
                    "call\t{}\t{}..{}\tleft_argument={}\tright_argument={}",
                    safe_text!(path),
                    span.start().get(),
                    span.end().get(),
                    safe_text!(left_argument),
                    safe_text!(right_argument)
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
                "left_name": plan.left_name.as_str(),
                "right_name": plan.right_name.as_str(),
                "left_index": plan.left_index,
                "right_index": plan.right_index,
                "swapped_arguments": plan.swapped_arguments.iter().map(|(left, right)| {
                    json!({
                        "left": left,
                        "right": right,
                    })
                }).collect::<Vec<_>>(),
                "changed": plan.changed,
                "written": written,
                "rewritten": &plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
