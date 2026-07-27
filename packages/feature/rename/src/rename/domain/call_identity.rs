use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;

pub fn call_reference_eq(dialect: Dialect, candidate: &str, expected: &str) -> bool {
    match dialect {
        Dialect::CommonLisp => common_lisp_symbol_reference_eq(candidate, expected),
        Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Clojure
        | Dialect::Janet
        | Dialect::Fennel => candidate == expected,
        Dialect::Unknown => false,
    }
}

pub fn is_local_call_bound(dialect: Dialect, local_callables: &[String], expected: &str) -> bool {
    local_callables
        .iter()
        .rev()
        .any(|candidate| call_reference_eq(dialect, candidate, expected))
}
