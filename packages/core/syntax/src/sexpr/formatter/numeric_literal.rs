//! Text-level letter-case normalization of a numeric literal atom: the
//! radix-prefix marker (`#x`/`#o`/`#b`/`#<n>r`) and the float exponent marker
//! (`e`/`s`/`f`/`d`/`l`). Driven by `format.numeric-literal-case`;
//! [`NumericLiteralCase::Preserve`] (the default) makes every call here a
//! no-op, and every other setting only ever rewrites the marker
//! character(s) themselves, never a digit.
//!
//! This never misfires against a same-shaped symbol because of how a Lisp
//! reader's token grammar works: a token whose shape matches a number can
//! never also be read as a symbol (CLHS 2.3.1's "a potential number is not a
//! symbol", and R7RS's equivalent rule), so matching this module's number
//! grammar is never a false positive that could rename part of an
//! identifier — `x1e5` fails the grammar (it does not start with a sign or a
//! digit) and stays untouched, while `1e5` unambiguously is a number in
//! every dialect that reads it at all.
//!
//! Radix prefixes are gated to the three dialects whose reader actually folds
//! `#x`/`#o`/`#b`/`#<n>r` into one atom token rather than treating `#` as a
//! reader-macro dispatch of its own: see `classify_common_lisp` and
//! `classify_scheme` in `reader_policy.rs`, and `is_numeric_radix_dispatch`
//! for the arbitrary-radix spelling, which only Common Lisp has. Emacs Lisp's
//! `#x10` is not even valid syntax under this codebase's reader (it errors,
//! `classify_emacs_lisp` never special-cases digits after `#`), and a
//! Clojure `#x10` reads as a tagged literal — an opaque reader form this
//! rendering path never reaches in the first place. Gating explicitly here,
//! rather than only relying on those forms never reaching this function,
//! keeps this module correct even if a future reader-policy change widens
//! what parses.
//!
//! The exponent marker `e`/`E` is common to every dialect's float syntax, so
//! it is recognised regardless of dialect; the single/short/double/long
//! markers `s`/`f`/`d`/`l` are Common Lisp-specific (CLHS 12.1.3.2) and are
//! only recognised there.

use std::borrow::Cow;

use crate::dialect::Dialect;

/// How `format.numeric-literal-case` renders a numeric literal's letter
/// markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NumericLiteralCase {
    /// Byte-identical to source. The default, and the only behavior before
    /// this option existed: this is a layout *policy*, not a bug fix.
    #[default]
    Preserve,
    Lower,
    Upper,
}

impl NumericLiteralCase {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::Lower => "lower",
            Self::Upper => "upper",
        }
    }

    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "preserve" => Some(Self::Preserve),
            "lower" => Some(Self::Lower),
            "upper" => Some(Self::Upper),
            _ => None,
        }
    }
}

/// `text`, with its radix-prefix or exponent marker recased per `case` —
/// borrowed unchanged when `case` is [`NumericLiteralCase::Preserve`], `text`
/// is not a numeric literal this module recognises, or the marker is already
/// in the requested case.
#[must_use]
pub(super) fn numeric_literal_text(
    text: &str,
    dialect: Dialect,
    case: NumericLiteralCase,
) -> Cow<'_, str> {
    if case == NumericLiteralCase::Preserve || !could_start_numeric_literal(text) {
        return Cow::Borrowed(text);
    }
    match radix_marker_index(text, dialect).or_else(|| exponent_marker_index(text, dialect)) {
        Some(index) => Cow::Owned(recase_byte_at(text, index, case)),
        None => Cow::Borrowed(text),
    }
}

/// Cheap first-byte guard skipping `exponent_marker_index`'s
/// allocate-and-scan for an atom that can never be a numeric literal in any
/// dialect this module recognises. Every shape this module recases — a
/// radix prefix (`#x`/`#o`/`#b`/`#<n>r`) or a float, signed or not, with or
/// without a leading decimal point (`.5e10` recases, confirmed by this
/// module's own tests) — starts with `#`, a digit, `+`, `-`, or `.`.
/// Anything else — an ordinary symbol like `defun` or `my-variable-name`, or
/// a numeric-looking symbol this grammar deliberately rejects like `e10` or
/// `x1e5` — is guaranteed to make both `radix_marker_index` and
/// `exponent_marker_index` return `None` anyway (see their own docs), so
/// bailing out here changes no observable output; it only skips the
/// per-atom allocation for the common case of a non-numeric atom, which is
/// most atoms in real source.
const fn could_start_numeric_literal(text: &str) -> bool {
    matches!(
        text.as_bytes().first(),
        Some(b'0'..=b'9' | b'+' | b'-' | b'.' | b'#')
    )
}

