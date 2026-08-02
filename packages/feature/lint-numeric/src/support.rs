//! What the numeric rules share: which parts of a file are *code*, and how a
//! numeric token is classified.
//!
//! # Evaluation context
//!
//! The lint engine's dispatch walks into quoted data like any other subtree and
//! [`RuleContext`] carries no parent pointer, so a head-matched node cannot tell
//! on its own whether it is code. `'(+ 3.14 1.0d0)` is a list of four symbols,
//! not an arithmetic form. [`is_unevaluated_at`] answers that by descending from
//! the root along the single chain of nodes whose span contains the candidate's
//! — depth-many steps, not tree-many — and every rule here calls it only once it
//! already has a finding to report.
//!
//! The semantics are deliberately copied from
//! `paredit-feature-lint-condition-system`'s module of the same name rather than
//! depended on across a package boundary, and the five shapes that distinguish a
//! correct implementation from a plausible-looking wrong one are pinned in this
//! module's own tests:
//!
//! 1. `'(…)` is data.
//! 2. `(quote (…))` is data below its head.
//! 3. `` `(…) `` without an unquote is data.
//! 4. `` `(a ,(…)) `` is *code* from the comma down.
//! 5. `'(a ,(…))` is data — a comma inside a **hard** quote is a literal comma
//!    in a literal list, not an escape. This is the shape a single `i32` depth
//!    counter gets wrong, which is why `hard` is a `bool` that never clears and
//!    `quasi` is a separate counter.
//!
//! # Numeric classification
//!
//! [`FloatKind`] and [`integer_literal_value`] read a token the way the Common
//! Lisp reader does, which is *not* the way Rust's `str::parse` does: `1.` is a
//! decimal-radix **integer** in Common Lisp, `1/3` is an exact ratio, and a
//! float's format comes from its exponent marker (`1.0` is a single-float under
//! the default `*read-default-float-format*`, `1.0d0` is a double-float).
//!
//! Nothing here is called per visited node. That is deliberate: the
//! `clean/forms/*` benchmarks lint files with zero findings, so the per-file
//! cost of a rule that matches nothing is exactly what they measure.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_is};

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuoteState {
    hard: bool,
    quasi: u32,
}

impl QuoteState {
    const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    const fn is_data(self) -> bool {
        self.hard || self.quasi > 0
    }

    /// The state inside a node, given the state outside it and the node's own
    /// reader prefixes.
    ///
    /// `#'`, `#.`, `#+`, metadata and the rest are deliberately neutral: none of
    /// them turns code into data.
    fn after_prefixes(mut self, view: &ExpressionView) -> Self {
        for prefix in &view.reader_prefixes {
            match prefix {
                ReaderPrefix::Quote => self.hard = true,
                ReaderPrefix::Quasiquote => self.quasi += 1,
                ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing => {
                    self.quasi = self.quasi.saturating_sub(1);
                }
                _ => {}
            }
        }
        self
    }

    const fn quoted(mut self) -> Self {
        self.hard = true;
        self
    }
}

/// The long-hand `(quote …)`, which the reader also produces for `'…` but which
/// hand-written code and macro output both spell out.
fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the cost is the node's depth, not the file's size.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(truncate (float x))) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it, and
/// that is already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let root = tree.root_view();
    let mut view: &ExpressionView = &root;
    let mut state = QuoteState::EVALUATED;

    loop {
        let quoting = is_quote_form(view);
        // A span that names no node is judged by the innermost node that
        // contains it, which is the honest answer for a span the caller
        // synthesized rather than took from the tree.
        let Some(child) = view
            .children
            .iter()
            .find(|child| span_contains(child.span, target))
        else {
            return state.is_data();
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
        if view.span == target {
            return state.is_data();
        }
    }
}

/// Which floating-point format a numeric token reads as.
///
/// The distinction is the exponent marker, per CLHS 2.3.1: `e`/`E` (and no
/// marker at all) means the value of `*read-default-float-format*`, which is
/// `single-float` unless a file rebinds it; `s`/`f` are short and single; `d`/`l`
/// are double and long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatKind {
    /// `1.0`, `1.0e3`, `1.0s0`, `1.0f0` — single-float under the default
    /// `*read-default-float-format*`.
    Single,
    /// `1.0d0`, `1.0L0` — explicitly wider than single.
    Double,
}

