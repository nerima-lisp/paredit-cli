//! Which of `inspect constants`' findings may actually be written back.
//!
//! The report answers "what could be folded"; that is a strictly larger set
//! than "what may be folded into this file". A finding the report is right to
//! show can still be one this command must decline, because the report's
//! reader spelling is only required to be *informative* while a rewrite has to
//! be *equivalent*.

use crate::constant_report::domain::{FLOAT_LITERAL_KIND, FoldableExpression};

/// Whether this finding is safe to substitute for the source text it covers.
///
/// Floats are refused. [`LiteralValue::Float`] carries an `f64` and nothing
/// else, so the exponent marker that decides a Common Lisp float's *type* is
/// gone by the time a finding exists: `1.0d0` is a `double-float`, `1.0f0` a
/// `single-float`, and `1.0` whatever `*read-default-float-format*` says. No
/// spelling this command can emit reproduces the one the source had, and
/// substituting a different float type silently changes the precision — or,
/// when the printed value happens to be integral, the type entirely. Reading
/// a float is still useful to a lint asking whether a divisor is `0.0`, which
/// is why the value layer keeps them; writing one back is not.
///
/// [`LiteralValue::Float`]: paredit_core_semantics::semantics::value::LiteralValue::Float
#[must_use]
pub fn fold_preserves_the_value(foldable: &FoldableExpression) -> bool {
    foldable.kind != FLOAT_LITERAL_KIND
}

/// Whether this finding clears the caller's profitability threshold.
#[must_use]
pub const fn fold_is_profitable(foldable: &FoldableExpression, min_saved_bytes: i64) -> bool {
    foldable.saved_bytes >= min_saved_bytes
}

/// Whether this finding should be folded: safe first, profitable second.
#[must_use]
pub fn should_fold(foldable: &FoldableExpression, min_saved_bytes: i64) -> bool {
    fold_preserves_the_value(foldable) && fold_is_profitable(foldable, min_saved_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan};

    fn finding(kind: &'static str, value: &str, saved_bytes: i64) -> FoldableExpression {
        FoldableExpression {
            span: ByteSpan::new(ByteOffset::new(0), ByteOffset::new(1)),
            line: 1,
            text: "(form)".to_owned(),
            value: value.to_owned(),
            kind,
            saved_bytes,
        }
    }

    #[test]
    fn an_integer_fold_is_taken() {
        assert!(should_fold(&finding("integer", "3", 6), 0));
    }

    #[test]
    fn a_float_fold_is_refused_however_profitable() {
        // `(if t 1.0d0 2)` folds to a `double-float` the report can only spell
        // as `1.0`; writing that back turns a double into a single, and the
        // pre-fix spelling turned it into the integer 1.
        let float = finding(FLOAT_LITERAL_KIND, "1.0", 13);
        assert!(!fold_preserves_the_value(&float));
        assert!(!should_fold(&float, 0));
        assert!(!should_fold(&float, i64::MIN));
    }

    #[test]
    fn a_string_fold_is_taken_because_its_spelling_is_faithful() {
        assert!(should_fold(&finding("string", "\"ab\"", 4), 0));
    }

    #[test]
    fn the_threshold_holds_back_an_otherwise_safe_fold() {
        let integer = finding("integer", "3", 6);
        assert!(fold_preserves_the_value(&integer));
        assert!(!should_fold(&integer, 7));
        assert!(should_fold(&integer, 6), "the threshold is inclusive");
    }
}
