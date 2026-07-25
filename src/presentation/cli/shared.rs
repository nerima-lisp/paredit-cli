use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fmt::{self, Display, Write};
use std::path::PathBuf;

use super::{DialectArg, EditTargetArgs, SourceInput};
use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{
    AtomOccurrence, ByteSpan, Delimiter, Edit, ExpressionKind, ExpressionView, Path, Selection,
    SymbolName, SyntaxTree,
};
use crate::infrastructure::workspace::{WorkspaceDiscoveryOptions, discover_workspace_files};

#[path = "diff.rs"]
mod diff;
#[path = "io.rs"]
mod io;
#[cfg(target_os = "macos")]
#[path = "macos_acl.rs"]
mod macos_acl;

pub(crate) use diff::unified_diff;
pub(crate) use io::{AnchoredExpectedWrite, write_files_with_rollback_expected_anchored};
pub(crate) use io::{
    ExpectedWriteTarget, MAX_SOURCE_INPUT_BYTES, parse_document, read_file_or_empty,
    read_input_and_dialect, read_input_dialect_and_tree, read_text_file_with_expected_target,
    read_text_file_with_limit, read_text_with_limit, write_artifact_with_rollback,
    write_file_with_rollback, write_files_with_rollback, write_files_with_rollback_expected,
};

pub(crate) fn terminal_safe<T: Display>(value: T) -> TerminalSafe<T> {
    TerminalSafe(value)
}

pub(crate) struct TerminalSafe<T>(T);

impl<T: Display> Display for TerminalSafe<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(TerminalEscapeWriter(formatter), "{}", self.0)
    }
}

pub(crate) fn terminal_safe_error_chain(error: &anyhow::Error) -> TerminalSafeErrorChain<'_> {
    TerminalSafeErrorChain(error)
}

pub(crate) struct TerminalSafeErrorChain<'a>(&'a anyhow::Error);

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

fn is_terminal_control(character: char) -> bool {
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

pub(crate) fn apply_byte_span_edits(
    input: &str,
    mut edits: Vec<(ByteSpan, String)>,
) -> Result<String> {
    for (span, _) in &edits {
        span.validate_against(input)
            .context("rewrite span is outside input or not UTF-8 aligned")?;
    }
    edits.sort_by_key(|(span, _)| span.start());
    ensure_non_overlapping_spans(edits.iter().map(|(span, _)| *span))?;

    let mut output = input.to_owned();
    for (span, replacement) in edits.into_iter().rev() {
        output.replace_range(span.as_range(), &replacement);
    }
    Ok(output)
}

pub(crate) fn stable_text_hash(text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

pub(crate) fn bounded_preview(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_owned();
    }

    let mut end = max_bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

fn ensure_non_overlapping_spans(spans: impl IntoIterator<Item = ByteSpan>) -> Result<()> {
    let mut previous_end = None;
    for span in spans {
        let start = span.start().get();
        let end = span.end().get();
        if let Some(previous_end) = previous_end {
            if start < previous_end {
                anyhow::bail!("refusing overlapping rewrite spans");
            }
        }
        previous_end = Some(end);
    }
    Ok(())
}

pub(crate) fn package_context_before_top_level(
    tree: &SyntaxTree,
    dialect: Dialect,
    target_index: usize,
) -> Result<Option<String>> {
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

pub(crate) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub(crate) fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

pub(crate) fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }

    atom_child(view, 0)
}

pub(crate) fn matching_symbol_occurrences(
    tree: &SyntaxTree,
    symbol: &SymbolName,
) -> Vec<AtomOccurrence> {
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

pub(crate) fn edit_target(
    args: EditTargetArgs,
    f: fn(&str, &SyntaxTree, Selection<'_>) -> Result<String>,
) -> Result<()> {
    let target = args.target;
    let (input, dialect) = read_input_and_dialect(target.file, target.dialect)?;
    let tree = parse_document(&input, dialect)?;
    let selection = resolve_target(&tree, target.path.as_ref(), target.at)?;
    let rewritten = f(&input.text, &tree, selection)?;
    let rewritten = Edit::normalize_changed_line_trivia(&input.text, rewritten, dialect)?;
    emit_document(&input, dialect, args.write, args.diff, rewritten)
}

/// Print the rewritten document to stdout, or with `write` persist it back to
/// the source file after confirming the result reparses with the input dialect.
/// With `diff`, stdout carries a unified diff instead of the whole document.
pub(crate) fn emit_document(
    input: &SourceInput,
    dialect: Dialect,
    write: bool,
    diff: bool,
    rewritten: String,
) -> Result<()> {
    if write {
        let path = require_output_file(input.file.as_ref())?.clone();
        SyntaxTree::parse_with_dialect(&rewritten, dialect)
            .context("refusing to write: rewritten source does not reparse")?;
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

pub(crate) fn resolve_target<'a>(
    tree: &'a SyntaxTree,
    path: Option<&Path>,
    at: Option<usize>,
) -> Result<Selection<'a>> {
    match (path, at) {
        (Some(path), None) => tree.select_path(path),
        (None, Some(offset)) => tree.select_at(offset),
        (None, None) => anyhow::bail!("target required: pass --path or --at"),
        (Some(_), Some(_)) => anyhow::bail!("pass only one of --path or --at"),
    }
}

pub(crate) fn detect_dialect(input: &SourceInput, explicit: Option<DialectArg>) -> Dialect {
    Dialect::detect(input.file.as_deref(), explicit.map(Into::into))
}

pub(crate) fn require_output_file(file: Option<&PathBuf>) -> Result<&PathBuf> {
    file.context("--write requires --file")
}

/// Expands file/directory arguments into a deduplicated list of files: a
/// directory is walked for Lisp sources via workspace discovery, a file is
/// kept as-is. Argument order is preserved and duplicates (by canonical
/// path) are dropped. When `dialect` is set, unknown-extension files under a
/// directory are included, since the caller will parse them with that
/// dialect regardless of extension.
pub(crate) fn expand_input_files(
    inputs: &[PathBuf],
    dialect: Option<DialectArg>,
) -> Result<Vec<PathBuf>> {
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