/// The float format `text` reads as, or `None` when it is not a float token at
/// all.
///
/// Excluded on purpose, because each is *exact* and so cannot lose precision:
/// an integer (`42`), a decimal-radix integer (`1.` — a trailing point is
/// radix notation in Common Lisp, not a float), and a ratio (`1/3`).
///
/// The first byte is checked before anything else, so a symbol — which is what
/// almost every operand of almost every arithmetic form is — is rejected after
/// one comparison.
#[must_use]
pub fn float_kind(text: &str) -> Option<FloatKind> {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    // The cheap gate: an operand that cannot start a number is not one.
    if !body.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return None;
    }
    // A ratio is exact, whatever else it looks like.
    if body.contains('/') {
        return None;
    }

    let mut digits = false;
    let mut point = false;
    let mut marker: Option<char> = None;
    for (index, character) in body.char_indices() {
        match character {
            '0'..='9' => digits = true,
            '.' if !point && marker.is_none() => point = true,
            'e' | 's' | 'f' | 'd' | 'l' | 'E' | 'S' | 'F' | 'D' | 'L'
                if digits && marker.is_none() && index > 0 =>
            {
                marker = Some(character.to_ascii_lowercase());
            }
            '+' | '-' if marker.is_some() => {}
            _ => return None,
        }
    }
    if !digits {
        return None;
    }
    // `1.` is an integer in Common Lisp, so a trailing point is not a float.
    if !(point && !body.ends_with('.')) && marker.is_none() {
        return None;
    }
    match marker {
        Some('d' | 'l') => Some(FloatKind::Double),
        _ => Some(FloatKind::Single),
    }
}

/// The digits of a float token with its exponent marker normalized to `e`, so
/// that Rust's `str::parse` reads the same value the Common Lisp reader does.
///
/// `1.5d0` and `1.5` both become a string Rust parses as `1.5`; the format is
/// [`float_kind`]'s business, not this function's.
fn normalized_for_parse(text: &str) -> String {
    text.chars()
        .map(|c| match c.to_ascii_lowercase() {
            's' | 'f' | 'd' | 'l' => 'e',
            other => other,
        })
        .collect()
}

/// Whether widening this single-float token to a double-float would change its
/// value away from what the same digits mean as a double.
///
/// This is the discriminator that keeps `mixed-float-precision-arithmetic` off
/// correct code. `1.5`, `2.0`, `0.25` and `0.5` are exactly representable in
/// both formats, so `(* 1.5 x 1.0d0)` computes with the value that was written
/// and there is nothing to report. `3.14` is not: as a single-float it is
/// 3.1400001049041748, and *that* is the value a double-float result carries —
/// verified against SBCL, which prints `(* 3.14 1.0d0)` as
/// `3.140000104904175d0`.
///
/// A token Rust cannot parse answers `false`. Refusing to report what cannot be
/// read is the safe direction: this function only ever *adds* a finding.
#[must_use]
pub fn single_float_widens_lossily(text: &str) -> bool {
    let normalized = normalized_for_parse(text);
    let (Ok(narrow), Ok(wide)) = (normalized.parse::<f32>(), normalized.parse::<f64>()) else {
        return false;
    };
    // Not a tolerance comparison: the question is exactly whether the two
    // formats denote the same real number, and `f64::from(narrow)` is exact.
    f64::from(narrow) != wide
}

