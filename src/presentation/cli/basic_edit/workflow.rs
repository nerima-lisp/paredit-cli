use std::io::IsTerminal;

use paredit_core_cli::CliResult;
use serde_json::json;

use crate::presentation::cli::args::{
    CanonicalizeArgs, CompactSelectorArgs, CopyArgs, CursorArgs, EditTargetArgs, FormatArgs,
    KillArgs, KillRingArgs, NavigateArgs, NewlineArgs, NormalizeQuotesArgs, OutputFormat,
    RaiseArgs, ReindentArgs, RepairArgs, ReplaceArgs, TargetArgs, TransposeArgs, UnwrapPrefixArgs,
    WrapArgs, YankArgs, YankPlacement,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    Edit, Formatter, NumericLiteralCase, Placement, ReaderPrefixStyle, Selection, SyntaxTree,
};
use std::path::Path;

use crate::presentation::cli::shared::{
    AggregateDiffStat, DiffStat, analyze_files, diff_stat, edit_target, edit_target_in_dialect,
    edit_target_with, emit_document, expand_input_files, note_partial_file_failures,
    read_input_and_dialect, read_input_dialect_and_tree, resolve_compact, resolve_one,
    resolve_targets, total_file_failure, unified_diff,
};
use paredit_core_cli::error::ArgumentError;
use paredit_core_cli::kill_ring::{KillRingEntry, read_ring, ring_path, write_ring};
use paredit_core_syntax::selector::target_text;

pub(in crate::presentation::cli) fn format(args: FormatArgs) -> CliResult<()> {
    // `--diff-stat` is the one mode more than one file can drive; every other
    // mode (plain output, `--write`, `--diff`, `--check`) still names exactly
    // one file, or stdin, through `--file`. `paths` requires `--diff-stat`
    // already, at the argument parser.
    if !args.paths.is_empty() {
        return format_diff_stat_many(args);
    }

    let check = args.check;
    let diff_stat_only = args.diff_stat;
    // Computed before `args.file`/`args.dialect` are consumed below: only
    // borrows `args`, so the ordering here is about readability, not
    // ownership.
    let max_width = args
        .max_width
        .or_else(|| auto_detected_terminal_width(&args));
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    let formatter = build_formatter(FormatOptions {
        indent: args.indent,
        dialect,
        max_width,
        reindent_block_comments: args.reindent_block_comments,
        comment_column: args.comment_column,
        max_blank_lines: args.max_blank_lines,
        indent_table: &args.indent_table,
        width_profiles: &args.width_profiles,
        quote_style: args.quote_style.into(),
        numeric_literal_case: args.numeric_literal_case.into(),
        align_clause_values: args.align_clause_values,
        insert_final_newline: args.insert_final_newline,
        trim_trailing_whitespace: args.trim_trailing_whitespace,
    })?;
    let rendered = formatter.format(&tree);

    if check {
        // Neither writes nor prints a diff: a CI gate that runs this over
        // every file in a tree wants one exit code per file, not a diff
        // dumped for every clean one too. `edit format --diff` is still
        // there for "and show me what changed".
        if rendered == input.text {
            return Ok(());
        }
        let where_ = input
            .file
            .as_deref()
            .map_or_else(|| "stdin".to_owned(), |path| path.display().to_string());
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,
            format!("{where_} is not formatted; drop --check to see the diff or write it"),
        )
        .into());
    }

    if diff_stat_only {
        let path = input
            .file
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("stdin"));
        let diff = unified_diff(&path, &input.text, &rendered);
        let stat = diff_stat(&diff);
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "changed": !diff.is_empty(),
                "hunks": stat.hunks,
                "added_lines": stat.added_lines,
                "removed_lines": stat.removed_lines,
            }))?
        );
        return Ok(());
    }

    emit_document(&input, dialect, args.write, args.diff, rendered)
}

