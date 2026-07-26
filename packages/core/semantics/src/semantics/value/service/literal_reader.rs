//! Lexical interpretation of a single literal atom.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView};

use super::super::model::{FloatLiteral, KeywordName, LiteralValue, TextLiteral};
use super::super::policy::{folds_symbol_case, named_constant};

/// What the atom `view` denotes, or `None` when it is not a literal this layer
/// reads.
///
/// An atom carrying reader prefixes is not a literal in its own right — `'5`
/// is a quoted datum and `#.x` is read-time evaluation — so those yield `None`
/// rather than the value of whatever they enclose.
#[must_use]
pub fn literal_value(dialect: Dialect, view: &ExpressionView) -> Option<LiteralValue> {
    if view.kind != ExpressionKind::Atom || !view.reader_prefixes.is_empty() {
        return None;
    }
    read_literal(dialect, view.text.as_deref()?)
}

/// The same interpretation, from raw atom text.
fn read_literal(dialect: Dialect, text: &str) -> Option<LiteralValue> {
    if text.is_empty() {
        return None;
    }
    named_constant(dialect, text)
        .or_else(|| string_literal(text).map(LiteralValue::Text))
        .or_else(|| character_literal(text).map(LiteralValue::Char))
        .or_else(|| keyword_literal(dialect, text).map(LiteralValue::Keyword))
        .or_else(|| radix_integer(text).map(LiteralValue::Integer))
        .or_else(|| decimal_integer(text).map(LiteralValue::Integer))
        .or_else(|| float_literal(text).map(LiteralValue::Float))
}

/// A `"…"` literal with its reader escapes resolved.
///
/// Common Lisp's string reader has exactly one escape character: `\` makes the
/// next character literal, whatever it is. There are no `\n`-style sequences,
/// so resolving means dropping the backslashes rather than translating them.
fn string_literal(text: &str) -> Option<TextLiteral> {
    // A lone `"` would strip to itself, which is not a closed string.
    if text.len() < 2 {
        return None;
    }
    let body = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut resolved = String::with_capacity(body.len());
    let mut characters = body.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            resolved.push(characters.next()?);
        } else {
            resolved.push(character);
        }
    }
    Some(TextLiteral::new(resolved))
}

/// The CLHS standard character names. An implementation-specific spelling like
/// `#\U+0041` is deliberately absent: its meaning varies between
/// implementations, so reading it would be a guess.
const STANDARD_CHARACTER_NAMES: [(&str, char); 10] = [
    ("space", ' '),
    ("newline", '\n'),
    ("tab", '\t'),
    ("page", '\u{c}'),
    ("rubout", '\u{7f}'),
    ("linefeed", '\n'),
    ("return", '\r'),
    ("backspace", '\u{8}'),
    ("nul", '\0'),
    ("null", '\0'),
];

/// A `#\…` character literal: one character, or one of the standard names.
fn character_literal(text: &str) -> Option<char> {
    let name = text.strip_prefix("#\\")?;
    let mut characters = name.chars();
    let first = characters.next()?;
    if characters.next().is_none() {
        return Some(first);
    }
    STANDARD_CHARACTER_NAMES
        .iter()
        .find(|(standard, _)| standard.eq_ignore_ascii_case(name))
        .map(|(_, character)| *character)
}

/// A `:foo` keyword.
///
/// `::foo` is not keyword syntax — it names a symbol in the current package —
/// and a bare `:` names nothing.
fn keyword_literal(dialect: Dialect, text: &str) -> Option<KeywordName> {
    let name = text.strip_prefix(':')?;
    if name.is_empty() || name.starts_with(':') {
        return None;
    }
    Some(KeywordName::new(if folds_symbol_case(dialect) {
        name.to_ascii_uppercase()
    } else {
        name.to_owned()
    }))
}

/// A `#x`/`#o`/`#b`/`#<n>r` integer.
fn radix_integer(text: &str) -> Option<i128> {
    let rest = text.strip_prefix('#')?;
    let (radix, digits) = match rest.as_bytes().first()? {
        b'x' | b'X' => (16, &rest[1..]),
        b'o' | b'O' => (8, &rest[1..]),
        b'b' | b'B' => (2, &rest[1..]),
        _ => {
            let marker = rest.find(['r', 'R'])?;
            let radix: u32 = rest[..marker].parse().ok()?;
            // Outside 2..=36 there is no digit alphabet to read against.
            if !(2..=36).contains(&radix) {
                return None;
            }
            (radix, &rest[marker + 1..])
        }
    };
    parse_radix(digits, radix)
}

/// A decimal integer, including Common Lisp's trailing-dot spelling.
///
/// `10.` is the decimal integer ten, not a float — the dot pins the radix as
/// decimal regardless of `*read-base*`. Reading it as a float would make
/// `(parse-integer s :radix 10.)` look like a different call from
/// `(… :radix 10)`.
fn decimal_integer(text: &str) -> Option<i128> {
    parse_radix(text.strip_suffix('.').unwrap_or(text), 10)
}

/// `digits` in `radix`, with an optional leading sign.
///
/// Overflow yields `None`: a wrapped value would be a confident wrong answer,
/// and this layer's only real guarantee is that a `Known` can be trusted.
fn parse_radix(digits: &str, radix: u32) -> Option<i128> {
    let unsigned = digits.strip_prefix(['+', '-']).unwrap_or(digits);
    if unsigned.is_empty() || !unsigned.chars().all(|digit| digit.is_digit(radix)) {
        return None;
    }
    i128::from_str_radix(digits, radix).ok()
}

