use super::*;
use crate::domain::dialect::Dialect;

fn reference_texts_for(dialect: Dialect, input: &str, symbol: &str) -> Vec<String> {
    let view = selected_form(input);
    let symbol = SymbolName::new(symbol).expect("symbol");
    let mut spans = Vec::new();
    collect_unshadowed_symbol_references(dialect, &view, &symbol, input, &mut spans);
    spans
        .into_iter()
        .map(|span| span.slice(input).to_owned())
        .collect()
}

#[test]
fn scheme_named_let_preserves_outer_references_outside_local_callable_body() {
    let input = "(list loop (let loop ((value loop)) (loop value)) loop)";

    assert_eq!(
        reference_texts_for(Dialect::Scheme, input, "loop"),
        vec!["loop", "loop", "loop"]
    );
}

#[test]
fn scheme_named_let_star_preserves_outer_references_in_binding_inits() {
    let input = "(list outer (let* loop ((value outer) (copy value)) (list copy)) outer)";

    assert_eq!(
        reference_texts_for(Dialect::Scheme, input, "outer"),
        vec!["outer", "outer", "outer"]
    );
}
