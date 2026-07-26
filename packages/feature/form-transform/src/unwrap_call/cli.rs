use super::*;
use crate::application::usecase::unwrap_call::{
    UnwrapCallPlan, UnwrapCallRequest, plan_unwrap_call,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;

#[derive(Debug, Args)]
pub(super) struct UnwrapCallArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    dialect: Option<DialectArg>,
    /// Select by child index path, for example 0.2.1.
    #[arg(long, conflicts_with = "at")]
    path: Option<Path>,
    /// Select the smallest expression containing byte offset.
    #[arg(long, conflicts_with = "path")]
    at: Option<usize>,
    /// Optional guard: fail unless the selected call has this function head.
    #[arg(long)]
    function: Option<SymbolName>,
    /// Zero-based call argument to keep. The function head is not counted.
    #[arg(long, default_value_t = 0)]
    argument_index: usize,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub(super) fn unwrap_call(args: UnwrapCallArgs) -> Result<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let selection = resolve_target(&tree, args.path.as_ref(), args.at)?;
    let selected = selection.view();
    let plan = plan_unwrap_call(UnwrapCallRequest {
        input: &input.text,
        dialect,
        path: args.path,
        target: selected,
        expected_function: args.function,
        argument_index: args.argument_index,
    })?;
    let mut written = false;

    if args.write {
        let file = input.file.as_ref().context("--write requires --file")?;
        if plan.changed {
            write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
        }
        written = true;
    }

    print_unwrap_call_plan(&plan, written, args.output)
}

fn print_unwrap_call_plan(
    plan: &UnwrapCallPlan,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            if let Some(path) = &plan.path {
                println!("path\t{}", safe_text!(path));
            }
            println!("function\t{}", safe_text!(plan.function));
            println!(
                "span\t{}..{}",
                plan.span.start().get(),
                plan.span.end().get()
            );
            println!("argument_index\t{}", plan.argument_index);
            println!(
                "argument_span\t{}..{}",
                plan.argument_span.start().get(),
                plan.argument_span.end().get()
            );
            println!("call_argument_count\t{}", plan.call_argument_count);
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
                "function": plan.function.as_str(),
                "span": {
                    "start": plan.span.start().get(),
                    "end": plan.span.end().get(),
                },
                "argumentIndex": plan.argument_index,
                "argumentSpan": {
                    "start": plan.argument_span.start().get(),
                    "end": plan.argument_span.end().get(),
                },
                "callArgumentCount": plan.call_argument_count,
                "replacement": &plan.replacement,
                "changed": plan.changed,
                "written": written,
                "rewritten": &plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
