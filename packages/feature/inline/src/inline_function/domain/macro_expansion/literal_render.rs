use anyhow::Result;

use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

pub(super) fn render_unquoted_source(view: &ExpressionView) -> Result<String> {
    Ok(render_literal_expression_from(view, 1))
}

fn render_core_literal(view: &ExpressionView) -> String {
    match view.kind {
        ExpressionKind::Atom => strip_reader_prefix_source(view),
        ExpressionKind::List => render_list(
            view.delimiter.unwrap_or(Delimiter::Paren),
            view.children
                .iter()
                .map(render_literal_expression)
                .collect(),
        ),
        ExpressionKind::Root => String::new(),
    }
}

pub(super) fn render_literal_expression(view: &ExpressionView) -> String {
    render_literal_expression_from(view, 0)
}

fn render_literal_expression_from(view: &ExpressionView, prefix_index: usize) -> String {
    let mut rendered = String::new();
    for prefix in view.reader_prefixes.iter().skip(prefix_index) {
        rendered.push_str(prefix.as_source());
    }
    rendered.push_str(&render_core_literal(view));
    rendered
}

pub(super) fn render_list(delimiter: Delimiter, children: Vec<String>) -> String {
    let (open, close) = match delimiter {
        Delimiter::Paren => ('(', ')'),
        Delimiter::Bracket => ('[', ']'),
        Delimiter::Brace => ('{', '}'),
    };
    format!("{}{}{}", open, children.join(" "), close)
}

fn strip_reader_prefix_source(view: &ExpressionView) -> String {
    let Some(text) = &view.text else {
        return String::new();
    };
    let prefix_len: usize = view
        .reader_prefixes
        .iter()
        .map(|prefix| prefix.as_source().len())
        .sum();
    text.get(prefix_len..).unwrap_or(text).to_owned()
}