/// The terminal's own column width, when nothing else already decided
/// `--max-width` and using it is actually appropriate — `None` otherwise,
/// which leaves [`build_formatter`] falling back to the compiled-in default
/// exactly as before this option existed (FR-014).
///
/// Three conditions, all necessary:
///
/// - `--max-width` was not passed, *and* no `format.max-inline-width`
///   resolved either. Both are the same check here (`args.max_width.is_some()`)
///   because `config_bridge` already turns a configured
///   `format.max-inline-width` into `--max-width` before `clap` ever parses
///   `FormatArgs` — by the time this runs, "the flag is absent" and "neither
///   source set it" are the same fact.
/// - The command is not `--write` (nothing is printed to a terminal at all)
///   nor `--check`/`--diff-stat` (their result is a machine-consumed exit
///   code or JSON summary, not something rendered *for* a human at a
///   terminal — and unlike the FR's explicit `--diff-stat` exclusion,
///   `--check` is excluded on the same reasoning even though the FR does not
///   name it: an exit code that depends on the width of whoever's terminal
///   happened to run it would make the same file pass in one shell and fail
///   in another, and disagree with the fixed-width answer CI (which has no
///   tty) already gives for the identical file).
/// - Stdout is actually an interactive terminal. Piped or redirected output
///   keeps the fixed default, matching every CI run and every test in this
///   suite — this is the one branch of this function a test harness (whose
///   stdout is always captured, never a live tty) can exercise directly; the
///   "really is a terminal" branch can only be verified by hand, by running
///   `edit format` with no `--file`/`--write`/`--check`/`--diff-stat` at an
///   actual terminal and a piped one side by side.
fn auto_detected_terminal_width(args: &FormatArgs) -> Option<usize> {
    if args.max_width.is_some() || args.write || args.check || args.diff_stat {
        return None;
    }
    std::io::stdout()
        .is_terminal()
        .then(paredit_core_cli::terminal::width)
}

/// One file's `--diff-stat` result, computed while aggregating over more than
/// one file.
struct FileDiffStat {
    path: String,
    changed: bool,
    stat: DiffStat,
}

/// `edit format --diff-stat` over `--file` (if given) and every `PATH`,
/// aggregated into one summary alongside each file's own stat.
///
/// Routed through [`analyze_files`], the same per-file, partial-failure-
/// resilient infrastructure `migrate run` and the multi-file reports use: a
/// file that will not parse is reported and excluded, not a reason to fail a
/// run over every other file too. Directories among `PATH` are expanded via
/// workspace discovery first, through [`expand_input_files`] — the same
/// expansion every other command that accepts a directory uses.
fn format_diff_stat_many(args: FormatArgs) -> CliResult<()> {
    let mut inputs = Vec::with_capacity(args.paths.len().saturating_add(1));
    inputs.extend(args.file.clone());
    inputs.extend(args.paths.clone());
    let files = expand_input_files(&inputs, args.dialect)?;

    let indent = args.indent;
    let max_width = args.max_width;
    let reindent_block_comments = args.reindent_block_comments;
    let comment_column = args.comment_column;
    let max_blank_lines = args.max_blank_lines;
    let indent_table = args.indent_table.clone();
    let width_profiles = args.width_profiles.clone();
    let quote_style: ReaderPrefixStyle = args.quote_style.into();
    let numeric_literal_case: NumericLiteralCase = args.numeric_literal_case.into();
    let align_clause_values = args.align_clause_values;
    let insert_final_newline = args.insert_final_newline;
    let trim_trailing_whitespace = args.trim_trailing_whitespace;
    // Never terminal-width-detected here, regardless of `max_width`: this
    // path only runs under `--diff-stat`, one of FR-014's own exclusions
    // (a machine-consumed JSON summary, not something rendered for a human).
    let analysis = analyze_files(&files, args.dialect, move |file, dialect, tree, input| {
        let formatter = build_formatter(FormatOptions {
            indent,
            dialect,
            max_width,
            reindent_block_comments,
            comment_column,
            max_blank_lines,
            indent_table: &indent_table,
            width_profiles: &width_profiles,
            quote_style,
            numeric_literal_case,
            align_clause_values,
            insert_final_newline,
            trim_trailing_whitespace,
        })?;
        let rendered = formatter.format(tree);
        let diff = unified_diff(file, &input.text, &rendered);
        let stat = diff_stat(&diff);
        CliResult::Ok(FileDiffStat {
            path: file.display().to_string(),
            changed: !diff.is_empty(),
            stat,
        })
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);

    let totals = AggregateDiffStat::of(
        analysis
            .succeeded
            .iter()
            .map(|entry| (entry.changed, entry.stat)),
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "files": analysis
                .succeeded
                .iter()
                .map(|entry| json!({
                    "path": entry.path,
                    "changed": entry.changed,
                    "hunks": entry.stat.hunks,
                    "added_lines": entry.stat.added_lines,
                    "removed_lines": entry.stat.removed_lines,
                }))
                .collect::<Vec<_>>(),
            "total_files": totals.files_scanned,
            "files_changed": totals.files_changed,
            "hunks": totals.hunks,
            "added_lines": totals.added_lines,
            "removed_lines": totals.removed_lines,
        }))?
    );
    Ok(())
}

