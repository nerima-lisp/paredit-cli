use crate::error::{ArgumentError, CliError, CliResult, IoRefusal};
use std::collections::BTreeSet;
use std::fmt::{self, Display, Write};
use std::path::PathBuf;

use crate::args::{CompactSelectorArgs, DialectArg, EditTargetArgs, SelectorArgs, SourceInput};
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::{SelectorError, SelectorTarget, resolve as resolve_selector};
use paredit_core_syntax::sexpr::{
    AtomOccurrence, ByteSpan, Delimiter, Edit, ExpressionKind, ExpressionView, Path, Selection,
    SexprResult, SymbolName, SyntaxTree,
};
use paredit_core_workspace::workspace::{
    IgnoreOptions, WorkspaceDiscoveryOptions, WorkspaceLimits, discover_workspace_files_with_limits,
};

#[path = "diff.rs"]
mod diff;
#[path = "io.rs"]
mod io;
#[cfg(target_os = "macos")]
#[path = "macos_acl.rs"]
mod macos_acl;

pub use diff::{DiffStat, diff_stat, unified_diff};
pub use io::{AnchoredExpectedWrite, write_files_with_rollback_expected_anchored};
pub use io::{
    ExpectedWriteTarget, MAX_SOURCE_INPUT_BYTES, WritabilityCheck, check_deadline_for_read,
    check_writable, parse_document, read_file_or_empty, read_input_and_dialect,
    read_input_dialect_and_tree, read_input_dialect_tree_and_expected_target,
    read_text_file_with_expected_target, read_text_file_with_limit, read_text_with_limit,
    write_artifact_with_rollback, write_file_with_rollback, write_files_with_rollback,
    write_files_with_rollback_expected,
};

pub const fn terminal_safe<T: Display>(value: T) -> TerminalSafe<T> {
    TerminalSafe(value)
}

// Public since the extraction: this was crate-internal, a visibility that
// cannot cross a crate boundary, so the lint applies to it for the first time.
#[derive(Debug)]
pub struct TerminalSafe<T>(T);

impl<T: Display> Display for TerminalSafe<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(TerminalEscapeWriter(formatter), "{}", self.0)
    }
}

/// Renders an error's whole `source()` chain, escaped for a terminal.
///
/// Takes `&dyn Error` rather than a concrete type: the boundary renders a
/// [`crate::CommandFailure`], the budget check renders an
/// [`crate::ArgumentError`], and a feature is free to render its own. It used
/// to take `&anyhow::Error`, which meant every caller had to reach `anyhow`
/// just to print.
#[must_use]
pub const fn terminal_safe_error_chain(
    error: &dyn std::error::Error,
) -> TerminalSafeErrorChain<'_> {
    TerminalSafeErrorChain(error)
}

// Public since the extraction: this was crate-internal, a visibility that
// cannot cross a crate boundary, so the lint applies to it for the first time.
pub struct TerminalSafeErrorChain<'a>(&'a dyn std::error::Error);

impl Display for TerminalSafeErrorChain<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `error_chain` is what `{:#}` was: `thiserror`'s own `Display` stops
        // at the outermost message.
        write!(
            TerminalEscapeWriter(formatter),
            "{}",
            crate::error::error_chain(self.0)
        )
    }
}

impl fmt::Debug for TerminalSafeErrorChain<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("TerminalSafeErrorChain")
            .field(&self.0)
            .finish()
    }
}

struct TerminalEscapeWriter<'a, 'b>(&'a mut fmt::Formatter<'b>);

impl Write for TerminalEscapeWriter<'_, '_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        for character in value.chars() {
            if is_terminal_control(character) {
                write!(self.0, "\\u{{{:x}}}", u32::from(character))?;
            } else {
                self.0.write_char(character)?;
            }
        }
        Ok(())
    }
}