/// Whether a float token denotes a number binary floating point can hold
/// *exactly* — a dyadic rational, `m / 2^k`.
///
/// A decimal literal is `M × 10^(e-n)` for mantissa digits `M`, `n` fractional
/// digits and exponent `e`. Writing `k = n - e`, the value is `M / 10^k`, which
/// reduces to a power-of-two denominator exactly when `5^k` divides `M`. So
/// `0.5` (M=5, k=1) and `0.125` (M=125, k=3) are exact, and `0.1` (M=1, k=1) and
/// `0.3` (M=3, k=1) are not.
///
/// This is the guard that keeps `epsilon-less-float-loop-bound` off correct
/// code. A loop stepping by an exact 0.5 accumulates 0.5, 1.0, 1.5, … with no
/// drift at all, so an equality test against it is not the classic footgun —
/// verified against SBCL, where a `do` loop stepping by 0.5 to a limit of 2
/// terminates and one stepping by 0.1 does not.
///
/// Undecidable input answers `true` (exact, so no finding): a mantissa too long
/// for `i64` and an exponent large enough to overflow `5^k` both take the
/// false-negative direction on purpose.
#[must_use]
pub fn is_exact_binary_float(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);

    // Split the exponent off, if the token carries one.
    let (mantissa, exponent) = match body.char_indices().find(|(index, c)| {
        *index > 0 && matches!(c.to_ascii_lowercase(), 'e' | 's' | 'f' | 'd' | 'l')
    }) {
        Some((at, _)) => {
            let (head, tail) = body.split_at(at);
            match tail[1..].parse::<i32>() {
                Ok(value) => (head, value),
                // An exponent that will not parse is not one this can reason
                // about.
                Err(_) => return true,
            }
        }
        None => (body, 0),
    };

    let fraction_digits = match mantissa.find('.') {
        Some(at) => i32::try_from(mantissa.len() - at - 1).unwrap_or(0),
        None => 0,
    };
    let Ok(digits) = mantissa.replace('.', "").parse::<i64>() else {
        return true;
    };

    let k = fraction_digits - exponent;
    if k <= 0 {
        // An integer times a power of ten: exactly representable.
        return true;
    }
    // 5^28 overflows an i64, and a literal needing that many fractional digits
    // is past anything this can decide.
    let Some(power) = 5i64.checked_pow(u32::try_from(k).unwrap_or(u32::MAX)) else {
        return true;
    };
    digits % power == 0
}