/// Every width/style knob `FormatArgs` and `format_diff_stat_many` share,
/// bundled so [`build_formatter`] takes one argument instead of nine.
struct FormatOptions<'a> {
    indent: usize,
    dialect: Dialect,
    max_width: Option<usize>,
    reindent_block_comments: bool,
    comment_column: Option<usize>,
    max_blank_lines: Option<usize>,
    indent_table: &'a [String],
    width_profiles: &'a [String],
    quote_style: ReaderPrefixStyle,
    numeric_literal_case: NumericLiteralCase,
    align_clause_values: bool,
    insert_final_newline: bool,
    trim_trailing_whitespace: bool,
}

/// Builds the [`Formatter`] `edit format` renders with, threading through
/// every width/style knob `FormatArgs` and `format_diff_stat_many` share.
///
/// Fallible only because `--indent-table`/`--width-profile` are plain
/// strings at the argument-parser boundary (`SYMBOL=STYLE`/`STYLE=WIDTH`,
/// not something `clap` can shape-check on its own): a malformed entry or an
/// unrecognised style name is refused here with a message naming the flag,
/// rather than reaching [`Formatter`] itself. A value that came from
/// `paredit.toml` already passed this same shape and vocabulary check in
/// `packages/core/config`'s schema validation, so in practice this only ever
/// rejects a flag typed directly on the command line.
fn build_formatter(options: FormatOptions<'_>) -> CliResult<Formatter> {
    let FormatOptions {
        indent,
        dialect,
        max_width,
        reindent_block_comments,
        comment_column,
        max_blank_lines,
        indent_table,
        width_profiles,
        quote_style,
        numeric_literal_case,
        align_clause_values,
        insert_final_newline,
        trim_trailing_whitespace,
    } = options;
    let mut formatter = Formatter::with_dialect(indent, dialect)
        .with_reindent_block_comments(reindent_block_comments)
        .with_numeric_literal_case(numeric_literal_case)
        .with_align_clause_values(align_clause_values)
        .with_insert_final_newline(insert_final_newline)
        .with_trim_trailing_whitespace(trim_trailing_whitespace);
    if let Some(max_width) = max_width {
        formatter = formatter.with_max_width(max_width);
    }
    if let Some(comment_column) = comment_column {
        formatter = formatter.with_comment_column(comment_column);
    }
    if let Some(max_blank_lines) = max_blank_lines {
        formatter = formatter.with_max_blank_lines(max_blank_lines);
    }
    if !indent_table.is_empty() {
        let overrides = split_pairs(indent_table, "--indent-table", "SYMBOL=STYLE")?;
        formatter = formatter
            .with_indent_overrides(&overrides)
            .map_err(|error| style_name_refusal("--indent-table", &error))?;
    }
    if !width_profiles.is_empty() {
        let profiles = parse_width_profiles(width_profiles)?;
        formatter = formatter
            .with_width_profiles(&profiles)
            .map_err(|error| style_name_refusal("--width-profile", &error))?;
    }
    Ok(formatter.with_reader_prefix_style(quote_style))
}

/// Splits each `KEY=VALUE` entry in `raw`, refusing cleanly — not panicking —
/// when one is missing its `=`.
fn split_pairs(raw: &[String], flag: &str, shape: &str) -> CliResult<Vec<(String, String)>> {
    raw.iter()
        .map(|entry| {
            entry
                .split_once('=')
                .map(|(left, right)| (left.to_owned(), right.to_owned()))
                .ok_or_else(|| {
                    paredit_core_cli::error::FeatureRefusal::message(
                        paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,
                        format!("{flag} entry {entry:?} is not {shape}"),
                    )
                    .into()
                })
        })
        .collect()
}

