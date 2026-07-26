//! The result of asking what an expression evaluates to.

use crate::domain::dialect::Dialect;

use super::literal::LiteralValue;

/// What an expression evaluates to, when that is provable.
///
/// [`Value::Unknown`] is not "an error" and not "no value" — it is the honest
/// answer for every expression this layer cannot prove something about, which
/// is most of them. Rules must treat it as "say nothing": the whole layer is
/// only useful if a `Known` can be trusted absolutely, so anything short of a
/// proof degrades to `Unknown` rather than to a guess.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Known(LiteralValue),
    Unknown,
}

impl Value {
    pub const fn known(value: LiteralValue) -> Self {
        Self::Known(value)
    }

    pub const fn as_known(&self) -> Option<&LiteralValue> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown => None,
        }
    }

    /// The integer this evaluates to, if it provably evaluates to one.
    pub const fn as_integer(&self) -> Option<i128> {
        match self {
            Self::Known(LiteralValue::Integer(value)) => Some(*value),
            _ => None,
        }
    }

    /// Whether this provably evaluates to the integer `expected`.
    ///
    /// The question a lint actually asks — "is this divisor zero?", "is this
    /// radix ten?" — phrased so a caller cannot forget to handle `Unknown`.
    pub fn is_integer(&self, expected: i128) -> bool {
        self.as_integer() == Some(expected)
    }

    /// Whether this provably evaluates to something true, false, or neither
    /// (because it is not `Known`).
    pub fn truthiness(&self, dialect: Dialect) -> Option<bool> {
        self.as_known().map(|value| value.is_truthy(dialect))
    }

    pub const fn is_known(&self) -> bool {
        matches!(self, Self::Known(_))
    }
}

impl From<Option<LiteralValue>> for Value {
    fn from(value: Option<LiteralValue>) -> Self {
        value.map_or(Self::Unknown, Self::Known)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_answers_nothing() {
        let unknown = Value::Unknown;
        assert_eq!(unknown.as_integer(), None);
        assert!(!unknown.is_integer(0));
        assert_eq!(unknown.truthiness(Dialect::CommonLisp), None);
        assert!(!unknown.is_known());
    }

    #[test]
    fn a_known_integer_answers_only_its_own_value() {
        let value = Value::known(LiteralValue::Integer(10));
        assert!(value.is_integer(10));
        assert!(!value.is_integer(0));
    }

    #[test]
    fn truthiness_follows_the_dialect() {
        let nil = Value::known(LiteralValue::Nil);
        assert_eq!(nil.truthiness(Dialect::CommonLisp), Some(false));
        assert_eq!(nil.truthiness(Dialect::Scheme), Some(true));
    }

    #[test]
    fn a_missing_literal_reads_as_unknown() {
        assert_eq!(Value::from(None), Value::Unknown);
        assert_eq!(
            Value::from(Some(LiteralValue::Nil)),
            Value::Known(LiteralValue::Nil)
        );
    }
}
