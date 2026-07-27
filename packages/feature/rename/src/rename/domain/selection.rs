use anyhow::Result;

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
pub(super) use crate::domain::sexpr::reader::atom_text;
use crate::domain::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Selection, SymbolName, SyntaxTree,
};

use super::RenameTarget;

pub(super) fn select_rename_target<'a>(
    tree: &'a SyntaxTree,
    target: &RenameTarget,
) -> Result<Selection<'a>> {
    match target {
        RenameTarget::Path(path) => tree.select_path(path),
        RenameTarget::Offset(offset) => tree.select_at(*offset),
    }
}

pub(super) fn collect_symbol_atom_spans(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
) {
    if atom_text(view).is_some_and(|text| common_lisp_symbol_reference_eq(text, symbol.as_str())) {
        output.push(view.span);
        return;
    }
    for child in &view.children {
        collect_symbol_atom_spans(child, symbol, output);
    }
}

pub(super) fn apply_byte_span_edits(
    input: &str,
    mut edits: Vec<(ByteSpan, String)>,
) -> Result<String> {
    edits.sort_by_key(|(span, _)| span.start());
    ensure_non_overlapping_spans(edits.iter().map(|(span, _)| *span))?;

    let mut output = input.to_owned();
    for (span, replacement) in edits.into_iter().rev() {
        output.replace_range(span.as_range(), &replacement);
    }
    Ok(output)
}

fn ensure_non_overlapping_spans(spans: impl IntoIterator<Item = ByteSpan>) -> Result<()> {
    let mut previous: Option<ByteSpan> = None;
    for span in spans {
        if let Some(previous) = previous {
            if previous.end().get() > span.start().get() {
                anyhow::bail!(
                    "overlapping edits at {}..{} and {}..{}",
                    previous.start().get(),
                    previous.end().get(),
                    span.start().get(),
                    span.end().get()
                );
            }
        }
        previous = Some(span);
    }
    Ok(())
}

pub(super) fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }

    atom_child(view, 0)
}

pub(super) fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

pub(super) trait SpannedCallSite {
    fn span(&self) -> ByteSpan;
}

/// Keeps only the outermost call site among any that nest inside one
/// another (e.g. `(foo (foo x))`), since rewriting the outer site already
/// rewrites everything nested inside its span.
pub(super) fn select_outermost_call_sites<T: SpannedCallSite>(
    mut candidates: Vec<T>,
) -> (Vec<T>, Vec<T>) {
    candidates.sort_by_key(|site| {
        (
            site.span().start().get(),
            std::cmp::Reverse(site.span().len()),
        )
    });

    let mut selected: Vec<T> = Vec::new();
    let mut skipped_nested = Vec::new();
    for site in candidates {
        if selected.iter().any(|selected: &T| {
            selected.span().contains_span(site.span()) && selected.span() != site.span()
        }) {
            skipped_nested.push(site);
        } else {
            selected.push(site);
        }
    }
    selected.sort_by_key(|site| site.span().start());
    skipped_nested.sort_by_key(|site| site.span().start());
    (selected, skipped_nested)
}