const fn is_terminal_control(character: char) -> bool {
    matches!(
        character,
        '\u{0}'..='\u{1f}'
            | '\u{7f}'..='\u{9f}'
            | '\u{61c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

pub fn apply_byte_span_edits(input: &str, mut edits: Vec<(ByteSpan, String)>) -> CliResult<String> {
    for (span, _) in &edits {
        span.validate_against(input)
            .map_err(|_| IoRefusal::RewriteSpanOutOfBounds)?;
    }
    edits.sort_by_key(|(span, _)| span.start());
    ensure_non_overlapping_spans(edits.iter().map(|(span, _)| *span))?;

    let mut output = input.to_owned();
    for (span, replacement) in edits.into_iter().rev() {
        output.replace_range(span.as_range(), &replacement);
    }
    Ok(output)
}

// Re-exported rather than defined here since the undo journal started
// comparing against the same digest. Two spellings of one hash would agree
// until the day one of them changed.
pub use paredit_core_safety::hash::stable_text_hash;

#[must_use]
pub fn bounded_preview(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_owned();
    }

    let mut end = max_bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

fn ensure_non_overlapping_spans(spans: impl IntoIterator<Item = ByteSpan>) -> CliResult<()> {
    let mut previous_end = None;
    for span in spans {
        let start = span.start().get();
        let end = span.end().get();
        if let Some(previous_end) = previous_end {
            if start < previous_end {
                return Err(IoRefusal::OverlappingRewriteSpans.into());
            }
        }
        previous_end = Some(end);
    }
    Ok(())
}

pub fn package_context_before_top_level(
    tree: &SyntaxTree,
    dialect: Dialect,
    target_index: usize,
) -> CliResult<Option<String>> {
    if dialect != Dialect::CommonLisp {
        return Ok(None);
    }

    let mut current_package = None;
    for index in 0..target_index {
        let path = Path::from_indexes(vec![index]);
        let view = tree.select_path(&path)?.view();
        if list_head(&view).is_some_and(|head| head.eq_ignore_ascii_case("in-package")) {
            if let Some(package_name) = atom_child(&view, 1) {
                current_package = Some(package_name.to_owned());
            }
        }
    }
    Ok(current_package)
}

#[must_use]
pub fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

#[must_use]
pub fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }

    atom_child(view, 0)
}

#[must_use]
pub fn matching_symbol_occurrences(tree: &SyntaxTree, symbol: &SymbolName) -> Vec<AtomOccurrence> {
    tree.atom_occurrences()
        .into_iter()
        // Bare quoted-symbol designators (`'foo`) are also included: they are
        // the standard idiom for referencing a symbol as data (e.g. `(error
        // 'foo ...)`, `(typep x 'foo)`), and a rename that skips them would
        // silently leave behind a reference to a definition that no longer
        // exists.
        .chain(tree.quoted_symbol_designator_occurrences())
        .filter(|occurrence| common_lisp_symbol_reference_eq(&occurrence.text, symbol.as_str()))
        .collect()
}

/// Resolves a selector into the forms it names, in source order.
///
/// The one entry point for every selector kind. Callers that can only act on
/// a single form use [`resolve_one`]; callers that can fan out
/// (`--all`) keep the whole list.
pub fn resolve_targets(
    tree: &SyntaxTree,
    dialect: Dialect,
    selector: &SelectorArgs,
) -> CliResult<Vec<SelectorTarget>> {
    let request = selector.to_request(dialect)?;
    Ok(resolve_selector(tree, dialect, &request)?)
}

/// Resolves a selector that must name exactly one whole form.
///
/// Two things are refused here rather than silently collapsed to a first
/// form, because for a command that acts on one form both would be a wrong
/// action rather than a failed one:
///
/// - a `--from`/`--to` range or a multi-form rest capture, and
/// - `--all` on a selector that matched several forms. `--all` is meaningful
///   only where the caller can fan out; passing it to a single-form command
///   would otherwise quietly discard every match but the first.
pub fn resolve_one_target(
    tree: &SyntaxTree,
    dialect: Dialect,
    selector: &SelectorArgs,
    command: &str,
) -> CliResult<SelectorTarget> {
    let request = selector.to_request(dialect)?;
    let targets = resolve_selector(tree, dialect, &request)?;
    exactly_one_target(targets, &request.describe(), command)
}

/// Resolves a [`CompactSelectorArgs`] into the one form it names.
///
/// The `--path` / `--at` / `--select` counterpart of [`resolve_one_target`],
/// for the commands whose own flags already claim `--name` and `--from`.
pub fn resolve_compact_target(
    tree: &SyntaxTree,
    dialect: Dialect,
    selector: &CompactSelectorArgs,
    command: &str,
) -> CliResult<SelectorTarget> {
    let request = selector.to_request(dialect)?;
    let targets = resolve_selector(tree, dialect, &request)?;
    exactly_one_target(targets, &request.describe(), command)
}

fn exactly_one_target(
    targets: Vec<SelectorTarget>,
    selector: &str,
    command: &str,
) -> CliResult<SelectorTarget> {
    let count = targets.len();
    let target = targets
        .into_iter()
        .next()
        // Unreachable while `resolve` refuses an empty result, which it does;
        // stated as a refusal rather than an index so the invariant cannot
        // become a panic if that ever changes.
        .ok_or_else(|| SelectorError::NoMatch {
            selector: selector.to_owned(),
        })?;
    if count > 1 {
        return Err(SelectorError::Ambiguous {
            selector: selector.to_owned(),
            count,
        }
        .into());
    }
    require_single_form(&target, command)?;
    Ok(target)
}

/// [`resolve_compact_target`] as a [`Selection`], for callers that only edit.
pub fn resolve_compact<'a>(
    tree: &'a SyntaxTree,
    dialect: Dialect,
    selector: &CompactSelectorArgs,
    command: &str,
) -> CliResult<Selection<'a>> {
    let target = resolve_compact_target(tree, dialect, selector, command)?;
    Ok(tree.select_path(&target.path)?)
}

