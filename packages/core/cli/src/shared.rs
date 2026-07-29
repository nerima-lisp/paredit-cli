use crate::error::{ArgumentError, CliResult, IoRefusal};
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
    IgnoreOptions, WorkspaceDiscoveryOptions, discover_workspace_files,
};

#[path = "diff.rs"]
mod diff;
#[path = "io.rs"]
mod io;
#[cfg(target_os = "macos")]
#[path = "macos_acl.rs"]
mod macos_acl;

pub use diff::unified_diff;
pub use io::{AnchoredExpectedWrite, write_files_with_rollback_expected_anchored};
pub use io::{
    ExpectedWriteTarget, MAX_SOURCE_INPUT_BYTES, parse_document, read_file_or_empty,
    read_input_and_dialect, read_input_dialect_and_tree, read_text_file_with_expected_target,
    read_text_file_with_limit, read_text_with_limit, write_artifact_with_rollback,
    write_file_with_rollback, write_files_with_rollback, write_files_with_rollback_expected,
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

#[must_use]
pub const fn terminal_safe_error_chain(error: &anyhow::Error) -> TerminalSafeErrorChain<'_> {
    TerminalSafeErrorChain(error)
}

// Public since the extraction: this was crate-internal, a visibility that
// cannot cross a crate boundary, so the lint applies to it for the first time.
#[derive(Debug)]
pub struct TerminalSafeErrorChain<'a>(&'a anyhow::Error);

impl Display for TerminalSafeErrorChain<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(TerminalEscapeWriter(formatter), "{:#}", self.0)
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

#[must_use]
pub fn stable_text_hash(text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

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
pub fn emit_document(
    input: &SourceInput,
    dialect: Dialect,
    write: bool,
    diff: bool,
    rewritten: String,
) -> CliResult<()> {
    if write {
        let path = require_output_file(input.file.as_ref())?.clone();
        SyntaxTree::parse_with_dialect(&rewritten, dialect)
            .map_err(|_| IoRefusal::RewriteDoesNotReparse)?;
        if diff {
            print!("{}", unified_diff(&path, &input.text, &rewritten));
        }
        return write_file_with_rollback(path, rewritten);
    }

    if diff {
        let path = input.file.clone().unwrap_or_else(|| PathBuf::from("stdin"));
        print!("{}", unified_diff(&path, &input.text, &rewritten));
        return Ok(());
    }

    print!("{rewritten}");
    Ok(())
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
pub fn detect_dialect(input: &SourceInput, explicit: Option<DialectArg>) -> Dialect {
    Dialect::detect_in_source(input.file.as_deref(), explicit.map(Into::into), &input.text)
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
pub fn expand_input_files(
    inputs: &[PathBuf],
    dialect: Option<DialectArg>,
) -> CliResult<Vec<PathBuf>> {
    let mut expanded = Vec::new();
    let mut seen = BTreeSet::new();

    for input in inputs {
        if input.is_dir() {
            let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
                roots: vec![input.clone()],
                include_unknown: dialect.is_some(),
                include_hidden: false,
                include_generated: false,
                max_depth: None,
                exclude: Vec::new(),
                // These commands take explicit paths and have no input flags of
                // their own, so the environment is the only place a caller can
                // say "look at the generated files too".
                ignore: IgnoreOptions::from_environment(),
                ..WorkspaceDiscoveryOptions::default()
            })?;
            for discovered in discovery.into_files() {
                push_unique_path(&mut expanded, &mut seen, discovered);
            }
        } else {
            push_unique_path(&mut expanded, &mut seen, input.clone());
        }
    }

    Ok(expanded)
}

fn push_unique_path(expanded: &mut Vec<PathBuf>, seen: &mut BTreeSet<PathBuf>, path: PathBuf) {
    let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if seen.insert(canonical) {
        expanded.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::{require_output_file, terminal_safe, terminal_safe_error_chain};

    #[test]
    fn require_output_file_rejects_missing_file() {
        let error = require_output_file(None).unwrap_err();
        assert_eq!(error.to_string(), "--write requires --file");
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
        let error = anyhow::anyhow!("leaf\n\u{202e}").context("context\t\u{1b}");

        assert_eq!(
            terminal_safe_error_chain(&error).to_string(),
            "context\\u{9}\\u{1b}: leaf\\u{a}\\u{202e}"
        );
    }
}