/// The value of a plain decimal integer token, or `None` for anything else.
///
/// Deliberately narrow. A ratio, a float, a radix prefix (`#x10`), a
/// decimal-radix trailing point (`1.`) and a bignum too large for `i64` all
/// answer `None`, because the rules that consult this want to *compute* with the
/// value and a guess would be worse than a skipped form.
#[must_use]
pub fn integer_literal_value(text: &str) -> Option<i64> {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::view_query::for_each_subview;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    /// Finds the first node with `head` — deliberately through the *unfiltered*
    /// walk, since the point is to locate the node even when it is data — and
    /// asks the span-directed lookup about it.
    fn unevaluated_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    // -- the five quote shapes ----------------------------------------------

    #[test]
    fn shape_1_a_hard_quoted_list_is_data() {
        assert!(unevaluated_at_first_head("'(+ 3.14 1.0d0)", "+"));
    }

    #[test]
    fn shape_2_a_long_hand_quote_form_is_data_below_its_head() {
        assert!(unevaluated_at_first_head("(quote (+ 3.14 1.0d0))", "+"));
    }

    #[test]
    fn shape_3_a_backquote_without_an_unquote_is_data() {
        assert!(unevaluated_at_first_head("`(+ 3.14 1.0d0)", "+"));
    }

    #[test]
    fn shape_4_an_unquote_inside_a_backquote_is_code_again() {
        assert!(!unevaluated_at_first_head("`(a ,(+ 3.14 1.0d0))", "+"));
    }

    /// The shape a single depth counter gets wrong: the comma is a literal
    /// comma inside a literal list, so everything under it stays data.
    #[test]
    fn shape_5_a_comma_inside_a_hard_quote_stays_data() {
        assert!(unevaluated_at_first_head("'(a ,(+ 3.14 1.0d0))", "+"));
    }

    #[test]
    fn plain_code_is_evaluated() {
        assert!(!unevaluated_at_first_head(
            "(defun f (x) (+ 3.14 x 1.0d0))",
            "+"
        ));
    }

    /// A node one level *inside* a quote is still data, which is the other half
    /// of what a "check the immediate parent only" implementation misses.
    #[test]
    fn a_node_nested_deep_inside_a_hard_quote_is_still_data() {
        assert!(unevaluated_at_first_head(
            "'(a (b (c (+ 3.14 1.0d0))))",
            "+"
        ));
    }

    // -- float classification -----------------------------------------------

    #[test]
    fn reads_the_single_float_spellings() {
        for text in ["1.0", "0.1", "-3.25", "1.0e10", "3.0s-2", "1.5f0", "+2.5"] {
            assert_eq!(float_kind(text), Some(FloatKind::Single), "{text}");
        }
    }

    #[test]
    fn reads_the_double_and_long_float_spellings() {
        for text in ["1.0d0", "1d0", "-2.5d-3", "1.0L0", "3.14D0"] {
            assert_eq!(float_kind(text), Some(FloatKind::Double), "{text}");
        }
    }

    /// Every exact spelling. None of these can lose precision, so none of them
    /// is a float as far as these rules are concerned.
    #[test]
    fn does_not_read_an_exact_number_as_a_float() {
        for text in ["1", "-42", "1/3", "0", "1."] {
            assert_eq!(float_kind(text), None, "{text}");
        }
    }

    #[test]
    fn does_not_read_a_symbol_or_a_string_as_a_float() {
        for text in ["x", "point.five", "", "+", "-", "\"1.0\"", "1.0.0", "d0"] {
            assert_eq!(float_kind(text), None, "{text}");
        }
    }

    // -- the widening discriminator, checked against SBCL's answers ----------

    /// SBCL: `(coerce 3.14 'double-float)` => 3.140000104904175d0, which is not
    /// 3.14d0. Same for 0.1 and 1.1.
    #[test]
    fn a_single_float_that_cannot_be_written_exactly_widens_lossily() {
        for text in ["3.14", "0.1", "1.1", "0.2", "1.0e-1"] {
            assert!(single_float_widens_lossily(text), "{text}");
        }
    }

    /// SBCL: `(* 1.5 1.0d0)` => 1.5d0 exactly. A rule that reported these would
    /// be reporting correct code.
    #[test]
    fn a_single_float_that_is_exact_in_both_formats_widens_losslessly() {
        for text in ["1.5", "2.0", "0.25", "0.5", "100.0", "0.0", "-4.75"] {
            assert!(!single_float_widens_lossily(text), "{text}");
        }
    }

    #[test]
    fn an_unparseable_token_is_never_called_lossy() {
        assert!(!single_float_widens_lossily("x"));
        assert!(!single_float_widens_lossily(""));
    }

    // -- exact binary representability --------------------------------------

    /// SBCL: a `do` loop stepping by each of these to a limit of 2 terminates.
    #[test]
    fn a_dyadic_decimal_is_exact() {
        for text in [
            "0.5", "0.25", "0.75", "0.125", "1.0", "2.0", "-0.5", "0.0", "1.5d0", "1.0e0",
        ] {
            assert!(is_exact_binary_float(text), "{text}");
        }
    }

    /// SBCL: a `do` loop stepping by each of these to a limit of 2 never
    /// terminates.
    #[test]
    fn a_decimal_needing_a_factor_of_five_is_not_exact() {
        for text in ["0.1", "0.2", "0.3", "3.14", "0.01", "1.1", "0.1d0"] {
            assert!(!is_exact_binary_float(text), "{text}");
        }
    }

    /// A positive exponent cancels fractional digits: 1.5e1 is 15.
    #[test]
    fn an_exponent_shifts_the_fractional_digit_count() {
        assert!(is_exact_binary_float("1.5e1"));
        assert!(is_exact_binary_float("0.1e1"));
        assert!(!is_exact_binary_float("0.1e-1"));
    }

    /// Undecidable input takes the false-negative direction, so a rule built on
    /// this never reports what it could not read.
    #[test]
    fn an_undecidable_token_is_called_exact() {
        assert!(is_exact_binary_float("0.99999999999999999999999999999999"));
        assert!(is_exact_binary_float("1.0e999999999999"));
        assert!(is_exact_binary_float("x"));
    }

    // -- integer literals ----------------------------------------------------

    #[test]
    fn reads_a_plain_decimal_integer() {
        assert_eq!(integer_literal_value("42"), Some(42));
        assert_eq!(integer_literal_value("-7"), Some(-7));
        assert_eq!(integer_literal_value("+3"), Some(3));
        assert_eq!(integer_literal_value("0"), Some(0));
    }

    #[test]
    fn refuses_everything_that_is_not_a_plain_decimal_integer() {
        for text in [
            "1.0",
            "1/3",
            "#x10",
            "1.",
            "x",
            "",
            "-",
            "99999999999999999999",
        ] {
            assert_eq!(integer_literal_value(text), None, "{text}");
        }
    }
}
