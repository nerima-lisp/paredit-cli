//! Narrowing a type from a predicate that guarded the branch.

use super::super::model::Ty;

/// The type a predicate confirms when it is true.
///
/// Only the "true" direction is modelled. Knowing `(stringp x)` was false
/// tells you `x` is not a string, which this coarse lattice cannot express as
/// a type — there is no "everything except string" — so the false branch
/// narrows to nothing rather than to something wrong. The one exception is
/// [`narrowed_by_predicate`]'s handling of `atom`, whose complement *is* a
/// modelled type.
#[must_use]
pub fn type_predicate(name: &str) -> Option<Ty> {
    const PREDICATES: [(&str, Ty); 13] = [
        ("null", Ty::Null),
        ("consp", Ty::Cons),
        ("listp", Ty::List),
        ("symbolp", Ty::Symbol),
        ("keywordp", Ty::Keyword),
        ("numberp", Ty::Number),
        ("integerp", Ty::Integer),
        ("floatp", Ty::Float),
        ("rationalp", Ty::Ratio),
        ("characterp", Ty::Character),
        ("stringp", Ty::String),
        ("vectorp", Ty::Vector),
        ("functionp", Ty::Function),
    ];
    PREDICATES
        .iter()
        .find(|(predicate, _)| predicate.eq_ignore_ascii_case(name))
        .map(|(_, ty)| *ty)
}

/// What `name` proves about its argument on the given branch.
///
/// `atom` is the one predicate whose *false* branch is expressible: everything
/// that is not an atom is a cons.
#[must_use]
pub fn narrowed_by_predicate(name: &str, holds: bool) -> Option<Ty> {
    if name.eq_ignore_ascii_case("atom") {
        return (!holds).then_some(Ty::Cons);
    }
    if name.eq_ignore_ascii_case("consp") && !holds {
        // Not a cons says nothing in this lattice: it could be any atom.
        return None;
    }
    holds.then(|| type_predicate(name)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_true_predicate_narrows_to_its_type() {
        assert_eq!(narrowed_by_predicate("stringp", true), Some(Ty::String));
        assert_eq!(narrowed_by_predicate("INTEGERP", true), Some(Ty::Integer));
    }

    #[test]
    fn a_false_predicate_generally_proves_nothing_expressible() {
        assert_eq!(narrowed_by_predicate("stringp", false), None);
        assert_eq!(narrowed_by_predicate("consp", false), None);
    }

    #[test]
    fn a_non_atom_is_provably_a_cons() {
        assert_eq!(narrowed_by_predicate("atom", false), Some(Ty::Cons));
        assert_eq!(narrowed_by_predicate("atom", true), None);
    }

    #[test]
    fn an_unrecognized_predicate_narrows_nothing() {
        assert_eq!(type_predicate("my-struct-p"), None);
        assert_eq!(narrowed_by_predicate("my-struct-p", true), None);
    }
}
