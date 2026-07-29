//! `float-equality`: comparing a float for exact equality.
//!
//! `(= x 0.1)` asks whether a computed value is bit-identical to the nearest
//! representable float to one tenth. Almost no arithmetic that should produce
//! one tenth does. The test is either always false or accidentally true, and
//! which of the two depends on the rounding of every operation upstream.
//!
//! The rule fires on a *literal* float operand, and only that. A comparison
//! between two variables might be comparing integers, and nothing here can
//! tell; a comparison against a written-out float is unambiguous about what
//! was meant, which is why it is the case worth reporting.
//!
//! `zerop` is included and is the sharpest instance: `(zerop x)` on a float
//! accumulator is the classic never-terminates loop condition.
//!
//! Report-only. The replacement needs a tolerance, and choosing one is a
//! statement about the problem domain that no rule can make.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "float-equality",
    RuleCategory::NumericPrecision,
    Severity::Warning,
    "an exact equality test against a float literal, whose result depends on rounding upstream",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Binary floating point cannot represent most decimal fractions exactly, so a value that \
         should equal 0.1 usually differs from the literal `0.1` in the last bits. The comparison \
         then succeeds or fails according to the rounding of every operation that produced it.",
    )
    .with_example(
        "(when (= total 0.1) (done))",
        "(when (< (abs (- total 0.1)) *tolerance*) (done))",
    )
    .with_caveat(
        "Only a written-out float operand is reported. `(= a b)` may well be comparing integers, \
         and this layer cannot tell — that is `char-op-string`'s kind of question, not this \
         rule's.",
    ),
);

const HEADS: [NormalizedHead; 6] = [
    NormalizedHead::new("="),
    NormalizedHead::new("/="),
    NormalizedHead::new("eql"),
    NormalizedHead::new("equal"),
    NormalizedHead::new("eq"),
    NormalizedHead::new("zerop"),
];

/// One exact float comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatComparison {
    pub span: ByteSpan,
    /// The literal as written, or `"0.0"` for the implicit constant `zerop`
    /// carries.
    pub literal: String,
}

/// Whether `text` reads as a float literal.
///
/// A float in Common Lisp is a token with a decimal point between digits, or
/// one carrying an exponent marker (`1e10`, `1d0`, `3.0s-2`). A ratio (`1/3`)
/// is exact and deliberately excluded; so is an integer, whose equality is
/// exact and fine.
#[must_use]
pub fn is_float_literal(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    if body.is_empty() || !body.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return false;
    }
    // A ratio is exact, whatever else it looks like.
    if body.contains('/') {
        return false;
    }
    let mut digits = false;
    let mut point = false;
    let mut exponent = false;
    for (index, character) in body.char_indices() {
        match character {
            '0'..='9' => digits = true,
            '.' if !point && !exponent => point = true,
            'e' | 's' | 'f' | 'd' | 'l' | 'E' | 'S' | 'F' | 'D' | 'L'
                if digits && !exponent && index > 0 =>
            {
                exponent = true;
            }
            '+' | '-' if exponent => {}
            _ => return false,
        }
    }
    // `1.` is an integer in Common Lisp — a trailing point is decimal-radix
    // notation, not a float — so a point alone is not enough.
    digits && ((point && !body.ends_with('.')) || exponent)
}

/// Reads one comparison and reports the float literal it tests against.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<FloatComparison> {
    let head = list_head(view)?;
    // The dispatcher only routes the heads below here, but the check belongs in
    // the function too: a caller reading `examine` alone must not be told that
    // `(< x 0.1)` is an exact-equality test.
    if !HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate.as_str()))
    {
        return None;
    }

    if head.eq_ignore_ascii_case("zerop") {
        // `(zerop x)` is only worth reporting when `x` is provably a float
        // computation, which this layer cannot see — except in the one case it
        // can: a literal.
        let argument = view.children.get(1)?;
        let text = atom_text(argument)?;
        return is_float_literal(text).then(|| FloatComparison {
            span: view.span,
            literal: text.to_owned(),
        });
    }

    view.children.iter().skip(1).find_map(|operand| {
        if !operand.reader_prefixes.is_empty() {
            return None;
        }
        let text = atom_text(operand)?;
        is_float_literal(text).then(|| FloatComparison {
            span: view.span,
            literal: text.to_owned(),
        })
    })
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        if let Some(found) = examine(view) {
            sink.report(
                found.span,
                format!(
                    "exact equality against the float literal {} depends on the rounding of every \
                     operation upstream; compare against a tolerance instead",
                    found.literal
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn literal(input: &str) -> Option<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.literal)
    }

    #[test]
    fn recognises_the_float_spellings() {
        assert!(is_float_literal("0.1"));
        assert!(is_float_literal("-3.25"));
        assert!(is_float_literal("1.0e10"));
        assert!(is_float_literal("1d0"));
        assert!(is_float_literal("3.0s-2"));
    }

    #[test]
    fn does_not_read_an_exact_number_as_a_float() {
        assert!(!is_float_literal("1"));
        assert!(!is_float_literal("-42"));
        assert!(!is_float_literal("1/3"));
        // A trailing point is decimal-radix notation for an integer.
        assert!(!is_float_literal("1."));
    }

    #[test]
    fn does_not_read_a_symbol_as_a_float() {
        assert!(!is_float_literal("x"));
        assert!(!is_float_literal("point.five"));
        assert!(!is_float_literal(""));
        assert!(!is_float_literal("+"));
    }

    #[test]
    fn flags_an_equality_test_against_a_float() {
        assert_eq!(literal("(= total 0.1)"), Some("0.1".to_owned()));
        assert_eq!(literal("(= 0.1 total)"), Some("0.1".to_owned()));
        assert_eq!(literal("(eql x 2.5)"), Some("2.5".to_owned()));
        assert_eq!(literal("(/= x 1.0e-3)"), Some("1.0e-3".to_owned()));
    }

    #[test]
    fn flags_zerop_of_a_float_literal() {
        assert_eq!(literal("(zerop 0.0)"), Some("0.0".to_owned()));
    }

    #[test]
    fn does_not_flag_an_integer_comparison() {
        assert_eq!(literal("(= n 0)"), None);
        assert_eq!(literal("(eql x 42)"), None);
    }

    #[test]
    fn does_not_flag_a_comparison_between_two_expressions() {
        assert_eq!(literal("(= a b)"), None);
        assert_eq!(literal("(= (f x) (g y))"), None);
    }

    #[test]
    fn does_not_flag_zerop_of_something_unknown() {
        assert_eq!(literal("(zerop x)"), None);
    }

    #[test]
    fn does_not_flag_an_ordered_comparison() {
        // `(< x 0.1)` is a perfectly sound thing to write about a float.
        assert_eq!(literal("(< x 0.1)"), None);
    }
}