/// [`resolve_one_target`] as a [`Selection`], for callers that only edit.
pub fn resolve_one<'a>(
    tree: &'a SyntaxTree,
    dialect: Dialect,
    selector: &SelectorArgs,
    command: &str,
) -> CliResult<Selection<'a>> {
    let target = resolve_one_target(tree, dialect, selector, command)?;
    Ok(tree.select_path(&target.path)?)
}

fn require_single_form(target: &SelectorTarget, command: &str) -> CliResult<()> {
    let count = target.form_count();
    if count > 1 {
        return Err(SelectorError::RangeUnsupported {
            command: command.to_owned(),
            count,
        }
        .into());
    }
    Ok(())
}

/// Runs one structural edit over every form the selector names.
///
/// Matches are applied **right to left**, and each application re-parses the
/// document it produced. That ordering is what makes `--all` safe without a
/// span-remapping pass: an edit never moves the text before it, so the spans
/// still to be visited are still correct. Each one is re-resolved by offset
/// and checked against the span it had, so an edit that *did* disturb a later
/// match — `slurp-forward` swallowing the next match, say — stops with a
/// refusal instead of rewriting the wrong form.
pub fn edit_target(
    args: EditTargetArgs,
    f: fn(&str, &SyntaxTree, Selection<'_>) -> SexprResult<String>,
) -> CliResult<()> {
    edit_target_with(args, f, |_| Ok(()))
}

/// [`edit_target`] with a hook that sees each selection before it is edited.
///
/// It exists for `edit kill --to-ring`: capturing what an edit removes must not
/// cost the command `--all`, the range refusal, or the shifted-match guard, and
/// resolving the selector a second time in the caller would cost all three —
/// and would read stdin twice.
///
/// The hook runs in reverse source order, the same order the edits are applied
/// in, because applying them forward would invalidate every span after the
/// first.
pub fn edit_target_with(
    args: EditTargetArgs,
    f: fn(&str, &SyntaxTree, Selection<'_>) -> SexprResult<String>,
    mut observe: impl FnMut(Selection<'_>) -> CliResult<()>,
) -> CliResult<()> {
    let target = args.target;
    let (input, dialect) = read_input_and_dialect(target.file, target.dialect)?;
    let tree = parse_document(&input, dialect)?;
    let targets = resolve_targets(&tree, dialect, &target.selector)?;
    for resolved in &targets {
        require_single_form(resolved, "this edit")?;
    }

    let mut spans = targets
        .iter()
        .map(|resolved| resolved.span)
        .collect::<Vec<_>>();
    spans.sort_by_key(ByteSpan::start);

    let mut current = input.text.clone();
    for span in spans.into_iter().rev() {
        let tree = SyntaxTree::parse_with_dialect(&current, dialect)
            .map_err(|_| IoRefusal::RewriteDoesNotReparse)?;
        let selection = tree.select_at(span.start().get())?;
        if selection.span() != span {
            return Err(ArgumentError::AllMatchShifted {
                start: span.start().get(),
            }
            .into());
        }
        observe(selection)?;
        let rewritten = f(&current, &tree, selection)?;
        current = Edit::normalize_changed_line_trivia(&current, rewritten, dialect)?;
    }

    emit_document(&input, dialect, args.write, args.diff, current)
}

/// Print the rewritten document to stdout, or with `write` persist it back to
/// the source file after confirming the result reparses with the input dialect.
/// With `diff`, stdout carries a unified diff instead of the whole document.
///
/// The rewrite always arrives with bare `\n` line endings — a whole-document
/// reformat has no other line ending to work from — so a CRLF-authored input
/// is restored to CRLF here, once, rather than in every command that produces
/// one. Without this, `format --diff` on a CRLF file would show every line as
/// changed, and `--write` would silently convert the file's line endings.
pub fn emit_document(
    input: &SourceInput,
    dialect: Dialect,
    write: bool,
    diff: bool,
    rewritten: String,
) -> CliResult<()> {
    let rewritten = restore_line_ending(&input.text, rewritten, dialect);
    if write {
        let path = require_output_file(input.file.as_ref())?.clone();
        SyntaxTree::parse_with_dialect(&rewritten, dialect)
            .map_err(|_| IoRefusal::RewriteDoesNotReparse)?;
        if diff {
            print!(
                "{}",
                crate::color::colorize_diff(
                    crate::color::Painter::stdout(),
                    &unified_diff(&path, &input.text, &rewritten)
                )
            );
        }
        return write_file_with_rollback(path, rewritten);
    }

    if diff {
        let path = input.file.clone().unwrap_or_else(|| PathBuf::from("stdin"));
        print!(
            "{}",
            crate::color::colorize_diff(
                crate::color::Painter::stdout(),
                &unified_diff(&path, &input.text, &rewritten)
            )
        );
        return Ok(());
    }

    print!("{rewritten}");
    Ok(())
}

/// Re-applies `original`'s line ending to `rewritten` when `original` is
/// CRLF-dominant.
///
/// Normalizes any `\r\n` already in `rewritten` back to a bare `\n` first, so
/// this is safe to call on a rewrite that already preserved CRLF in an
/// untouched region (a targeted edit's output) as well as one that has none
/// at all (a whole-document reformat) — either way the result ends up with
/// exactly one line ending throughout, matching `original`.
///
/// Only newlines *outside* every atom are touched. A newline inside an atom is
/// not a line ending at all, it is program data — the `\n` in a multi-line
/// string literal, or the newline that *is* the character in Common Lisp's
/// `#\<newline>` — and a whole-document substitution turns it into `\r\n`,
/// silently changing what the program means. Atom text is copied verbatim from
/// the source by every rewrite, so a CRLF file's multi-line atoms already carry
/// `\r\n` and need nothing done to them; leaving them alone is both the safe
/// choice and the correct one.
///
/// A rewrite that no longer parses is returned untouched: the write path
/// refuses it a few lines below anyway, and guessing at line endings in text
/// whose structure is unknown is exactly how the data above gets corrupted.
fn restore_line_ending(original: &str, rewritten: String, dialect: Dialect) -> String {
    if !rewritten.contains('\n') || !prefers_crlf(original) {
        return rewritten;
    }
    let Ok(tree) = SyntaxTree::parse_with_dialect(&rewritten, dialect) else {
        return rewritten;
    };
    let mut atoms = Vec::new();
    collect_atom_spans(&tree.root_view(), &mut atoms);
    atoms.sort_by_key(ByteSpan::start);

    let mut restored = String::with_capacity(rewritten.len());
    let mut cursor = 0usize;
    for atom in atoms {
        let (start, end) = (atom.start().get(), atom.end().get());
        if start < cursor {
            continue;
        }
        push_crlf_line_endings(&mut restored, &rewritten[cursor..start]);
        restored.push_str(&rewritten[start..end]);
        cursor = end;
    }
    push_crlf_line_endings(&mut restored, &rewritten[cursor..]);
    restored
}

/// Collects every atom's content span (its text past any reader prefix), in
/// document order for a well-formed tree.
fn collect_atom_spans(view: &ExpressionView, spans: &mut Vec<ByteSpan>) {
    if view.kind == ExpressionKind::Atom {
        spans.push(view.content_span);
    }
    for child in &view.children {
        collect_atom_spans(child, spans);
    }
}

/// Appends `segment` with every line ending rewritten to `\r\n`, whichever of
/// the two it arrived as.
fn push_crlf_line_endings(out: &mut String, segment: &str) {
    let bytes = segment.as_bytes();
    let mut start = 0usize;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte != b'\n' {
            continue;
        }
        let end = if index > 0 && bytes[index - 1] == b'\r' {
            index - 1
        } else {
            index
        };
        out.push_str(&segment[start..end]);
        out.push_str("\r\n");
        start = index + 1;
    }
    out.push_str(&segment[start..]);
}

/// Whether `text` uses `\r\n` line endings more often than a bare `\n`.
///
/// Strictly more, not merely as often: a file with no clear majority is left
/// exactly as the rewrite produced it rather than guessed at.
fn prefers_crlf(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut crlf = 0usize;
    let mut lone_lf = 0usize;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte != b'\n' {
            continue;
        }
        if index > 0 && bytes[index - 1] == b'\r' {
            crlf += 1;
        } else {
            lone_lf += 1;
        }
    }
    crlf > lone_lf
}

pub fn resolve_target<'a>(
    tree: &'a SyntaxTree,
    path: Option<&Path>,
    at: Option<usize>,
) -> CliResult<Selection<'a>> {
    match (path, at) {
        (Some(path), None) => Ok(tree.select_path(path)?),
        (None, Some(offset)) => Ok(tree.select_at(offset)?),
        (None, None) => Err(ArgumentError::TargetRequired.into()),
        (Some(_), Some(_)) => Err(ArgumentError::TargetAmbiguous.into()),
    }
}