/// The byte index of `text`'s radix marker letter (`x`/`o`/`b`/`r`), when
/// `text` is a radix-prefixed integer literal `dialect` reads as one atom.
fn radix_marker_index(text: &str, dialect: Dialect) -> Option<usize> {
    if !matches!(
        dialect,
        Dialect::CommonLisp | Dialect::Scheme | Dialect::Racket
    ) {
        return None;
    }
    if !text.starts_with('#') {
        return None;
    }
    match *text.as_bytes().get(1)? {
        b'x' | b'X' if valid_radix_digits(&text[2..], 16) => Some(1),
        b'o' | b'O' if valid_radix_digits(&text[2..], 8) => Some(1),
        b'b' | b'B' if valid_radix_digits(&text[2..], 2) => Some(1),
        _ if dialect == Dialect::CommonLisp => {
            let rest = &text[1..];
            let marker = rest.find(['r', 'R'])?;
            if marker == 0 {
                return None;
            }
            let radix: u32 = rest[..marker].parse().ok()?;
            if !(2..=36).contains(&radix) {
                return None;
            }
            valid_radix_digits(&rest[marker + 1..], radix).then_some(1 + marker)
        }
        _ => None,
    }
}

fn valid_radix_digits(digits: &str, radix: u32) -> bool {
    let unsigned = digits.strip_prefix(['+', '-']).unwrap_or(digits);
    !unsigned.is_empty() && unsigned.chars().all(|digit| digit.is_digit(radix))
}

/// The byte index of `text`'s exponent marker letter, when `text` is a float
/// literal with an explicit marker. `None` for a float with no marker at all
/// (`1.5`, nothing to recase) as well as for anything that is not a float.
fn exponent_marker_index(text: &str, dialect: Dialect) -> Option<usize> {
    let single_letter_markers: &[u8] = if dialect == Dialect::CommonLisp {
        b"sSfFdDlL"
    } else {
        b""
    };

    let mut marker_index = None;
    let mut normalized = String::with_capacity(text.len());
    for (index, byte) in text.bytes().enumerate() {
        if matches!(byte, b'e' | b'E') || single_letter_markers.contains(&byte) {
            if marker_index.is_some() {
                // More than one candidate marker: not a shape this grammar
                // reads as a single exponent, whatever it is.
                return None;
            }
            marker_index = Some(index);
            normalized.push('e');
        } else {
            normalized.push(byte as char);
        }
    }
    let marker_index = marker_index?;
    if !normalized
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'.' | b'e' | b'+' | b'-'))
    {
        return None;
    }
    let value: f64 = normalized.parse().ok()?;
    value.is_finite().then_some(marker_index)
}

