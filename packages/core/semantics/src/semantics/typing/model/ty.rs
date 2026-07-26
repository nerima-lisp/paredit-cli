//! The coarse type lattice.

use std::fmt;

/// A Common Lisp type, at the granularity this layer models.
///
/// Deliberately coarse. The CLHS type system has range-restricted integers,
/// `satisfies` types, compound array specifiers, and user-defined types; none
/// of that is here, because the rules that consume this ask questions like
/// "is this argument definitely a number?" and a coarse answer they can trust
/// beats a precise one they cannot.
///
/// The relation is a DAG, not a tree: `null` is both a `symbol` and a `list`.
/// [`Ty::supertypes`] states the edges and everything else is derived from
/// them, so the two cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ty {
    /// `t` — every object.
    Top,
    Number,
    Integer,
    Float,
    Ratio,
    Character,
    Symbol,
    Keyword,
    /// `boolean` — `t` or `nil`.
    Boolean,
    /// `null` — the type whose only member is `nil`.
    Null,
    List,
    Cons,
    Vector,
    String,
    Function,
    HashTable,
    /// `nil` as a type — no object at all. The bottom of the lattice, and what
    /// a contradictory narrowing produces.
    Bottom,
}

impl Ty {
    /// Every type, for exhaustive testing of the lattice laws.
    pub const ALL: [Self; 17] = [
        Self::Top,
        Self::Number,
        Self::Integer,
        Self::Float,
        Self::Ratio,
        Self::Character,
        Self::Symbol,
        Self::Keyword,
        Self::Boolean,
        Self::Null,
        Self::List,
        Self::Cons,
        Self::Vector,
        Self::String,
        Self::Function,
        Self::HashTable,
        Self::Bottom,
    ];

    /// The immediate supertypes of this type — the edges of the DAG.
    ///
    /// `Null` having two parents is the whole reason this returns a slice:
    /// `nil` really is both a symbol and a list, and collapsing that would let
    /// a rule conclude `(symbolp x)` rules out `(listp x)`.
    #[must_use]
    pub const fn supertypes(self) -> &'static [Self] {
        match self {
            Self::Top | Self::Bottom => &[],
            Self::Number
            | Self::Character
            | Self::Symbol
            | Self::List
            | Self::Vector
            | Self::Function
            | Self::HashTable => &[Self::Top],
            Self::Integer | Self::Float | Self::Ratio => &[Self::Number],
            Self::Keyword | Self::Boolean => &[Self::Symbol],
            Self::Null => &[Self::Boolean, Self::List],
            Self::Cons => &[Self::List],
            Self::String => &[Self::Vector],
        }
    }

    /// The CLHS type name, as it would be written in a `the` or `declare`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Top => "t",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::Float => "float",
            Self::Ratio => "ratio",
            Self::Character => "character",
            Self::Symbol => "symbol",
            Self::Keyword => "keyword",
            Self::Boolean => "boolean",
            Self::Null => "null",
            Self::List => "list",
            Self::Cons => "cons",
            Self::Vector => "vector",
            Self::String => "string",
            Self::Function => "function",
            Self::HashTable => "hash-table",
            Self::Bottom => "nil",
        }
    }

    /// The type a CLHS type name denotes, if this layer models it.
    ///
    /// An unmodelled name yields `None` rather than [`Ty::Top`]: "I don't know
    /// this type" and "this could be anything" are different answers, and only
    /// the caller knows which one is safe where it stands.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str().eq_ignore_ascii_case(name))
    }

    /// This type and every type above it, `Bottom` included by convention
    /// since it is below everything.
    #[must_use]
    pub fn ancestors(self) -> Vec<Self> {
        let mut seen = vec![self];
        let mut frontier = vec![self];
        while let Some(current) = frontier.pop() {
            for parent in current.supertypes() {
                if !seen.contains(parent) {
                    seen.push(*parent);
                    frontier.push(*parent);
                }
            }
        }
        seen
    }

    /// Whether every object of this type is also of type `other`.
    #[must_use]
    pub fn is_subtype_of(self, other: Self) -> bool {
        self == Self::Bottom || self.ancestors().contains(&other)
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtyping_is_reflexive_and_transitive() {
        for left in Ty::ALL {
            assert!(
                left.is_subtype_of(left),
                "{left} is not a subtype of itself"
            );
            for middle in Ty::ALL {
                for right in Ty::ALL {
                    if left.is_subtype_of(middle) && middle.is_subtype_of(right) {
                        assert!(
                            left.is_subtype_of(right),
                            "{left} <: {middle} <: {right} but not {left} <: {right}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn top_is_above_everything_and_bottom_below_it() {
        for ty in Ty::ALL {
            assert!(ty.is_subtype_of(Ty::Top), "{ty} is not below t");
            assert!(Ty::Bottom.is_subtype_of(ty), "nil is not below {ty}");
        }
    }

    #[test]
    fn null_is_both_a_symbol_and_a_list() {
        // `nil` really is both, and a rule that assumes otherwise will make
        // wrong deductions from `symbolp`/`listp`.
        assert!(Ty::Null.is_subtype_of(Ty::Symbol));
        assert!(Ty::Null.is_subtype_of(Ty::List));
        assert!(!Ty::Cons.is_subtype_of(Ty::Symbol));
    }

    #[test]
    fn a_string_is_a_vector_and_an_integer_a_number() {
        assert!(Ty::String.is_subtype_of(Ty::Vector));
        assert!(Ty::Integer.is_subtype_of(Ty::Number));
        assert!(!Ty::Integer.is_subtype_of(Ty::Float));
    }

    #[test]
    fn names_round_trip_and_unknown_names_stay_unknown() {
        for ty in Ty::ALL {
            assert_eq!(Ty::from_name(ty.as_str()), Some(ty));
        }
        assert_eq!(Ty::from_name("HASH-TABLE"), Some(Ty::HashTable));
        assert_eq!(Ty::from_name("simple-array"), None);
        assert_eq!(Ty::from_name("my-struct"), None);
    }
}
