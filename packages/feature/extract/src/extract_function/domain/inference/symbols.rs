use crate::domain::sexpr::SymbolName;

pub(super) fn is_extract_function_param_candidate(text: &str) -> bool {
    if text.is_empty()
        || text.starts_with(':')
        || text.starts_with('"')
        || text.starts_with('&')
        || matches!(text, "nil" | "t" | "true" | "false")
        || looks_like_numeric_literal(text)
    {
        return false;
    }

    SymbolName::new(text).is_ok()
}

fn looks_like_numeric_literal(text: &str) -> bool {
    text.parse::<f64>().is_ok()
        || text.chars().all(|character| {
            character.is_ascii_digit() || matches!(character, '+' | '-' | '.' | '/')
        })
}