/// Resolves the dialect for one input, consulting its `#lang` line.
///
/// An explicit `--dialect` always wins and a recognised extension is trusted
/// over the contents; the directive only breaks ties. That matters for stdin
/// and for the `.scm`-named Racket files that turn up in mixed projects,
/// where reading Racket as R7RS Scheme applies the wrong reader to `#:keyword`
/// literals and the wrong rules to `struct`.
///
/// After all of that, `[dialect]` in the configuration gets its say — as a
/// fallback for what detection could not place, or, with `dialect.force`, as
/// an override. It is consulted last precisely so that an explicit flag and a
/// recognised extension both still win over a file nobody re-read today.
pub fn detect_dialect(input: &SourceInput, explicit: Option<DialectArg>) -> Dialect {
    let detected =
        Dialect::detect_in_source(input.file.as_deref(), explicit.map(Into::into), &input.text);
    if explicit.is_some() {
        return detected;
    }
    crate::runtime::current().resolve_dialect(detected)
}

pub fn require_output_file(file: Option<&PathBuf>) -> CliResult<&PathBuf> {
    file.ok_or_else(|| ArgumentError::WriteRequiresFile.into())
}

/// Expands file/directory arguments into a deduplicated list of files: a
/// directory is walked for Lisp sources via workspace discovery, a file is
/// kept as-is. Argument order is preserved and duplicates (by canonical
/// path) are dropped. When `dialect` is set, unknown-extension files under a
/// directory are included, since the caller will parse them with that
/// dialect regardless of extension.
///
/// The `[paths]` configuration applies here, and only to the directory branch:
/// a file named outright on the command line is always processed. Excluding a
/// path someone typed would be the tool second-guessing an explicit request,
/// which is a different thing from bounding a directory walk.
pub fn expand_input_files(
    inputs: &[PathBuf],
    dialect: Option<DialectArg>,
) -> CliResult<Vec<PathBuf>> {
    let mut expanded = Vec::new();
    let mut seen = BTreeSet::new();
    let runtime = crate::runtime::current();

    let limits = workspace_limits();
    for input in inputs {
        if input.is_dir() {
            let discovery = discover_workspace_files_with_limits(
                &WorkspaceDiscoveryOptions {
                    roots: vec![input.clone()],
                    include_unknown: dialect.is_some(),
                    include_hidden: runtime.include_hidden,
                    include_generated: runtime.include_generated,
                    max_depth: runtime.max_depth,
                    exclude: runtime.exclude_paths.clone(),
                    // These commands take explicit paths and have no input flags of
                    // their own, so `[paths]` in `paredit.toml` and the environment
                    // are the only places a caller can say "look at the generated
                    // files too". The environment narrows what the file allowed,
                    // which is the precedence every other setting follows.
                    ignore: runtime.ignore_options(IgnoreOptions::from_environment()),
                    ..WorkspaceDiscoveryOptions::default()
                },
                limits,
            )?;
            let files = discovery.into_files();
            crate::progress::discovered(files.len(), input);
            for discovered in files {
                push_unique_path(&mut expanded, &mut seen, discovered);
            }
        } else {
            push_unique_path(&mut expanded, &mut seen, input.clone());
        }
    }

    Ok(expanded)
}

