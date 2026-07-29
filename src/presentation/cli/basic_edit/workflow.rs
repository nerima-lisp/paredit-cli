use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Edit, Formatter, Placement, Selection, SyntaxTree};
use crate::presentation::cli::args::{
    CompactSelectorArgs, CopyArgs, CursorArgs, EditTargetArgs, FormatArgs, KillArgs, KillRingArgs,
    NavigateArgs, NewlineArgs, OutputFormat, RaiseArgs, ReindentArgs, RepairArgs, ReplaceArgs,
    TargetArgs, TransposeArgs, UnwrapPrefixArgs, WrapArgs, YankArgs, YankPlacement,
};
use std::path::Path;

use crate::presentation::cli::shared::{
    edit_target, edit_target_with, emit_document, read_input_and_dialect,
    read_input_dialect_and_tree, resolve_compact, resolve_one, resolve_targets,
};
use paredit_core_cli::error::ArgumentError;
use paredit_core_cli::kill_ring::{KillRingEntry, read_ring, ring_path, write_ring};
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

/// Removes the selected forms, optionally keeping what it removed.
///
/// Routed through [`edit_target_with`] rather than resolving the selector here,
/// so `--to-ring` costs the command neither `--all` nor the range refusal. The
/// ring is written only after every edit has succeeded: a kill that refused
/// halfway should not leave half its forms on the clipboard.
pub(in crate::presentation::cli) fn kill(args: KillArgs) -> Result<()> {
    let KillArgs {
        target,
        write,
        diff,
        to_ring,
        kill_ring,
    } = args;
    let origin = target.file.clone();

    let mut killed = Vec::new();
    edit_target_with(
        EditTargetArgs {
            target,
            write,
            diff,
        },
        Edit::kill,
        |selection| {
            if to_ring {
                // Exactly what `kill` removes, which is the form and not the
                // comment block above it. `copy --to-ring` is the one that
                // takes the comments, because that is what `copy` prints.
                killed.push(selection.text().to_owned());
            }
            Ok(())
        },
    )?;

    if to_ring {
        // The hook ran in reverse source order; pushing back to front leaves
        // the ring's newest entry as the last form in the file.
        for text in killed.into_iter().rev() {
            push_to_ring(&kill_ring, origin.as_deref(), &text)?;
        }
    }
    Ok(())
}

pub(in crate::presentation::cli) fn wrap(args: WrapArgs) -> Result<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    let selection = resolve_one(&tree, dialect, &args.target.selector, "edit wrap")?;
    let rewritten = match (args.prefix, args.delimiter.list_delimiter()) {
        (Some(prefix), _) => Edit::wrap_prefix(&input.text, &tree, selection, prefix.into())?,
        (None, Some(delimiter)) => Edit::wrap(&input.text, &tree, selection, delimiter)?,
        // `--delimiter doublequote`: a string is a quotation, not a delimiter
        // pair, so the selection's own quotes and backslashes are escaped.
        (None, None) => Edit::wrap_string(&input.text, &tree, selection)?,
    };
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    Ok(emit_document(
        &input, dialect, args.write, args.diff, rewritten,
    )?)
}

pub(in crate::presentation::cli) fn unwrap_prefix(args: UnwrapPrefixArgs) -> Result<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    let selection = resolve_one(&tree, dialect, &args.target.selector, "edit unwrap-prefix")?;
    let rewritten = Edit::unwrap_prefix(&input.text, &tree, selection, args.all_prefixes)?;
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

pub(in crate::presentation::cli) fn raise(args: RaiseArgs) -> Result<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    let selection = resolve_one(&tree, dialect, &args.target.selector, "edit raise")?;
    let rewritten = Edit::raise_levels(&input.text, &tree, selection, args.levels as usize)?;
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    Ok(emit_document(
        &input, dialect, args.write, args.diff, rewritten,
    )?)
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