/// Splits and parses every `--width-profile` entry, refusing cleanly when the
/// width half is not a plain integer.
fn parse_width_profiles(raw: &[String]) -> CliResult<Vec<(String, usize)>> {
    split_pairs(raw, "--width-profile", "STYLE=WIDTH")?
        .into_iter()
        .map(|(style, width_text)| {
            width_text.parse::<usize>().map(|width| (style, width)).map_err(|_| {
                paredit_core_cli::error::FeatureRefusal::message(
                    paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,
                    format!("--width-profile entry has a width that is not an integer: {width_text:?}"),
                )
                .into()
            })
        })
        .collect()
}

fn style_name_refusal(
    flag: &str,
    error: &paredit_core_syntax::sexpr::UnknownStyleName,
) -> paredit_core_cli::error::CliError {
    paredit_core_cli::error::FeatureRefusal::message(
        paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,
        format!("{flag}: {error}"),
    )
    .into()
}

pub(in crate::presentation::cli) fn repair_unclosed_lists(args: RepairArgs) -> CliResult<()> {
    let (input, dialect) = read_input_and_dialect(args.file, args.dialect)?;
    let repaired = SyntaxTree::repair_unclosed_lists(&input.text).map_err(|_| {
        paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,
            "repair-unclosed-lists only repairs unclosed lists",
        )
    })?;
    if repaired == input.text {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,
            "input is already balanced",
        )
        .into());
    }
    emit_document(&input, dialect, args.write, args.diff, repaired)
}

/// Sorts an alist- or plist-shaped data file's keys and flattens its
/// whitespace to a single space between elements, refusing a file that has
/// no confidently alist- or plist-shaped list anywhere in it.
///
/// Routed through the same [`emit_document`] every other `edit` command
/// writes through, so it inherits the same reparse guard, the same rollback,
/// and the same CRLF restoration `edit format` already relies on — this
/// command's own write-safety concern is narrower than "does it reparse", so
/// `canonicalize::same_data` is checked first and refuses before
/// `emit_document` is ever reached.
///
/// A file carrying any comment is refused outright, before either the
/// stdout or the `--write` path is reached. `SyntaxTree` keeps comments
/// outside the node tree entirely (see its own doc comment), so
/// `canonicalize_document`'s `render` — which walks `ExpressionView` nodes —
/// and `same_data`'s leaf-token equivalence check are both structurally
/// blind to them: a comment is neither a node `render` can copy forward nor
/// a leaf `same_data` can notice went missing. Rather than teach a
/// key-reordering rewrite how to keep an inline trailing comment attached to
/// the entry it followed — the same problem the formatter does not have,
/// because it never reorders anything — this command refuses instead, the
/// same "when in doubt, refuse" posture every other write command in this
/// namespace already takes.
pub(in crate::presentation::cli) fn canonicalize(args: CanonicalizeArgs) -> CliResult<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    if tree.comments().next().is_some() {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,
            "refusing to canonicalize a file containing a comment: this command reorders and \
             reflows entries but has no way to keep a comment attached to the entry it \
             belongs to, so rewriting would silently drop it",
        )
        .into());
    }
    let Some(rendered) = super::canonicalize::canonicalize_document(&input.text, &tree) else {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,
            "no alist- or plist-shaped list found; refusing to canonicalize a file that is not \
             confidently data",
        )
        .into());
    };

    let reparsed = SyntaxTree::parse_with_dialect(&rendered, dialect).map_err(|_| {
        paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::RefusalRewriteDoesNotReparse,
            "refusing to write: the canonicalized document does not reparse",
        )
    })?;
    if !super::canonicalize::same_data(&input.text, &tree, &rendered, &reparsed) {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::RefusalRewriteDoesNotReparse,
            "refusing to write: the canonicalized document does not carry the same keys and \
             values as the input",
        )
        .into());
    }

    emit_document(&input, dialect, args.write, args.diff, rendered)
}

/// Prints the selected source text.
///
/// With `--all` the matches are separated by a newline rather than run
/// together. A single match is still printed bare, with no trailing newline,
/// because that is what callers pipe into other commands. Use
/// `inspect resolve` when the matched forms may themselves span lines and the
/// separator has to be unambiguous.
pub(in crate::presentation::cli) fn select(args: TargetArgs) -> CliResult<()> {
    let (_, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    let targets = resolve_targets(&tree, dialect, &args.selector)?;
    let rendered = targets
        .iter()
        .map(|target| target_text(&tree, target))
        .collect::<Vec<_>>();
    print!("{}", rendered.join("\n"));
    Ok(())
}

pub(in crate::presentation::cli) fn replace(args: ReplaceArgs) -> CliResult<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    SyntaxTree::parse_with_dialect(&args.with, dialect).map_err(|_| {
        paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputUnparsable,
            "replacement is not a valid S-expression document",
        )
    })?;
    let selection = resolve_one(&tree, dialect, &args.selector, "edit replace")?;
    let rewritten = Edit::replace(&input.text, selection, &args.with)?;
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    emit_document(&input, dialect, args.write, args.diff, rewritten)
}

