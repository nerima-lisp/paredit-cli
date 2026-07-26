//! Which dialects the type layer runs for.

use crate::domain::dialect::Dialect;

/// Whether type inference is enabled for `dialect`.
///
/// Common Lisp only, and deliberately so. The lattice, the declaration forms
/// (`the`, `check-type`, `declare`), and the standard-function return types
/// are all CLHS-specific; applying them to Scheme or Clojure would be
/// borrowing a type system those dialects do not have. Other dialects get an
/// empty table, so every rule that consumes types stays silent there.
pub const fn supports_type_inference(dialect: Dialect) -> bool {
    matches!(dialect, Dialect::CommonLisp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_common_lisp_is_enabled() {
        assert!(supports_type_inference(Dialect::CommonLisp));
        for dialect in [
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Racket,
            Dialect::Clojure,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            assert!(!supports_type_inference(dialect), "{dialect:?}");
        }
    }
}
