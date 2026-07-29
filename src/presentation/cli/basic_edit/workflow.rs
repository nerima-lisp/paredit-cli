use anyhow::{Context, Result, bail};

use crate::domain::sexpr::{Edit, Formatter, SyntaxTree};
use crate::presentation::cli::args::{
    EditTargetArgs, FormatArgs, RepairArgs, ReplaceArgs, TargetArgs, WrapArgs,
};
use crate::presentation::cli::shared::{
    edit_target, emit_document, read_input_and_dialect, read_input_dialect_and_tree, resolve_one,
    resolve_targets,
};
use paredit_core_syntax::selector::target_text;

pub(in crate::presentation::cli) fn format(args: FormatArgs) -> Result<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    let rendered = Formatter::with_dialect(args.indent, dialect).format(&tree);
    Ok(emit_document(
        &input, dialect, args.write, args.diff, rendered,
    )?)
}

pub(in crate::presentation::cli) fn repair_unclosed_lists(args: RepairArgs) -> Result<()> {
    let (input, dialect) = read_input_and_dialect(args.file, args.dialect)?;
    let repaired = SyntaxTree::repair_unclosed_lists(&input.text)
        .context("repair-unclosed-lists only repairs unclosed lists")?;
    if repaired == input.text {
        bail!("input is already balanced");
    }
    Ok(emit_document(
        &input, dialect, args.write, args.diff, repaired,
    )?)
}

/// Prints the selected source text.
///
/// With `--all` the matches are separated by a newline rather than run
/// together. A single match is still printed bare, with no trailing newline,
/// because that is what callers pipe into other commands. Use
/// `inspect resolve` when the matched forms may themselves span lines and the
/// separator has to be unambiguous.
pub(in crate::presentation::cli) fn select(args: TargetArgs) -> Result<()> {
    let (_, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    let targets = resolve_targets(&tree, dialect, &args.selector)?;
    let rendered = targets
        .iter()
        .map(|target| target_text(&tree, target))
        .collect::<Vec<_>>();
    print!("{}", rendered.join("\n"));
    Ok(())
}

pub(in crate::presentation::cli) fn replace(args: ReplaceArgs) -> Result<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    SyntaxTree::parse_with_dialect(&args.with, dialect)
        .context("replacement is not a valid S-expression document")?;
    let selection = resolve_one(&tree, dialect, &args.selector, "edit replace")?;
    let rewritten = Edit::replace(&input.text, selection, &args.with)?;
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    Ok(emit_document(
        &input, dialect, args.write, args.diff, rewritten,
    )?)
}

pub(in crate::presentation::cli) fn kill(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::kill)?)
}

pub(in crate::presentation::cli) fn wrap(args: WrapArgs) -> Result<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    let selection = resolve_one(&tree, dialect, &args.target.selector, "edit wrap")?;
    let rewritten = Edit::wrap(&input.text, &tree, selection, args.delimiter.into())?;
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    Ok(emit_document(
        &input, dialect, args.write, args.diff, rewritten,
    )?)
}

pub(in crate::presentation::cli) fn splice(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::splice)?)
}

pub(in crate::presentation::cli) fn split(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::split)?)
}

pub(in crate::presentation::cli) fn join(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::join)?)
}

pub(in crate::presentation::cli) fn splice_killing_backward(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::splice_killing_backward)?)
}

pub(in crate::presentation::cli) fn splice_killing_forward(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::splice_killing_forward)?)
}

pub(in crate::presentation::cli) fn convolute(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::convolute)?)
}

pub(in crate::presentation::cli) fn raise(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::raise)?)
}

pub(in crate::presentation::cli) fn transpose_forward(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::transpose_forward)?)
}

pub(in crate::presentation::cli) fn transpose_backward(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::transpose_backward)?)
}

pub(in crate::presentation::cli) fn slurp_forward(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::slurp_forward)?)
}

pub(in crate::presentation::cli) fn slurp_backward(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::slurp_backward)?)
}

pub(in crate::presentation::cli) fn barf_forward(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::barf_forward)?)
}

pub(in crate::presentation::cli) fn barf_backward(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::barf_backward)?)
}