/// Swaps two expressions that share a list, adjacent or not.
///
/// The adjacent pair already has `transpose-forward` and `transpose-backward`;
/// this is the one that needs a second address, so it is the one command in the
/// namespace that takes `--with-path` / `--with-at`.
pub(in crate::presentation::cli) fn transpose(args: TransposeArgs) -> Result<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    let first = resolve_one(&tree, dialect, &args.target.selector, "edit transpose")?;
    if args.with_path.is_none() && args.with_at.is_none() && args.with_select.is_none() {
        return Err(ArgumentError::SecondTargetRequired.into());
    }
    // Built here rather than flattened: clap cannot carry two selectors whose
    // flags are spelled the same, so the partner keeps the `--with-` names and
    // is handed to the same resolver.
    let second = resolve_compact(
        &tree,
        dialect,
        &CompactSelectorArgs {
            path: args.with_path.clone(),
            at: args.with_at,
            select: args.with_select.clone(),
        },
        "edit transpose",
    )?;
    let rewritten = Edit::transpose_siblings(&input.text, &tree, first, second)?;
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    Ok(emit_document(
        &input, dialect, args.write, args.diff, rewritten,
    )?)
}

/// Reports where one structural move lands, as a `--path` the other commands
/// accept verbatim.
pub(in crate::presentation::cli) fn navigate(args: NavigateArgs) -> Result<()> {
    let (_, dialect, tree) = read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    let from = resolve_one(&tree, dialect, &args.target.selector, "edit navigate")?;
    let to = from.navigate(args.direction.into())?;

    match args.output {
        // One bare path on stdout, so `--path "$(paredit edit navigate ...)"`
        // is the whole composition story.
        OutputFormat::Text => println!("{}", to.path()),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": dialect.label(),
                "direction": paredit_core_syntax::sexpr::Direction::from(args.direction).as_str(),
                "from": selection_json(from),
                "to": selection_json(to),
            }))?
        ),
    }
    Ok(())
}

/// Prints the selected form together with the comment block written above it.
pub(in crate::presentation::cli) fn copy(args: CopyArgs) -> Result<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    let selection = resolve_one(&tree, dialect, &args.target.selector, "edit copy")?;
    let copied = Edit::copy(&input.text, &tree, selection)?;
    if args.to_ring {
        push_to_ring(&args.kill_ring, input.file.as_deref(), &copied)?;
    }
    print!("{copied}");
    Ok(())
}

/// Pastes a kill ring entry beside, or over, the selected form.
pub(in crate::presentation::cli) fn yank(args: YankArgs) -> Result<()> {
    let path = ring_path(args.kill_ring.ring.clone());
    let ring = read_ring(&path)?;
    let entry = ring
        .get(args.index)
        .ok_or_else(|| ArgumentError::KillRingIndexOutOfRange {
            path: path.display().to_string(),
            index: args.index,
            available: ring.entries.len(),
        })?;

    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    SyntaxTree::parse_with_dialect(&entry.text, dialect)
        .context("kill ring entry is not a valid S-expression document")?;
    let selection = resolve_one(&tree, dialect, &args.target.selector, "edit yank")?;
    // The entry carries the indentation it had where it was cut; the insertion
    // point supplies its own, and keeping both would indent it twice.
    let text = entry.text.trim_start();

    let rewritten = match args.placement {
        YankPlacement::Before => {
            Edit::insert_sibling(&input.text, &tree, selection, text, Placement::Before)?
        }
        YankPlacement::After => {
            Edit::insert_sibling(&input.text, &tree, selection, text, Placement::After)?
        }
        YankPlacement::Replace => Edit::replace(&input.text, selection, text)?,
    };
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    Ok(emit_document(
        &input, dialect, args.write, args.diff, rewritten,
    )?)
}

pub(in crate::presentation::cli) fn delete_forward(args: CursorArgs) -> Result<()> {
    cursor_edit(args, Edit::delete_forward)
}

pub(in crate::presentation::cli) fn delete_backward(args: CursorArgs) -> Result<()> {
    cursor_edit(args, Edit::delete_backward)
}

