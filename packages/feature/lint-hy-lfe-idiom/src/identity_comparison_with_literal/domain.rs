//! Which operands of Hy's `is` are the literals CPython itself warns about.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionView};

use crate::shared::{atom_text, is_hash_list};

/// Hy only. `is` compiles to Python's `is`, an object-identity test.
pub const DIALECTS: [Dialect; 1] = [Dialect::Hy];

/// Both spellings of each operator.
///
/// Hy mangles `-` to `_`, so `is-not` and `is_not` name the same operator and
/// both parse; the audit corpus uses `is-not` in 82 files and `is_not` in 4.
/// `head_key` returns a non-Common-Lisp head *verbatim*, with no folding of
/// any kind, so a rule that listed only the hyphenated spelling would miss the
/// underscored one silently.
pub const HEAD_NAMES: [&str; 3] = ["is", "is-not", "is_not"];

/// The three singletons for which `is` is the *correct* operator, and which
/// CPython therefore does not warn about.
///
/// Kept although it is, today, redundant: mutation-testing removed the check
/// below that consults it and killed no test, because `None`, `True` and
/// `False` are neither string nor number literals and so fail
/// [`is_value_literal`]'s other two predicates anyway. It stays because it is
/// the *specification* — CPython's rule is literally "warn for a constant that
/// is not one of these" — and because widening `is_number_literal` even
/// slightly would otherwise start reporting the one spelling this rule exists
/// to recommend. [`a_singleton_is_not_a_literal_by_two_independent_routes`]
/// pins both routes so the redundancy stays visible rather than becoming a
/// silent dependency.
const SINGLETONS: [&str; 3] = ["None", "True", "False"];

/// Whether an operand is a value literal whose identity is an interning
/// accident rather than a property of the program.
///
/// This mirrors CPython's own `SyntaxWarning`, which fires for `int`, `str`,
/// `bytes`, `float`, `complex` and tuple constants and not for `None`, `True`
/// or `False`. Verified by running Hy 1.3.1 on CPython 3.14.6:
///
/// ```text
/// (is small 5)  =>  SyntaxWarning: "is" with 'int' literal. Did you mean "=="?
/// (is n None)   =>  no warning
/// ```
#[must_use]
pub fn is_value_literal(view: &ExpressionView) -> bool {
    // `#(…)` is a tuple constant, which CPython warns about too.
    if is_hash_list(view, Delimiter::Paren) {
        return true;
    }
    let Some(text) = atom_text(view) else {
        return false;
    };
    if SINGLETONS.contains(&text) {
        return false;
    }
    is_string_literal(text) || is_number_literal(text)
}

/// A `"…"`, `b"…"` or `r"…"` literal, but not an f-string.
///
/// An f-string is not a constant — it is a runtime concatenation — so
/// CPython does not warn about it, and neither does this. (This workspace's
/// reader cannot represent an *interpolated* f-string at all, so in practice
/// the file would have failed to parse; the prefix is still refused here so
/// the predicate is honest on its own terms.)
fn is_string_literal(text: &str) -> bool {
    let body = text
        .strip_prefix("rb")
        .or_else(|| text.strip_prefix("br"))
        .or_else(|| text.strip_prefix('b'))
        .or_else(|| text.strip_prefix('r'))
        .unwrap_or(text);
    if text.starts_with('f') || text.starts_with("fr") || text.starts_with("rf") {
        return false;
    }
    body.starts_with('"') && body.len() >= 2 && body.ends_with('"')
}

/// A numeric literal, in any spelling Python accepts.
///
/// Requires a digit somewhere so that a bare `-` or `.` is not read as one,
/// and refuses anything with a character a number cannot contain — which is
/// what keeps an ordinary identifier like `x2` or `-count` out.
fn is_number_literal(text: &str) -> bool {
    let body = text.strip_prefix(['-', '+']).unwrap_or(text);
    if body.is_empty() {
        return false;
    }
    // A number starts with a digit or a bare decimal point.
    if !body.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return false;
    }
    body.chars().any(|c| c.is_ascii_digit())
        && body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}