/// `text` with the ASCII byte at `index` recased. `index` is always the
/// position of a single-byte ASCII marker character this module itself
/// located, so treating it as a `char` boundary is always valid.
fn recase_byte_at(text: &str, index: usize, case: NumericLiteralCase) -> String {
    text.char_indices()
        .map(|(position, character)| {
            if position == index {
                match case {
                    NumericLiteralCase::Lower => character.to_ascii_lowercase(),
                    NumericLiteralCase::Upper => character.to_ascii_uppercase(),
                    NumericLiteralCase::Preserve => character,
                }
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalize(text: &str, dialect: Dialect, case: NumericLiteralCase) -> String {
        numeric_literal_text(text, dialect, case).into_owned()
    }

    #[test]
    fn preserve_is_always_a_no_op() {
        for text in ["#x1f", "#X1F", "1.0d0", "1.0E10"] {
            assert_eq!(
                normalize(text, Dialect::CommonLisp, NumericLiteralCase::Preserve),
                text
            );
        }
    }

    #[test]
    fn lowercases_only_the_radix_marker() {
        assert_eq!(
            normalize("#X1Fe", Dialect::CommonLisp, NumericLiteralCase::Lower),
            "#x1Fe"
        );
        assert_eq!(
            normalize("#O17", Dialect::CommonLisp, NumericLiteralCase::Lower),
            "#o17"
        );
        assert_eq!(
            normalize("#B1010", Dialect::CommonLisp, NumericLiteralCase::Lower),
            "#b1010"
        );
    }

    #[test]
    fn uppercases_only_the_radix_marker() {
        assert_eq!(
            normalize("#x1f", Dialect::CommonLisp, NumericLiteralCase::Upper),
            "#X1f"
        );
    }

    #[test]
    fn recases_the_arbitrary_radix_marker_but_not_the_radix_digits() {
        assert_eq!(
            normalize("#16rff", Dialect::CommonLisp, NumericLiteralCase::Upper),
            "#16Rff"
        );
        assert_eq!(
            normalize("#16RFF", Dialect::CommonLisp, NumericLiteralCase::Lower),
            "#16rFF"
        );
    }

    #[test]
    fn recases_the_exponent_marker_but_not_the_mantissa_digits() {
        assert_eq!(
            normalize("1.0E10", Dialect::CommonLisp, NumericLiteralCase::Lower),
            "1.0e10"
        );
        assert_eq!(
            normalize("1.0d0", Dialect::CommonLisp, NumericLiteralCase::Upper),
            "1.0D0"
        );
        assert_eq!(
            normalize("1e10", Dialect::CommonLisp, NumericLiteralCase::Upper),
            "1E10"
        );
    }

    #[test]
    fn a_leading_decimal_point_float_still_recases_its_exponent_marker() {
        // No leading digit before the `.` — pinned as its own case because
        // `could_start_numeric_literal`'s fast-path guard must treat `.` as
        // a plausible numeric-literal start, not just a digit or sign, or
        // this would silently stop recasing.
        assert_eq!(
            normalize(".5e10", Dialect::CommonLisp, NumericLiteralCase::Upper),
            ".5E10"
        );
        assert_eq!(
            normalize("-.5e10", Dialect::CommonLisp, NumericLiteralCase::Upper),
            "-.5E10"
        );
    }

    #[test]
    fn radix_prefixes_are_dialect_gated() {
        // Clojure never reads `#x10` as a radix-prefixed number (it is a
        // tagged literal there), so this module must not touch it even
        // though the text shape matches Common Lisp's.
        assert_eq!(
            normalize("#x10", Dialect::Clojure, NumericLiteralCase::Upper),
            "#x10"
        );
        assert_eq!(
            normalize("#x10", Dialect::EmacsLisp, NumericLiteralCase::Upper),
            "#x10"
        );
    }

    #[test]
    fn single_letter_exponent_markers_are_common_lisp_only() {
        // `d`/`s`/`f`/`l` are Common Lisp float-precision markers; outside
        // Common Lisp, `1d0` is left alone rather than guessed at as a float.
        assert_eq!(
            normalize("1.0d0", Dialect::Scheme, NumericLiteralCase::Upper),
            "1.0d0"
        );
        assert_eq!(
            normalize("1.0e10", Dialect::Scheme, NumericLiteralCase::Upper),
            "1.0E10"
        );
    }

    #[test]
    fn a_symbol_that_merely_looks_numeric_is_left_alone() {
        for text in ["x1e5", "1cost", "e10", "2d", "#xZZ", "#37rA"] {
            assert_eq!(
                normalize(text, Dialect::CommonLisp, NumericLiteralCase::Upper),
                text,
                "{text} must not be treated as a numeric literal"
            );
        }
    }

    #[test]
    fn a_plain_integer_has_no_marker_to_recase() {
        assert_eq!(
            normalize("123", Dialect::CommonLisp, NumericLiteralCase::Upper),
            "123"
        );
        assert_eq!(
            normalize("10.", Dialect::CommonLisp, NumericLiteralCase::Upper),
            "10."
        );
    }

    #[test]
    fn recasing_is_idempotent() {
        let once = normalize("#X1f", Dialect::CommonLisp, NumericLiteralCase::Lower);
        let twice = normalize(&once, Dialect::CommonLisp, NumericLiteralCase::Lower);
        assert_eq!(once, twice);
    }
}
