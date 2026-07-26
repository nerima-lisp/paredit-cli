use super::*;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Path, SymbolName, SyntaxTree};

fn selected_form(input: &str) -> paredit_core_syntax::sexpr::ExpressionView {
    let tree = SyntaxTree::parse(input).expect("parse");
    tree.select_path(&"0".parse::<Path>().expect("path"))
        .expect("select")
        .view()
}

fn selected_form_with_dialect(
    input: &str,
    dialect: Dialect,
) -> paredit_core_syntax::sexpr::ExpressionView {
    let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
    tree.select_path(&"0".parse::<Path>().expect("path"))
        .expect("select")
        .view()
}

fn reference_texts(input: &str, symbol: &str) -> Vec<String> {
    let view = selected_form(input);
    let symbol = SymbolName::new(symbol).expect("symbol");
    let mut spans = Vec::new();
    collect_unshadowed_symbol_references(Dialect::CommonLisp, &view, &symbol, input, &mut spans);
    spans
        .into_iter()
        .map(|span| span.slice(input).to_owned())
        .collect()
}

fn reference_texts_with_dialect(input: &str, dialect: Dialect, symbol: &str) -> Vec<String> {
    let view = selected_form_with_dialect(input, dialect);
    let symbol = SymbolName::new(symbol).expect("symbol");
    let mut spans = Vec::new();
    collect_unshadowed_symbol_references(dialect, &view, &symbol, input, &mut spans);
    spans
        .into_iter()
        .map(|span| span.slice(input).to_owned())
        .collect()
}

mod binding_forms;
mod boundaries;
mod capture;
mod property;
mod shadowing;