/// Removes the selected forms, optionally keeping what it removed.
///
/// Routed through [`edit_target_with`] rather than resolving the selector here,
/// so `--to-ring` costs the command neither `--all` nor the range refusal. The
/// ring is written only after every edit has succeeded: a kill that refused
/// halfway should not leave half its forms on the clipboard.
pub(in crate::presentation::cli) fn kill(args: KillArgs) -> CliResult<()> {
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
        |source, tree, selection, _| Edit::kill(source, tree, selection),
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

pub(in crate::presentation::cli) fn wrap(args: WrapArgs) -> CliResult<()> {
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
    emit_document(&input, dialect, args.write, args.diff, rewritten)
}

pub(in crate::presentation::cli) fn unwrap_prefix(args: UnwrapPrefixArgs) -> CliResult<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    let selection = resolve_one(&tree, dialect, &args.target.selector, "edit unwrap-prefix")?;
    let rewritten = Edit::unwrap_prefix(&input.text, &tree, selection, args.all_prefixes)?;
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    emit_document(&input, dialect, args.write, args.diff, rewritten)
}

pub(in crate::presentation::cli) fn splice(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::splice)
}

pub(in crate::presentation::cli) fn split(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::split)
}

/// Writes a second copy of the selected form immediately after it.
///
/// `edit copy --to-ring` followed by `edit yank --placement after` reaches the
/// same result in two calls, at the cost of whatever was on the kill ring.
pub(in crate::presentation::cli) fn duplicate(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::duplicate)
}

pub(in crate::presentation::cli) fn join(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::join)
}

pub(in crate::presentation::cli) fn splice_killing_backward(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::splice_killing_backward)
}

pub(in crate::presentation::cli) fn splice_killing_forward(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::splice_killing_forward)
}

pub(in crate::presentation::cli) fn convolute(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::convolute)
}

pub(in crate::presentation::cli) fn raise(args: RaiseArgs) -> CliResult<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    let selection = resolve_one(&tree, dialect, &args.target.selector, "edit raise")?;
    let rewritten = Edit::raise_levels(&input.text, &tree, selection, args.levels as usize)?;
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    emit_document(&input, dialect, args.write, args.diff, rewritten)
}

pub(in crate::presentation::cli) fn normalize_quotes(args: NormalizeQuotesArgs) -> CliResult<()> {
    let style = args.style.into();
    edit_target_in_dialect(
        EditTargetArgs {
            target: args.target,
            write: args.write,
            diff: args.diff,
        },
        move |source, tree, selection, dialect| {
            Edit::normalize_quotes(source, tree, selection, style, dialect)
        },
    )
}

pub(in crate::presentation::cli) fn transpose_forward(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::transpose_forward)
}

pub(in crate::presentation::cli) fn transpose_backward(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::transpose_backward)
}

pub(in crate::presentation::cli) fn slurp_forward(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::slurp_forward)
}

pub(in crate::presentation::cli) fn slurp_backward(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::slurp_backward)
}

pub(in crate::presentation::cli) fn barf_forward(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::barf_forward)
}

pub(in crate::presentation::cli) fn barf_backward(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::barf_backward)
}

/// Swaps two expressions that share a list, adjacent or not.
///
/// The adjacent pair already has `transpose-forward` and `transpose-backward`;
/// this is the one that needs a second address, so it is the one command in the
/// namespace that takes `--with-path` / `--with-at`.
pub(in crate::presentation::cli) fn transpose(args: TransposeArgs) -> CliResult<()> {
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
    emit_document(&input, dialect, args.write, args.diff, rewritten)
}

