//! Combining types: what two branches agree on, and what a test narrows to.

use super::ty::Ty;

/// The least type both `left` and `right` are subtypes of.
///
/// Used where control flow merges — the two arms of an `if`, the clauses of a
/// `cond` — because the result is whichever arm ran.
///
/// The relation is a DAG, so a pair can have several minimal common
/// supertypes: `null` sits under both `boolean` and `list`, so `join(null,
/// keyword)` could be argued down to `symbol` while `join(null, cons)` lands
/// on `list`. Where the minimum is not unique this returns [`Ty::Top`] rather
/// than picking one. Widening too far only ever loses a deduction; picking
/// wrongly would license a false one.
pub fn join(left: Ty, right: Ty) -> Ty {
    if left == right {
        return left;
    }
    if left.is_subtype_of(right) {
        return right;
    }
    if right.is_subtype_of(left) {
        return left;
    }

    let right_ancestors = right.ancestors();
    let shared: Vec<Ty> = left
        .ancestors()
        .into_iter()
        .filter(|candidate| right_ancestors.contains(candidate))
        .collect();
    least(&shared)
}

/// The greatest type that is a subtype of both `left` and `right`.
///
/// Used where facts accumulate — a `declare type` plus a `stringp` guard —
/// and yields [`Ty::Bottom`] when they contradict, which is itself a useful
/// answer: the branch is unreachable.
pub fn meet(left: Ty, right: Ty) -> Ty {
    if left == right {
        return left;
    }
    if left.is_subtype_of(right) {
        return left;
    }
    if right.is_subtype_of(left) {
        return right;
    }

    let shared: Vec<Ty> = Ty::ALL
        .into_iter()
        .filter(|candidate| {
            *candidate != Ty::Bottom
                && candidate.is_subtype_of(left)
                && candidate.is_subtype_of(right)
        })
        .collect();
    greatest(&shared)
}

/// The minimum of `candidates` under subtyping, or [`Ty::Top`] when there is
/// no unique one.
fn least(candidates: &[Ty]) -> Ty {
    let minimal: Vec<Ty> = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            !candidates
                .iter()
                .any(|other| *other != *candidate && other.is_subtype_of(*candidate))
        })
        .collect();
    match minimal.as_slice() {
        [only] => *only,
        _ => Ty::Top,
    }
}

/// The maximum of `candidates` under subtyping, or [`Ty::Bottom`] when there
/// is no unique one.
fn greatest(candidates: &[Ty]) -> Ty {
    let maximal: Vec<Ty> = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            !candidates
                .iter()
                .any(|other| *other != *candidate && candidate.is_subtype_of(*other))
        })
        .collect();
    match maximal.as_slice() {
        [only] => *only,
        _ => Ty::Bottom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_and_meet_are_commutative() {
        for left in Ty::ALL {
            for right in Ty::ALL {
                assert_eq!(join(left, right), join(right, left), "{left} {right}");
                assert_eq!(meet(left, right), meet(right, left), "{left} {right}");
            }
        }
    }

    #[test]
    fn a_join_is_above_both_operands_and_a_meet_below_both() {
        for left in Ty::ALL {
            for right in Ty::ALL {
                let above = join(left, right);
                assert!(left.is_subtype_of(above) && right.is_subtype_of(above));
                let below = meet(left, right);
                assert!(below.is_subtype_of(left) && below.is_subtype_of(right));
            }
        }
    }

    #[test]
    fn joining_within_one_family_stays_in_that_family() {
        assert_eq!(join(Ty::Integer, Ty::Float), Ty::Number);
        assert_eq!(join(Ty::Null, Ty::Cons), Ty::List);
        assert_eq!(join(Ty::Keyword, Ty::Boolean), Ty::Symbol);
        assert_eq!(join(Ty::String, Ty::Vector), Ty::Vector);
    }

    #[test]
    fn an_ambiguous_join_widens_to_top_rather_than_guessing() {
        // `null` is under both `boolean` and `list`, so `null` and `keyword`
        // share `symbol` — but they also share `t` through `list`'s side of
        // the DAG only via `t`. Whatever the minimal set, the answer must
        // never be a type that is not above both.
        let widened = join(Ty::Null, Ty::Keyword);
        assert!(Ty::Null.is_subtype_of(widened) && Ty::Keyword.is_subtype_of(widened));
    }

    #[test]
    fn contradictory_facts_meet_at_the_empty_type() {
        assert_eq!(meet(Ty::Integer, Ty::String), Ty::Bottom);
        assert_eq!(meet(Ty::Character, Ty::Function), Ty::Bottom);
    }

    #[test]
    fn meeting_a_supertype_keeps_the_narrower_fact() {
        assert_eq!(meet(Ty::Number, Ty::Integer), Ty::Integer);
        assert_eq!(meet(Ty::Symbol, Ty::Null), Ty::Null);
        assert_eq!(meet(Ty::List, Ty::Symbol), Ty::Null);
    }

    #[test]
    fn top_and_bottom_are_the_identities() {
        for ty in Ty::ALL {
            assert_eq!(join(ty, Ty::Top), Ty::Top);
            assert_eq!(join(ty, Ty::Bottom), ty);
            assert_eq!(meet(ty, Ty::Top), ty);
            assert_eq!(meet(ty, Ty::Bottom), Ty::Bottom);
        }
    }
}