/// One file `analyze_files` could not produce a result for, and why.
#[derive(Debug, Clone)]
pub struct FileFailure {
    pub file: PathBuf,
    /// The failure's message chain, flattened the way `anyhow`'s `{:#}`
    /// flattens it — the same rendering a top-level command failure gets.
    pub message: String,
}

/// The result of analyzing a list of files: what succeeded, and what did not.
///
/// Never itself an error. A file that fails to read or parse is data a
/// report can act on — "N of M files analyzed cleanly" — not a reason to
/// discard every other file's result, which is what returning
/// `anyhow::Result<Vec<T>>` used to do: one bad file among a thousand good
/// ones threw the whole run away and told the caller nothing about the other
/// 999. Both fields preserve the input order of the files they came from.
#[derive(Debug)]
pub struct FileAnalysis<T> {
    pub succeeded: Vec<T>,
    pub failed: Vec<FileFailure>,
}

impl<T> FileAnalysis<T> {
    /// Whether nothing could be analyzed at all.
    ///
    /// The one case a caller cannot report around: if every file failed,
    /// there is no partial result to show, and the run should fail loudly
    /// rather than print an empty report that looks like a clean one.
    #[must_use]
    pub fn is_total_failure(&self) -> bool {
        self.succeeded.is_empty() && !self.failed.is_empty()
    }
}

/// The error a run whose every file failed should return.
///
/// Beside [`analyze_files`] because it is the other half of the same
/// abstraction: a per-file failure is data, right up until there is no file
/// left that succeeded, and then it is the command's failure. Names the
/// first failure by input order, the same file a fully serial run would have
/// stopped on first.
#[must_use]
pub fn total_file_failure(failures: Vec<FileFailure>) -> crate::error::FeatureRefusal {
    let first = failures
        .into_iter()
        .next()
        .expect("total failure has at least one failure");
    crate::error::FeatureRefusal::message(
        crate::diagnosis::ErrorCode::InputUnparsable,
        format!(
            "failed to analyze {}: {}",
            first.file.display(),
            first.message
        ),
    )
}

