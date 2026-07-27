pub(super) use super::*;

fn symbol_names(names: &[&str]) -> Vec<SymbolName> {
    names.iter().map(|name| symbol(name)).collect()
}

fn assert_parameter_order(actual: &[SymbolName], expected: &[&str]) {
    assert_eq!(
        actual.iter().map(SymbolName::as_str).collect::<Vec<_>>(),
        expected
    );
}

fn assert_reordered_arguments(actual: &[Vec<String>], expected: &[&[&str]]) {
    assert_eq!(
        actual,
        &expected
            .iter()
            .map(|arguments| arguments
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect())
            .collect::<Vec<Vec<String>>>()
    );
}

fn assert_swapped_arguments(actual: &[(String, String)], expected: &[(&str, &str)]) {
    assert_eq!(
        actual,
        &expected
            .iter()
            .map(|(left, right)| ((*left).to_owned(), (*right).to_owned()))
            .collect::<Vec<_>>()
    );
}

mod basic;
mod common_lisp;
mod failure;
