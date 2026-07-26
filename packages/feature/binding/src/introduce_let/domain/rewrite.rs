use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, SymbolName};

pub(super) fn introduced_let(
    dialect: Dialect,
    name: &SymbolName,
    value: &str,
    body: &str,
) -> String {
    match dialect {
        Dialect::Clojure | Dialect::Hy | Dialect::Carp | Dialect::Janet | Dialect::Fennel => {
            format!("(let [{} {}] {})", name.as_str(), value, body)
        }
        Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Unknown => {
            format!("(let (({} {})) {})", name.as_str(), value, body)
        }
    }
}

pub(super) fn replace_spans_within_span(
    input: &str,
    container_span: ByteSpan,
    spans: &[ByteSpan],
    replacement: &str,
) -> String {
    let container_start = container_span.start().get();
    let mut output = container_span.slice(input).to_owned();
    let mut sorted = spans.to_vec();
    sorted.sort_by_key(|span| span.start());
    for span in sorted.into_iter().rev() {
        let start = span.start().get() - container_start;
        let end = span.end().get() - container_start;
        output.replace_range(start..end, replacement);
    }
    output
}

pub(super) fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() - span.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}