/// Notes on stderr the files [`analyze_files`] could not produce a result for,
/// without failing the command over them.
///
/// The files that *did* succeed still have a report worth producing — but a
/// report that quietly covers 1450 of 1453 files is a report that reads as
/// covering all of them. The note is on stderr because it is an aside about
/// the run rather than part of the report body, so a consumer piping stdout
/// through `jq` still gets clean JSON and a human still sees the warning.
pub fn note_partial_file_failures(failures: &[FileFailure]) {
    if failures.is_empty() {
        return;
    }
    eprintln!(
        "warning: {} of the requested files could not be analyzed and are excluded from this report:",
        failures.len()
    );
    for failure in failures {
        eprintln!("  {}: {}", failure.file.display(), failure.message);
    }
}

/// Reads, parses, and analyzes a list of files, using every available core.
///
/// The shape every multi-file report in this tool has is
/// "read, parse, analyze, collect" per file, with no dependency between files.
/// That is embarrassingly parallel and was entirely serial; on a large tree
/// the parse alone dominates, and it was being done on one core.
///
/// Three properties make this safe to substitute for the loop it replaces:
///
/// - **Order is preserved.** Results come back in input order regardless of
///   which thread finished first, so the report's bytes do not depend on
///   scheduling. That is the same-input-same-output contract, and it is the
///   reason the results are collected into pre-indexed slots rather than
///   pushed as they arrive. [`FileAnalysis::succeeded`] and
///   [`FileAnalysis::failed`] each keep that ordering independently.
/// - **Every failure is kept, not only the first.** A run over ten files
///   where files 2 and 7 both fail reports both, every time — not whichever
///   thread lost the race, and not only the earliest by input order.
/// - **One worker is the serial path.** With `--jobs 1`, or a list short
///   enough not to be worth a thread, no thread is spawned at all. A caller
///   debugging a panic gets the original stack.
///
/// `std::thread::scope` rather than a work-stealing pool: the work is a flat
/// map over a known list, and a scoped spawn needs no dependency, no runtime,
/// and no `'static` bound on the closure.
/// The closure names its own error type rather than returning `CliResult`: an
/// analysis is free to fail for reasons the I/O layer has no vocabulary for,
/// and each feature has its own. The only thing this needs of `E` is that it
/// can absorb the read failure this function itself produces, and that it can
/// be rendered — the failures are turned into [`FileFailure`] messages here
/// and never escape.
pub fn analyze_files<T, E, F>(
    files: &[PathBuf],
    dialect: Option<DialectArg>,
    analyze: F,
) -> FileAnalysis<T>
where
    T: Send,
    E: std::error::Error + From<CliError> + Send,
    F: Fn(&PathBuf, Dialect, &SyntaxTree, &SourceInput) -> Result<T, E> + Sync,
{
    let workers = worker_count(files.len());
    let results: Vec<Result<T, E>> = if workers <= 1 {
        files
            .iter()
            .map(|file| analyze_one(file, dialect, &analyze))
            .collect()
    } else {
        // Static partition into contiguous chunks. A work-stealing queue would
        // balance an uneven file-size distribution better; it would also need a
        // dependency, a mutex on the hot path, and a reason to believe the
        // imbalance costs more than the contention. Contiguous chunks of a
        // sorted file list are close to even in practice, and each worker
        // writes only its own slice — which is what makes the whole thing
        // sound without a lock.
        let mut results = files
            .iter()
            .map(|_| None::<Result<T, E>>)
            .collect::<Vec<_>>();
        let per_worker = files.len().div_ceil(workers);
        let analyze = &analyze;

        std::thread::scope(|scope| {
            for (chunk_index, slots) in results.chunks_mut(per_worker).enumerate() {
                let start = chunk_index * per_worker;
                scope.spawn(move || {
                    for (offset, slot) in slots.iter_mut().enumerate() {
                        // `files` is borrowed immutably by every worker; the
                        // mutable half is the disjoint slice each one owns.
                        if let Some(file) = files.get(start + offset) {
                            *slot = Some(analyze_one(file, dialect, analyze));
                        }
                    }
                });
            }
        });

        results
            .into_iter()
            .map(|slot| slot.expect("every slot is filled before the scope ends"))
            .collect()
    };

    let mut analysis = FileAnalysis {
        succeeded: Vec::with_capacity(results.len()),
        failed: Vec::new(),
    };
    for (file, result) in files.iter().zip(results) {
        match result {
            Ok(value) => analysis.succeeded.push(value),
            Err(error) => analysis.failed.push(FileFailure {
                file: file.clone(),
                message: crate::error::error_chain(&error),
            }),
        }
    }
    analysis
}

fn analyze_one<T, E, F>(file: &PathBuf, dialect: Option<DialectArg>, analyze: &F) -> Result<T, E>
where
    E: From<CliError>,
    F: Fn(&PathBuf, Dialect, &SyntaxTree, &SourceInput) -> Result<T, E>,
{
    let (input, resolved, tree) = read_input_dialect_and_tree(Some(file.clone()), dialect)?;
    analyze(file, resolved, &tree, &input)
}