/// A float literal, including Common Lisp's `s`/`f`/`d`/`l` exponent markers.
///
/// Interpreted so a rule can tell `0.0` from `0`, but never folded: matching
/// an implementation's rounding order is a risk with no payoff here.
fn float_literal(text: &str) -> Option<FloatLiteral> {
    let normalized: String = text
        .chars()
        .map(|character| match character {
            's' | 'S' | 'f' | 'F' | 'd' | 'D' | 'l' | 'L' => 'e',
            other => other,
        })
        .collect();
    // Rust's parser accepts `inf`/`nan` spellings a Lisp reader would not, and
    // a float needs a decimal point or an exponent to be a float at all.
    if !normalized
        .chars()
        .all(|character| character.is_ascii_digit() || matches!(character, '.' | 'e' | '+' | '-'))
        || !normalized.contains(['.', 'e'])
    {
        return None;
    }
    let value: f64 = normalized.parse().ok()?;
    value.is_finite().then(|| FloatLiteral::new(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn read(input: &str) -> Option<LiteralValue> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree.select_path(&Path::root_child(0)).expect("form").view();
        literal_value(Dialect::CommonLisp, &view)
    }

    fn integer(input: &str) -> Option<i128> {
        match read(input) {
            Some(LiteralValue::Integer(value)) => Some(value),
            _ => None,
        }
    }

    #[test]
    fn reads_decimal_integers_with_either_sign() {
        assert_eq!(integer("123"), Some(123));
        assert_eq!(integer("-123"), Some(-123));
        assert_eq!(integer("+123"), Some(123));
        assert_eq!(integer("0"), Some(0));
    }

    #[test]
    fn a_trailing_dot_marks_a_decimal_integer_not_a_float() {
        assert_eq!(integer("10."), Some(10));
    }

    #[test]
    fn reads_every_radix_spelling() {
        assert_eq!(integer("#x1F"), Some(31));
        assert_eq!(integer("#X1f"), Some(31));
        assert_eq!(integer("#o17"), Some(15));
        assert_eq!(integer("#b1010"), Some(10));
        assert_eq!(integer("#16rFF"), Some(255));
        assert_eq!(integer("#36rZ"), Some(35));
        assert_eq!(integer("#x-10"), Some(-16));
    }

    #[test]
    fn an_invalid_literal_is_unknown_rather_than_wrong() {
        for input in ["#xZZ", "#37rA", "#1r0", "#b12", "#o18", "#\\Bogus", "#x"] {
            assert_eq!(read(input), None, "{input} must not read as a literal");
        }
    }

    #[test]
    fn an_integer_too_large_for_the_model_is_unknown() {
        // 2^127, one past `i128::MAX`. A wrapped value would be a confident
        // wrong answer, which is worse than no answer.
        assert_eq!(integer("170141183460469231731687303715884105728"), None);
        assert_eq!(integer(&format!("#x{}", "F".repeat(40))), None);
    }

    #[test]
    fn reads_single_characters_and_the_standard_names() {
        assert_eq!(read("#\\a"), Some(LiteralValue::Char('a')));
        assert_eq!(read("#\\Space"), Some(LiteralValue::Char(' ')));
        assert_eq!(read("#\\NEWLINE"), Some(LiteralValue::Char('\n')));
        assert_eq!(read("#\\Tab"), Some(LiteralValue::Char('\t')));
        assert_eq!(read("#\\Nul"), Some(LiteralValue::Char('\0')));
    }

    #[test]
    fn keywords_are_case_folded_and_lose_their_colon() {
        assert_eq!(
            read(":Foo"),
            Some(LiteralValue::Keyword(KeywordName::new("FOO")))
        );
        assert_eq!(read(":foo"), read(":FOO"));
    }

    #[test]
    fn a_package_qualified_name_is_not_a_keyword() {
        assert_eq!(read("::foo"), None);
        assert_eq!(read("pkg:foo"), None);
    }

    #[test]
    fn reads_t_and_nil_through_the_dialect_table() {
        assert_eq!(read("t"), Some(LiteralValue::Boolean(true)));
        assert_eq!(read("T"), Some(LiteralValue::Boolean(true)));
        assert_eq!(read("nil"), Some(LiteralValue::Nil));
    }

    #[test]
    fn resolves_string_escapes() {
        assert_eq!(
            read(r#""a\"b""#),
            Some(LiteralValue::Text(TextLiteral::new("a\"b")))
        );
        assert_eq!(
            read(r#""a\\b""#),
            Some(LiteralValue::Text(TextLiteral::new(r"a\b")))
        );
        assert_eq!(
            read(r#""""#),
            Some(LiteralValue::Text(TextLiteral::new("")))
        );
    }

    #[test]
    fn reads_floats_including_the_lisp_exponent_markers() {
        assert_eq!(
            read("1.5").and_then(|value| match value {
                LiteralValue::Float(float) => Some(float.get()),
                _ => None,
            }),
            Some(1.5)
        );
        assert!(matches!(read("1.5d0"), Some(LiteralValue::Float(_))));
        assert!(matches!(read("1e10"), Some(LiteralValue::Float(_))));
    }

    #[test]
    fn ordinary_symbols_are_not_literals() {
        for input in ["x", "1+", "-", "list", "*state*"] {
            assert_eq!(read(input), None, "{input} must not read as a literal");
        }
    }

    #[test]
    fn a_quoted_atom_is_a_datum_not_a_literal() {
        // `'5` denotes the object 5, but it is a quote form; reading it as a
        // literal would let a fix drop the quote and change what it means
        // somewhere the quote mattered.
        assert_eq!(read("'5"), None);
    }
}
