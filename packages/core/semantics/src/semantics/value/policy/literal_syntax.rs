//! How each dialect spells the literals that are not just numbers.

use crate::domain::dialect::Dialect;

use crate::domain::semantics::value::model::LiteralValue;

/// The named constant `text` denotes in `dialect`, if any.
///
/// The three families disagree in ways that matter: Common Lisp and Emacs Lisp
/// use `t`/`nil` and treat `nil` as both false and the empty list; Scheme and
/// Racket use `#t`/`#f` and consider `'()` true; Clojure uses `true`/`false`
/// with a separate `nil`. Reading `nil` as false in Scheme would invert a
/// conditional.
pub fn named_constant(dialect: Dialect, text: &str) -> Option<LiteralValue> {
    match dialect {
        Dialect::CommonLisp | Dialect::EmacsLisp | Dialect::Lfe => {
            match_case_insensitive(text, &[("t", LiteralValue::Boolean(true))])
                .or_else(|| match_case_insensitive(text, &[("nil", LiteralValue::Nil)]))
        }
        Dialect::Scheme | Dialect::Racket => match text {
            "#t" | "#true" => Some(LiteralValue::Boolean(true)),
            "#f" | "#false" => Some(LiteralValue::Boolean(false)),
            _ => None,
        },
        Dialect::Clojure | Dialect::Hy => match text {
            "true" => Some(LiteralValue::Boolean(true)),
            "false" => Some(LiteralValue::Boolean(false)),
            "nil" | "None" => Some(LiteralValue::Nil),
            _ => None,
        },
        _ => None,
    }
}

/// Whether `dialect` folds symbol case when reading, which decides whether
/// `:Foo` and `:foo` name the same keyword.
pub const fn folds_symbol_case(dialect: Dialect) -> bool {
    matches!(dialect, Dialect::CommonLisp)
}

fn match_case_insensitive(text: &str, table: &[(&str, LiteralValue)]) -> Option<LiteralValue> {
    table
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(text))
        .map(|(_, value)| value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_lisp_reads_t_and_nil_case_insensitively() {
        assert_eq!(
            named_constant(Dialect::CommonLisp, "T"),
            Some(LiteralValue::Boolean(true))
        );
        assert_eq!(
            named_constant(Dialect::CommonLisp, "NIL"),
            Some(LiteralValue::Nil)
        );
    }

    #[test]
    fn scheme_does_not_read_nil_as_a_constant() {
        assert_eq!(named_constant(Dialect::Scheme, "nil"), None);
        assert_eq!(
            named_constant(Dialect::Scheme, "#f"),
            Some(LiteralValue::Boolean(false))
        );
    }

    #[test]
    fn clojure_separates_false_from_nil() {
        assert_eq!(
            named_constant(Dialect::Clojure, "false"),
            Some(LiteralValue::Boolean(false))
        );
        assert_eq!(
            named_constant(Dialect::Clojure, "nil"),
            Some(LiteralValue::Nil)
        );
    }

    #[test]
    fn only_common_lisp_folds_symbol_case() {
        assert!(folds_symbol_case(Dialect::CommonLisp));
        assert!(!folds_symbol_case(Dialect::Clojure));
        assert!(!folds_symbol_case(Dialect::Scheme));
    }
}
