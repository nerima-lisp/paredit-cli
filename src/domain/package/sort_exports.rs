use anyhow::Result;

use crate::domain::{
    dialect::Dialect,
    sexpr::{ByteOffset, ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree},
};

use super::{
    syntax::{atom_text, package_option_name},
    visit::visit_defpackage_forms,
};
use crate::domain::leading_trivia::first_newline_or;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExportSortEdit {
    pub(super) defpackage_path: String,
    pub(super) defpackage_span: ByteSpan,
    pub(super) package_name: String,
    pub(super) export_path: String,
    pub(super) export_span: ByteSpan,
    pub(super) old_symbols: Vec<String>,
    pub(super) new_symbols: Vec<String>,
    pub(super) replacements: Vec<ExportSymbolReplacement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExportSymbolReplacement {
    pub(super) span: ByteSpan,
    pub(super) replacement: String,
}

pub(super) fn defpackage_export_sort_edits(
    input: &str,
    tree: &SyntaxTree,
    dialect: Dialect,
    package: Option<&SymbolName>,
) -> Result<Vec<ExportSortEdit>> {
    let mut edits = Vec::new();
    visit_defpackage_forms(tree, dialect, package, |view, path, package_name| {
        analyze_defpackage_exports(input, view, path, package_name, &mut edits)
    })?;

    Ok(edits)
}

fn analyze_defpackage_exports(
    input: &str,
    view: &ExpressionView,
    path: &Path,
    package_name: &str,
    edits: &mut Vec<ExportSortEdit>,
) -> Result<()> {
    for (option_index, option) in view.children.iter().enumerate().skip(2) {
        if option.kind != ExpressionKind::List || option.children.is_empty() {
            continue;
        }
        let Some(option_head) = atom_text(&option.children[0]) else {
            continue;
        };
        if package_option_name(option_head) != "export" {
            continue;
        }

        edits.push(analyze_export_option(
            input,
            option,
            path,
            &path.child(option_index),
            package_name,
            view.span,
        )?);
    }

    Ok(())
}

fn analyze_export_option(
    input: &str,
    option: &ExpressionView,
    defpackage_path: &Path,
    option_path: &Path,
    package_name: &str,
    defpackage_span: ByteSpan,
) -> Result<ExportSortEdit> {
    let mut symbols = Vec::new();

    for child in option.children.iter().skip(1) {
        let Some(symbol) = atom_text(child) else {
            anyhow::bail!(
                "cannot sort :export option at {option_path}; only atom symbol designators are supported"
            );
        };
        symbols.push((child.span, symbol.to_owned()));
    }

    let old_symbols = symbols
        .iter()
        .map(|(_, symbol)| symbol.clone())
        .collect::<Vec<_>>();

    // A `;; section` comment (or an own-line comment) that precedes an export
    // symbol is that symbol's leading trivia; a trailing comment on the same
    // line belongs to the symbol it follows. Each slot therefore spans from the
    // newline that ends the previous entry's line up to the next such newline,
    // so the comment travels with its symbol when the sort reorders entries.
    let head_end = option.children[0].span.end().get();
    let slots = build_export_slots(input, head_end, &symbols);

    let mut order = (0..symbols.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| {
        let left_symbol = symbols[left].1.as_str();
        let right_symbol = symbols[right].1.as_str();
        normalized_sort_key(left_symbol)
            .cmp(&normalized_sort_key(right_symbol))
            .then_with(|| left_symbol.cmp(right_symbol))
    });

    let new_symbols = order
        .iter()
        .map(|&index| symbols[index].1.clone())
        .collect::<Vec<_>>();

    let replacements = if let Some(slots) = slots.filter(|_| !is_identity(&order)) {
        let region = ByteSpan::new(slots[0].start(), symbols[symbols.len() - 1].0.end());
        let mut replacement = order
            .iter()
            .map(|&index| slots[index].slice(input))
            .collect::<String>();
        // If the reorder leaves a trailing same-line comment as the final entry,
        // the delimiters that follow the region (`)` ...) would be pulled onto the
        // comment line and commented out. Push them onto a fresh, indented line.
        if ends_with_open_line_comment(&replacement) {
            let indent = last_line_indent(&replacement);
            replacement.push('\n');
            replacement.push_str(&indent);
        }
        vec![ExportSymbolReplacement {
            span: region,
            replacement,
        }]
    } else {
        Vec::new()
    };

    Ok(ExportSortEdit {
        defpackage_path: defpackage_path.to_string(),
        defpackage_span,
        package_name: package_name.to_owned(),
        export_path: option_path.to_string(),
        export_span: option.span,
        old_symbols,
        new_symbols,
        replacements,
    })
}

/// Splits the export option body into one contiguous, gap-free slice per
/// symbol. Returns `None` when the option has no symbols. Each slot begins at
/// the newline that ends the previous entry's line (or the previous token when
/// the entries share a line) so leading comments move with the symbol below
/// them and trailing same-line comments stay with the symbol above them.
fn build_export_slots(
    input: &str,
    head_end: usize,
    symbols: &[(ByteSpan, String)],
) -> Option<Vec<ByteSpan>> {
    if symbols.is_empty() {
        return None;
    }

    let mut starts = Vec::with_capacity(symbols.len());
    for (index, (span, _)) in symbols.iter().enumerate() {
        let previous_end = if index == 0 {
            head_end
        } else {
            symbols[index - 1].0.end().get()
        };
        let this_start = span.start().get();
        starts.push(first_newline_or(input, previous_end, this_start));
    }

    let slots = starts
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = if index + 1 < starts.len() {
                starts[index + 1]
            } else {
                symbols[index].0.end().get()
            };
            ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
        })
        .collect();
    Some(slots)
}

fn is_identity(order: &[usize]) -> bool {
    order
        .iter()
        .enumerate()
        .all(|(index, &value)| index == value)
}

/// Reports whether the final line of `text` carries a line comment that is not
/// closed by a newline, meaning any following delimiter would be commented out.
fn ends_with_open_line_comment(text: &str) -> bool {
    let last_line = text.rsplit('\n').next().unwrap_or(text);
    last_line.contains(';')
}

/// Returns the leading whitespace of the final line of `text`.
fn last_line_indent(text: &str) -> String {
    let last_line = text.rsplit('\n').next().unwrap_or(text);
    last_line
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect()
}

fn normalized_sort_key(symbol: &str) -> String {
    symbol
        .strip_prefix("#:")
        .or_else(|| symbol.strip_prefix(':'))
        .unwrap_or(symbol)
        .to_ascii_lowercase()
}
