//! What a literal atom denotes.

use std::fmt;

use paredit_core_syntax::dialect::Dialect;

/// A keyword's name, without its leading colon and normalized by the dialect's
/// reader rules (case-folded for Common Lisp).
///
/// Wrapping it keeps `:foo` from being compared against the symbol `foo`:
/// they are different objects, and a lint that confused them would rewrite
/// working code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeywordName(String);

impl KeywordName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeywordName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, ":{}", self.0)
    }
}

/// A string literal's contents, with reader escapes already resolved.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextLiteral(String);

impl TextLiteral {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A float literal's value.
///
/// Deliberately not `Eq`/`Hash`: floats are read so a rule can see that a
/// divisor is `0.0` rather than `0`, but they are never folded and never
/// propagated, so nothing needs to key on one.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct FloatLiteral(f64);

impl FloatLiteral {
    #[must_use]
    pub const fn new(value: f64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// The value a literal atom denotes.
///
/// Everything the reader can interpret appears here, but only
/// [`LiteralValue::propagatable`] may travel through a binding — see
/// [`super::PropagatableValue`] for why the distinction is a type and not a
/// convention.
#[derive(Debug, Clone, PartialEq)]
pub enum LiteralValue {
    Integer(i128),
    Char(char),
    Keyword(KeywordName),
    /// `t` in Common Lisp, `#t`/`#f` in Scheme, `true`/`false` in Clojure.
    Boolean(bool),
    /// Common Lisp `nil`: false *and* the empty list. Scheme's `'()` is a
    /// separate thing from `#f`, which is why this is not folded into
    /// [`LiteralValue::Boolean`].
    Nil,
    /// Read, but never propagated: a Common Lisp string is mutable through
    /// `(setf (char s 0) …)`, so its identity matters and a copy would lie.
    Text(TextLiteral),
    /// Read, but never folded: reproducing an implementation's rounding order
    /// is a risk with no payoff for the lints that consume this.
    Float(FloatLiteral),
}

impl LiteralValue {
    /// Whether this value counts as true in a conditional.
    ///
    /// Common Lisp has exactly one false value, `nil`; Scheme and Racket have
    /// exactly one, `#f`, and treat the empty list as true. Getting this
    /// backwards would make a dead-branch lint delete the live branch.
    #[must_use]
    pub const fn is_truthy(&self, dialect: Dialect) -> bool {
        match dialect {
            Dialect::Scheme | Dialect::Racket => !matches!(self, Self::Boolean(false)),
            _ => !matches!(self, Self::Nil | Self::Boolean(false)),
        }
    }

    /// This value as something that may be propagated through a binding, or
    /// `None` for the two kinds that may not.
    #[must_use]
    pub fn propagatable(&self) -> Option<PropagatableValue> {
        match self {
            Self::Integer(value) => Some(PropagatableValue::Integer(*value)),
            Self::Char(value) => Some(PropagatableValue::Char(*value)),
            Self::Keyword(name) => Some(PropagatableValue::Keyword(name.clone())),
            Self::Boolean(value) => Some(PropagatableValue::Boolean(*value)),
            Self::Nil => Some(PropagatableValue::Nil),
            Self::Text(_) | Self::Float(_) => None,
        }
    }
}

/// The literals a constant may be propagated through a binding as.
///
/// Strings and floats are absent by construction rather than by a check the
/// propagation pass could forget: a string is mutable in place, so substituting
/// its value would lie about identity, and a float would invite folding.
/// Because the table can only hold this type, that decision cannot be
/// accidentally undone downstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropagatableValue {
    Integer(i128),
    Char(char),
    Keyword(KeywordName),
    Boolean(bool),
    Nil,
}

impl From<PropagatableValue> for LiteralValue {
    fn from(value: PropagatableValue) -> Self {
        match value {
            PropagatableValue::Integer(value) => Self::Integer(value),
            PropagatableValue::Char(value) => Self::Char(value),
            PropagatableValue::Keyword(name) => Self::Keyword(name),
            PropagatableValue::Boolean(value) => Self::Boolean(value),
            PropagatableValue::Nil => Self::Nil,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_lisp_has_exactly_one_false_value() {
        assert!(!LiteralValue::Nil.is_truthy(Dialect::CommonLisp));
        assert!(LiteralValue::Boolean(true).is_truthy(Dialect::CommonLisp));
        assert!(LiteralValue::Integer(0).is_truthy(Dialect::CommonLisp));
        assert!(LiteralValue::Text(TextLiteral::new("")).is_truthy(Dialect::CommonLisp));
    }

    #[test]
    fn scheme_treats_the_empty_list_as_true() {
        assert!(LiteralValue::Nil.is_truthy(Dialect::Scheme));
        assert!(!LiteralValue::Boolean(false).is_truthy(Dialect::Scheme));
    }

    #[test]
    fn strings_and_floats_are_not_propagatable() {
        assert_eq!(
            LiteralValue::Text(TextLiteral::new("x")).propagatable(),
            None
        );
        assert_eq!(
            LiteralValue::Float(FloatLiteral::new(1.5)).propagatable(),
            None
        );
    }

    #[test]
    fn every_other_literal_round_trips_through_propagation() {
        for value in [
            LiteralValue::Integer(7),
            LiteralValue::Char('a'),
            LiteralValue::Keyword(KeywordName::new("foo")),
            LiteralValue::Boolean(true),
            LiteralValue::Nil,
        ] {
            let propagated = value.propagatable().expect("propagatable");
            assert_eq!(LiteralValue::from(propagated), value);
        }
    }

    #[test]
    fn a_keyword_displays_with_its_colon() {
        assert_eq!(KeywordName::new("test").to_string(), ":test");
    }
}