/// Reports where one structural move lands, as a `--path` the other commands
/// accept verbatim.
pub(in crate::presentation::cli) fn navigate(args: NavigateArgs) -> CliResult<()> {
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
pub(in crate::presentation::cli) fn copy(args: CopyArgs) -> CliResult<()> {
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
pub(in crate::presentation::cli) fn yank(args: YankArgs) -> CliResult<()> {
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
    SyntaxTree::parse_with_dialect(&entry.text, dialect).map_err(|_| {
        paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputUnparsable,
            "kill ring entry is not a valid S-expression document",
        )
    })?;
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
    emit_document(&input, dialect, args.write, args.diff, rewritten)
}

pub(in crate::presentation::cli) fn delete_forward(args: CursorArgs) -> CliResult<()> {
    cursor_edit(args, Edit::delete_forward)
}

pub(in crate::presentation::cli) fn delete_backward(args: CursorArgs) -> CliResult<()> {
    cursor_edit(args, Edit::delete_backward)
}

/// Inserts a newline and reindents the definition it landed in.
///
/// The reindent runs against a *reparse* of the result: the tree the insertion
/// was planned on describes the document before it, and every offset past the
/// cursor has moved by one.
pub(in crate::presentation::cli) fn newline(args: NewlineArgs) -> CliResult<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.cursor.file, args.cursor.dialect)?;
    let inserted = Edit::insert_newline(&input.text, &tree, args.cursor.at)?;
    let rewritten = if args.no_reindent {
        inserted
    } else {
        let reparsed = SyntaxTree::parse_with_dialect(&inserted, dialect).map_err(|_| {
            paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::RefusalRewriteDoesNotReparse,
                "newline insertion produced a document that does not reparse",
            )
        })?;
        reparsed.reindent_form_at(args.cursor.at, args.indent)
    };
    // The break usually strands the space that used to separate two forms at
    // the end of the line it left behind.
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    validated(&rewritten, dialect)?;
    emit_document(
        &input,
        dialect,
        args.cursor.write,
        args.cursor.diff,
        rewritten,
    )
}

/// Reindents the selected definition without rewrapping its lines.
pub(in crate::presentation::cli) fn reindent_defun(args: ReindentArgs) -> CliResult<()> {
    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.target.file, args.target.dialect)?;
    let selection = resolve_one(&tree, dialect, &args.target.selector, "edit reindent-defun")?;
    let rewritten = tree.reindent(selection.span(), args.indent);
    validated(&rewritten, dialect)?;
    emit_document(&input, dialect, args.write, args.diff, rewritten)
}

pub(in crate::presentation::cli) fn split_string(args: CursorArgs) -> CliResult<()> {
    cursor_edit(args, Edit::split_string)
}

pub(in crate::presentation::cli) fn escape_string(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::escape_string)
}

pub(in crate::presentation::cli) fn unescape_string(args: EditTargetArgs) -> CliResult<()> {
    edit_target(args, Edit::unescape_string)
}

/// The shared body of the offset-addressed edits.
///
/// No trivia normalization: these commands promise to change exactly the
/// characters they name, and trimming the rest of the line would break that
/// promise in the one place a caller is counting bytes.
fn cursor_edit(
    args: CursorArgs,
    edit: fn(&str, &SyntaxTree, usize) -> paredit_core_syntax::sexpr::SexprResult<String>,
) -> CliResult<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    let rewritten = edit(&input.text, &tree, args.at)?;
    validated(&rewritten, dialect)?;
    emit_document(&input, dialect, args.write, args.diff, rewritten)
}

/// Reparses a rewrite before it is printed, not only before it is written.
///
/// `emit_document` already guards `--write`. These commands guard stdout too,
/// because a caller piping the result into the next command would otherwise
/// carry a broken document one step further before anything noticed.
fn validated(rewritten: &str, dialect: Dialect) -> CliResult<()> {
    SyntaxTree::parse_with_dialect(rewritten, dialect).map_err(|_| {
        paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::RefusalRewriteDoesNotReparse,
            "refusing to emit: the rewritten source does not reparse",
        )
    })?;
    Ok(())
}

/// Pushes `text` onto the kill ring, recording which file it came from.
///
/// The caller chooses what `text` is, because the two callers disagree on
/// purpose: `kill` stores what it removed, `copy` stores what it printed.
fn push_to_ring(args: &KillRingArgs, origin: Option<&Path>, text: &str) -> CliResult<()> {
    let path = ring_path(args.ring.clone());
    let mut ring = read_ring(&path)?;
    ring.push(
        KillRingEntry {
            text: text.to_owned(),
            origin: origin.map(|file| file.display().to_string()),
        },
        args.ring_size,
    );
    write_ring(&path, &ring)
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
