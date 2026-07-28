//! What the standard functions return.
//!
//! A hand-written table, and deliberately partial. Coverage is not the goal —
//! an absent function simply yields `Unknown`, which costs a deduction; a
//! wrong entry licenses a false one. Only functions whose CLHS return type is
//! unconditional at this lattice's granularity are listed. `car`, for
//! instance, is absent: it returns whatever was in the cons.

use super::super::model::Ty;

/// `(function name, return type)`, for functions whose result type does not
/// depend on their arguments.
const STANDARD_RETURNS: [(&str, Ty); 58] = [
    // Predicates — every CLHS predicate returns a generalized boolean, which
    // at this granularity is `t` or `nil`.
    ("null", Ty::Boolean),
    ("not", Ty::Boolean),
    ("eq", Ty::Boolean),
    ("eql", Ty::Boolean),
    ("equal", Ty::Boolean),
    ("equalp", Ty::Boolean),
    ("atom", Ty::Boolean),
    ("consp", Ty::Boolean),
    ("listp", Ty::Boolean),
    ("symbolp", Ty::Boolean),
    ("keywordp", Ty::Boolean),
    ("numberp", Ty::Boolean),
    ("integerp", Ty::Boolean),
    ("floatp", Ty::Boolean),
    ("rationalp", Ty::Boolean),
    ("characterp", Ty::Boolean),
    ("stringp", Ty::Boolean),
    ("vectorp", Ty::Boolean),
    ("arrayp", Ty::Boolean),
    ("functionp", Ty::Boolean),
    ("hash-table-p", Ty::Boolean),
    ("zerop", Ty::Boolean),
    ("plusp", Ty::Boolean),
    ("minusp", Ty::Boolean),
    ("evenp", Ty::Boolean),
    ("oddp", Ty::Boolean),
    ("=", Ty::Boolean),
    ("/=", Ty::Boolean),
    ("<", Ty::Boolean),
    (">", Ty::Boolean),
    ("<=", Ty::Boolean),
    (">=", Ty::Boolean),
    ("string=", Ty::Boolean),
    ("char=", Ty::Boolean),
    // Integer-valued.
    ("length", Ty::Integer),
    ("char-code", Ty::Integer),
    ("position", Ty::Integer),
    ("count", Ty::Integer),
    ("isqrt", Ty::Integer),
    ("hash-table-count", Ty::Integer),
    ("array-total-size", Ty::Integer),
    ("array-rank", Ty::Integer),
    // Character-valued.
    ("char", Ty::Character),
    ("schar", Ty::Character),
    ("code-char", Ty::Character),
    ("char-upcase", Ty::Character),
    ("char-downcase", Ty::Character),
    // String-valued.
    ("string", Ty::String),
    ("string-upcase", Ty::String),
    ("string-downcase", Ty::String),
    ("string-capitalize", Ty::String),
    ("symbol-name", Ty::String),
    ("princ-to-string", Ty::String),
    ("prin1-to-string", Ty::String),
    // Structural.
    ("cons", Ty::Cons),
    ("list", Ty::List),
    ("make-hash-table", Ty::HashTable),
    ("gensym", Ty::Symbol),
];

/// The return type of the standard function `name`, if this table knows it.
#[must_use]
pub fn standard_return_type(name: &str) -> Option<Ty> {
    STANDARD_RETURNS
        .iter()
        .find(|(function, _)| function.eq_ignore_ascii_case(name))
        .map(|(_, ty)| *ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(standard_return_type("LENGTH"), Some(Ty::Integer));
    }

    #[test]
    fn argument_dependent_functions_are_deliberately_absent() {
        // Each of these returns whatever it was given, so no unconditional
        // entry would be correct.
        for name in [
            "car", "cdr", "first", "aref", "gethash", "values", "identity",
        ] {
            assert_eq!(standard_return_type(name), None, "{name} must stay unknown");
        }
    }

    #[test]
    fn arithmetic_is_absent_because_the_result_type_follows_the_arguments() {
        // `(+ 1 2)` is an integer but `(+ 1 2.0)` is a float; the value layer
        // answers this precisely when the operands are constant.
        for name in ["+", "-", "*", "/", "max", "min", "abs"] {
            assert_eq!(standard_return_type(name), None, "{name} must stay unknown");
        }
    }

    #[test]
    fn no_name_is_listed_twice() {
        let mut names: Vec<&str> = STANDARD_RETURNS.iter().map(|(name, _)| *name).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "a duplicate entry would be ambiguous");
    }
}