/// How many workers to use for `count` files.
///
/// A thread costs more than parsing a handful of small files, so a short list
/// stays serial. `--jobs` is the caller's override and 0 means "as many as the
/// machine has".
fn worker_count(count: usize) -> usize {
    const PARALLEL_THRESHOLD: usize = 8;

    if count < PARALLEL_THRESHOLD {
        return 1;
    }
    let requested = paredit_core_safety::limits::effective_jobs();
    let available = if requested == 0 {
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    } else {
        requested
    };
    available.min(count).max(1)
}

/// Restates this invocation's bounds in the traversal's own vocabulary.
///
/// The two types are separate on purpose: `ResourceLimits` is what a caller
/// asked for and `WorkspaceLimits` is what a traversal enforces, and the
/// traversal has no business knowing that a command line exists. This is the
/// one place they meet.
#[must_use]
pub fn workspace_limits() -> WorkspaceLimits {
    let limits = paredit_core_safety::limits::effective();
    WorkspaceLimits {
        max_roots: limits.max_roots,
        max_entries: limits.max_entries,
        max_files: limits.max_files,
        max_file_bytes: limits.max_file_bytes,
        max_total_bytes: limits.max_total_bytes,
    }
}

fn push_unique_path(expanded: &mut Vec<PathBuf>, seen: &mut BTreeSet<PathBuf>, path: PathBuf) {
    let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if seen.insert(canonical) {
        expanded.push(path);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{
        CliError, CliResult, Dialect, PathBuf, analyze_files, prefers_crlf, require_output_file,
        restore_line_ending, terminal_safe, terminal_safe_error_chain,
    };

    fn restore(original: &str, rewritten: &str) -> String {
        restore_line_ending(original, rewritten.to_owned(), Dialect::CommonLisp)
    }

    static TEST_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// A file under a fresh temp directory, so parallel test binaries never
    /// collide.
    fn test_file(name: &str, content: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "paredit-cli-shared-{name}-{}-{}",
            std::process::id(),
            TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).expect("create test directory");
        let path = directory.join(format!("{name}.lisp"));
        std::fs::write(&path, content).expect("write test file");
        path
    }

    #[test]
    fn analyze_files_partitions_successes_and_failures_by_input_order() {
        let files = vec![
            test_file("a", "(defun a () 1)"),
            test_file("b", "(defun b (\n"), // unclosed: fails
            test_file("c", "(defun c () 3)"),
        ];

        let analysis = analyze_files(&files, None, |file, _dialect, _tree, _input| {
            CliResult::Ok(file.file_stem().unwrap().to_string_lossy().into_owned())
        });

        assert!(!analysis.is_total_failure());
        assert_eq!(analysis.succeeded, vec!["a".to_owned(), "c".to_owned()]);
        assert_eq!(analysis.failed.len(), 1);
        assert_eq!(analysis.failed[0].file, files[1]);
        assert!(
            analysis.failed[0].message.contains("unclosed list"),
            "{}",
            analysis.failed[0].message
        );
    }

    #[test]
    fn analyze_files_reports_every_failure_not_only_the_first() {
        let files = vec![
            test_file("first-broken", "(defun a (\n"),
            test_file("clean", "(defun b () 2)"),
            test_file("second-broken", "(defun c (\n"),
        ];

        let analysis = analyze_files(&files, None, |_file, _dialect, _tree, _input| {
            CliResult::Ok(())
        });

        assert_eq!(analysis.succeeded, vec![()]);
        assert_eq!(analysis.failed.len(), 2);
        assert_eq!(analysis.failed[0].file, files[0]);
        assert_eq!(analysis.failed[1].file, files[2]);
    }

    #[test]
    fn analyze_files_is_a_total_failure_only_when_nothing_succeeded() {
        let all_broken = vec![
            test_file("total-a", "(defun a (\n"),
            test_file("total-b", "(defun b (\n"),
        ];
        let analysis = analyze_files(&all_broken, None, |_file, _dialect, _tree, _input| {
            CliResult::Ok(())
        });
        assert!(analysis.is_total_failure());

        let all_clean = vec![test_file("clean-only", "(defun a () 1)")];
        let analysis = analyze_files(&all_clean, None, |_file, _dialect, _tree, _input| {
            CliResult::Ok(())
        });
        assert!(!analysis.is_total_failure());

        let empty: Vec<PathBuf> = Vec::new();
        let analysis = analyze_files(&empty, None, |_file, _dialect, _tree, _input| {
            CliResult::Ok(())
        });
        assert!(!analysis.is_total_failure());
    }

    /// The same partitioning holds on the parallel path — the worker
    /// threshold is small enough to exercise from a unit test without a
    /// slow fixture.
    #[test]
    fn analyze_files_partitions_correctly_on_the_parallel_path() {
        let files: Vec<PathBuf> = (0..10)
            .map(|index| {
                if index == 4 {
                    test_file(&format!("parallel-broken-{index}"), "(defun f (\n")
                } else {
                    test_file(&format!("parallel-ok-{index}"), "(defun f () 1)")
                }
            })
            .collect();

        let analysis = analyze_files(&files, None, |_file, _dialect, _tree, _input| {
            CliResult::Ok(())
        });

        assert_eq!(analysis.succeeded.len(), 9);
        assert_eq!(analysis.failed.len(), 1);
        assert_eq!(analysis.failed[0].file, files[4]);
    }

    #[test]
    fn require_output_file_rejects_missing_file() {
        let error = require_output_file(None).unwrap_err();
        assert_eq!(error.to_string(), "--write requires --file");
    }

    #[test]
    fn a_crlf_majority_is_detected_even_with_one_stray_lf() {
        assert!(prefers_crlf("(a)\r\n(b)\r\n(c)\n"));
    }

    #[test]
    fn a_pure_lf_document_does_not_prefer_crlf() {
        assert!(!prefers_crlf("(a)\n(b)\n"));
    }

    #[test]
    fn a_tie_does_not_prefer_crlf() {
        assert!(!prefers_crlf("(a)\r\n(b)\n"));
    }

    #[test]
    fn a_single_line_document_has_no_preference() {
        assert!(!prefers_crlf("(a)"));
    }

    #[test]
    fn restore_line_ending_converts_a_bare_lf_rewrite_back_to_crlf() {
        assert_eq!(restore("(a)\r\n(b)\r\n", "(a)\n(b)\n"), "(a)\r\n(b)\r\n");
    }

    #[test]
    fn restore_line_ending_does_not_double_convert_an_already_crlf_rewrite() {
        // A targeted edit's rewrite can already carry CRLF in an untouched
        // region; restore_line_ending must not turn that into `\r\r\n`.
        assert_eq!(
            restore("(a)\r\n(b)\r\n", "(a)\r\n(new)\n"),
            "(a)\r\n(new)\r\n"
        );
    }

    #[test]
    fn restore_line_ending_leaves_an_lf_document_untouched() {
        let rewritten = "(a)\n(c)\n";
        assert_eq!(restore("(a)\n(b)\n", rewritten), rewritten);
    }

    #[test]
    fn restore_line_ending_leaves_a_newline_inside_a_string_literal_alone() {
        // The `\n` between `one` and `two` is the string's own content, not a
        // line ending: turning it into `\r\n` changes the value the program
        // computes. Only the two real line endings may be converted.
        let original = "(defun f ()\r\n  \"line one\nline two\")\r\n";
        assert_eq!(
            restore(original, "(defun f ()\n  \"line one\nline two\")\n"),
            "(defun f ()\r\n  \"line one\nline two\")\r\n"
        );
    }

    #[test]
    fn restore_line_ending_leaves_a_newline_character_literal_alone() {
        // `#\` followed by a newline byte *is* the newline character in Common
        // Lisp; rewriting it to `\r\n` makes it the return character instead.
        let original = "(list #\\\n 1)\r\n(b)\r\n";
        assert_eq!(
            restore(original, "(list #\\\n 1)\n(b)\n"),
            "(list #\\\n 1)\r\n(b)\r\n"
        );
    }

    #[test]
    fn restore_line_ending_returns_an_unparseable_rewrite_untouched() {
        let rewritten = "(a\n";
        assert_eq!(restore("(a)\r\n(b)\r\n", rewritten), rewritten);
    }

    #[test]
    fn terminal_safe_escapes_record_and_display_controls() {
        let value = "safe\0\n\r\t\u{1b}\u{7f}\u{85}\u{61c}\u{200e}\u{200f}\u{2028}\u{202e}\u{2066}\u{2069}終";

        assert_eq!(
            terminal_safe(value).to_string(),
            "safe\\u{0}\\u{a}\\u{d}\\u{9}\\u{1b}\\u{7f}\\u{85}\\u{61c}\\u{200e}\\u{200f}\\u{2028}\\u{202e}\\u{2066}\\u{2069}終"
        );
    }

    #[test]
    fn terminal_safe_error_chain_escapes_each_context_as_one_value() {
        // A two-level typed chain, standing in for what
        // `anyhow!("leaf").context("context")` used to build here: the point
        // of the test is that each link is escaped as its own value and the
        // `": "` joiner survives, not which library produced the nesting.
        let error = CliError::Io {
            context: "context\t\u{1b}".to_owned(),
            source: std::io::Error::other("leaf\n\u{202e}"),
        };

        assert_eq!(
            terminal_safe_error_chain(&error).to_string(),
            "context\\u{9}\\u{1b}: leaf\\u{a}\\u{202e}"
        );
    }
}
