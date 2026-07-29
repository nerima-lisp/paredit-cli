use crate::error::CallBindingError;
use crate::inline_function::usecase::{
    InlineCallSelection, InlineFunctionPlan, InlineFunctionRequest, plan_inline_function,
};
use clap::Args;
use paredit_core_cli::CliResult;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use paredit_core_cli::shared::read_input_and_dialect;
use paredit_core_cli::shared::require_output_file;
use paredit_core_cli::shared::write_file_with_rollback;
use paredit_core_syntax::sexpr::Path;
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct InlineFunctionArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    dialect: Option<DialectArg>,
    /// Select the function definition by child index path, for example 0.
    #[arg(long)]
    definition_path: Path,
    /// Select function calls to inline by child index path, for example 1.3. Repeat to inline multiple calls.
    #[arg(long = "call-path")]
    call_paths: Vec<Path>,
    /// Inline every same-file call whose list head matches the selected definition.
    #[arg(long)]
    all_calls: bool,
    /// Remove the selected definition after replacing the selected call.
    #[arg(long)]
    remove_definition: bool,
    /// Allow inlining when a parameter is referenced more than once.
    #[arg(long)]
    allow_duplicate_evaluation: bool,
    /// Allow inlining when a parameter is unused and the call argument would be dropped.
    #[arg(long)]
    allow_drop_arguments: bool,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub fn inline_function(args: InlineFunctionArgs) -> CliResult<()> {
    if args.write && args.file.is_none() {
        return Err(paredit_core_cli::ArgumentError::WriteRequiresFile.into());
    }

    // `--all-calls` and `--call-path` are mutually exclusive. The check lives
    // here, at the argument boundary where it is an argument problem, because
    // InlineCallSelection has no way to represent "both" for the domain to
    // reject later.
    if args.all_calls && !args.call_paths.is_empty() {
        return Err(CallBindingError::AllCallsAndCallPath {
            command: "inline-function",
        }
        .into());
    }

    let (input, dialect) = read_input_and_dialect(args.file.clone(), args.dialect)?;
    let plan = plan_inline_function(InlineFunctionRequest {
        input: &input.text,
        dialect,
        definition_path: args.definition_path,
        calls: if args.all_calls {
            InlineCallSelection::AllCalls
        } else {
            InlineCallSelection::Paths(args.call_paths)
        },
        remove_definition: args.remove_definition,
        allow_duplicate_evaluation: args.allow_duplicate_evaluation,
        allow_drop_arguments: args.allow_drop_arguments,
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_inline_function_plan(&plan, written, args.output)
}

fn print_inline_function_plan(
    plan: &InlineFunctionPlan,
    written: bool,
    output: OutputFormat,
) -> CliResult<()> {
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
            println!("function_name\t{}", safe_text!(plan.function_name));
            for call in &plan.calls {
                println!("call_path\t{}", safe_text!(call.call_path));
                println!(
                    "call_span\t{}..{}",
                    call.call_span.start().get(),
                    call.call_span.end().get()
                );
                for parameter in &call.parameters {
                    println!(
                        "parameter\t{}\targument={}\treferences={}",
                        safe_text!(parameter.name),
                        safe_text!(parameter.argument),
                        parameter.reference_count
                    );
                }
                println!("replacement\t{}", safe_text!(call.replacement));
            }
            println!("remove_definition\t{}", plan.remove_definition);
            println!("definition_removed\t{}", plan.definition_removed);
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": plan.dialect.label(),
                "definition_path": plan.definition_path.to_string(),
                "call_path": plan.call_paths.first().map(ToString::to_string),
                "call_paths": plan.call_paths.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "all_calls": plan.all_calls,
                "definition_span": {
                    "start": plan.definition_span.start().get(),
                    "end": plan.definition_span.end().get(),
                },
                "call_span": plan.call_spans.first().map(|span| json!({
                    "start": span.start().get(),
                    "end": span.end().get(),
                })),
                "call_spans": plan.call_spans.iter().map(|span| json!({
                    "start": span.start().get(),
                    "end": span.end().get(),
                })).collect::<Vec<_>>(),
                "function_name": plan.function_name.as_str(),
                "parameters": plan.calls.first().map(|call| call.parameters.iter().map(|parameter| {
                    json!({
                        "name": &parameter.name,
                        "argument": &parameter.argument,
                        "reference_count": parameter.reference_count,
                    })
                }).collect::<Vec<_>>()).unwrap_or_default(),
                "replacement": plan.calls.first().map(|call| call.replacement.as_str()),
                "calls": plan.calls.iter().map(|call| {
                    json!({
                        "call_path": call.call_path.to_string(),
                        "call_span": {
                            "start": call.call_span.start().get(),
                            "end": call.call_span.end().get(),
                        },
                        "parameters": call.parameters.iter().map(|parameter| {
                            json!({
                                "name": &parameter.name,
                                "argument": &parameter.argument,
                                "reference_count": parameter.reference_count,
                            })
                        }).collect::<Vec<_>>(),
                        "replacement": &call.replacement,
                    })
                }).collect::<Vec<_>>(),
                "remove_definition": plan.remove_definition,
                "definition_removed": plan.definition_removed,
                "changed": plan.changed,
                "written": written,
                "rewritten": &plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