/// Inserts a newline and reindents the definition it landed in.
///
/// The reindent runs against a *reparse* of the result: the tree the insertion
/// was planned on describes the document before it, and every offset past the
/// cursor has moved by one.
pub(in crate::presentation::cli) fn newline(args: NewlineArgs) -> Result<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.cursor.file, args.cursor.dialect)?;
    let inserted = Edit::insert_newline(&input.text, &tree, args.cursor.at)?;
    let rewritten = if args.no_reindent {
        inserted
    } else {
        let reparsed = SyntaxTree::parse_with_dialect(&inserted, dialect)
            .context("newline insertion produced a document that does not reparse")?;
        reparsed.reindent_form_at(args.cursor.at, args.indent)
    };
    // The break usually strands the space that used to separate two forms at
    // the end of the line it left behind.
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    validated(&rewritten, dialect)?;
    Ok(emit_document(
        &input,
        dialect,
        args.cursor.write,
        args.cursor.diff,
        rewritten,
    )?)
}

/// Reindents the selected definition without rewrapping its lines.
pub(in crate::presentation::cli) fn reindent_defun(args: ReindentArgs) -> Result<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    let selection = resolve_one(&tree, dialect, &args.target.selector, "edit reindent-defun")?;
    let rewritten = tree.reindent(selection.span(), args.indent);
    validated(&rewritten, dialect)?;
    Ok(emit_document(
        &input, dialect, args.write, args.diff, rewritten,
    )?)
}

pub(in crate::presentation::cli) fn split_string(args: CursorArgs) -> Result<()> {
    cursor_edit(args, Edit::split_string)
}

pub(in crate::presentation::cli) fn escape_string(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::escape_string)?)
}

pub(in crate::presentation::cli) fn unescape_string(args: EditTargetArgs) -> Result<()> {
    Ok(edit_target(args, Edit::unescape_string)?)
}

/// The shared body of the offset-addressed edits.
///
/// No trivia normalization: these commands promise to change exactly the
/// characters they name, and trimming the rest of the line would break that
/// promise in the one place a caller is counting bytes.
fn cursor_edit(
    args: CursorArgs,
    edit: fn(&str, &SyntaxTree, usize) -> paredit_core_syntax::sexpr::SexprResult<String>,
) -> Result<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    let rewritten = edit(&input.text, &tree, args.at)?;
    validated(&rewritten, dialect)?;
    Ok(emit_document(
        &input, dialect, args.write, args.diff, rewritten,
    )?)
}

/// Reparses a rewrite before it is printed, not only before it is written.
///
/// `emit_document` already guards `--write`. These commands guard stdout too,
/// because a caller piping the result into the next command would otherwise
/// carry a broken document one step further before anything noticed.
fn validated(rewritten: &str, dialect: Dialect) -> Result<()> {
    SyntaxTree::parse_with_dialect(rewritten, dialect)
        .context("refusing to emit: the rewritten source does not reparse")?;
    Ok(())
}

/// Pushes `text` onto the kill ring, recording which file it came from.
///
/// The caller chooses what `text` is, because the two callers disagree on
/// purpose: `kill` stores what it removed, `copy` stores what it printed.
fn push_to_ring(args: &KillRingArgs, origin: Option<&Path>, text: &str) -> Result<()> {
    let path = ring_path(args.ring.clone());
    let mut ring = read_ring(&path)?;
    ring.push(
        KillRingEntry {
            text: text.to_owned(),
            origin: origin.map(|file| file.display().to_string()),
        },
        args.ring_size,
    );
    Ok(write_ring(&path, &ring)?)
}

fn selection_json(selection: Selection<'_>) -> serde_json::Value {
    json!({
        "path": selection.path().to_string(),
        "span": {
            "start": selection.span().start().get(),
            "end": selection.span().end().get(),
        },
        "kind": match selection.kind() {
            paredit_core_syntax::sexpr::ExpressionKind::List => "list",
            paredit_core_syntax::sexpr::ExpressionKind::Atom => "atom",
            paredit_core_syntax::sexpr::ExpressionKind::Root => "root",
        },
        "head": selection.head(),
    })
}
