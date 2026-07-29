use anyhow::Result;

use crate::context_report::cli::{args::ContextAtArgs, render::print_context_report};
use paredit_core_cli::gate::gate_failure;
use paredit_core_cli::shared::read_input_dialect_and_tree;

/// Reports what kind of text sits at one byte offset.
///
/// The command an agent runs *before* a character edit rather than after a
/// failed one: `edit delete-forward` and `edit newline` refuse an offset that
/// carries structure, and this says which offsets those are without attempting
/// the edit.
pub fn context_at_report(args: ContextAtArgs) -> Result<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    let context = tree.context_at(args.at)?;
    print_context_report(&input.text, dialect, &context, args.output)?;

    if args.fail_on_structural && !context.kind.is_structurally_inert() {
        return Err(gate_failure(format!(
            "fail-on-structural policy failed: byte offset {} is {}",
            context.offset,
            context.kind.as_str()
        )));
    }
    Ok(())
}
