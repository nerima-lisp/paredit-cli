//! Which forms reassign a variable, per dialect.
//!
//! This knowledge was scattered across the lints that needed a piece of it —
//! `self_assignment_report` knew the four `setq`/`setf` spellings,
//! `setq_non_variable_report` knew two of them, `literal_place_report` knew
//! where a place sits in each. The binding table needs all of it in one place,
//! because "was this binding ever reassigned" is the single fact that decides
//! whether the value layer may propagate through it. Missing one spelling here
//! makes the value layer confidently wrong.

use crate::domain::dialect::Dialect;

/// Where the places sit among a form's arguments.
///
/// Index 0 is the operator, so every variant is expressed relative to that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacePositions {
    /// `(setq a 1 b 2)` — place/value pairs, places at the odd indices.
    Pairs,
    /// `(incf place)`, `(pop place)` — the first argument only.
    FirstArgument,
    /// `(push item place)` — the second argument only.
    SecondArgument,
    /// `(rotatef a b c)` — every argument.
    EveryArgument,
    /// `(shiftf a b new)` — every argument but the last, which is a value.
    EveryArgumentButLast,
}

impl PlacePositions {
    /// The child indices that hold places for a form with `child_count`
    /// children (including the operator at index 0).
    pub fn indices(self, child_count: usize) -> Vec<usize> {
        match self {
            Self::Pairs => (1..child_count).step_by(2).collect(),
            Self::FirstArgument => (1..child_count.min(2)).collect(),
            Self::SecondArgument => (2..child_count.min(3)).collect(),
            Self::EveryArgument => (1..child_count).collect(),
            Self::EveryArgumentButLast => (1..child_count.saturating_sub(1)).collect(),
        }
    }
}

/// One assignment operator: its head spelling and where its places are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssignmentForm {
    head: &'static str,
    places: PlacePositions,
}

impl AssignmentForm {
    const fn new(head: &'static str, places: PlacePositions) -> Self {
        Self { head, places }
    }

    pub const fn head(self) -> &'static str {
        self.head
    }

    pub const fn places(self) -> PlacePositions {
        self.places
    }
}

const COMMON_LISP: [AssignmentForm; 11] = [
    AssignmentForm::new("setq", PlacePositions::Pairs),
    AssignmentForm::new("psetq", PlacePositions::Pairs),
    AssignmentForm::new("setf", PlacePositions::Pairs),
    AssignmentForm::new("psetf", PlacePositions::Pairs),
    AssignmentForm::new("incf", PlacePositions::FirstArgument),
    AssignmentForm::new("decf", PlacePositions::FirstArgument),
    AssignmentForm::new("pop", PlacePositions::FirstArgument),
    AssignmentForm::new("push", PlacePositions::SecondArgument),
    AssignmentForm::new("pushnew", PlacePositions::SecondArgument),
    AssignmentForm::new("rotatef", PlacePositions::EveryArgument),
    AssignmentForm::new("shiftf", PlacePositions::EveryArgumentButLast),
];

const EMACS_LISP: [AssignmentForm; 12] = [
    AssignmentForm::new("setq", PlacePositions::Pairs),
    AssignmentForm::new("setq-default", PlacePositions::Pairs),
    AssignmentForm::new("setf", PlacePositions::Pairs),
    AssignmentForm::new("cl-psetq", PlacePositions::Pairs),
    AssignmentForm::new("cl-psetf", PlacePositions::Pairs),
    AssignmentForm::new("cl-incf", PlacePositions::FirstArgument),
    AssignmentForm::new("cl-decf", PlacePositions::FirstArgument),
    AssignmentForm::new("pop", PlacePositions::FirstArgument),
    AssignmentForm::new("push", PlacePositions::SecondArgument),
    AssignmentForm::new("cl-pushnew", PlacePositions::SecondArgument),
    AssignmentForm::new("cl-rotatef", PlacePositions::EveryArgument),
    AssignmentForm::new("cl-shiftf", PlacePositions::EveryArgumentButLast),
];

/// Scheme and Racket mutate through `set!` only.
const SET_BANG: [AssignmentForm; 1] = [AssignmentForm::new("set!", PlacePositions::Pairs)];

/// The assignment operators of `dialect`.
///
/// Empty for dialects whose local bindings cannot be reassigned at all —
/// Clojure `let` bindings are immutable, so nothing there can invalidate a
/// propagated value.
pub fn assignment_forms(dialect: Dialect) -> &'static [AssignmentForm] {
    match dialect {
        Dialect::CommonLisp => &COMMON_LISP,
        Dialect::EmacsLisp => &EMACS_LISP,
        Dialect::Scheme | Dialect::Racket => &SET_BANG,
        _ => &[],
    }
}

/// The assignment form named by `head`, if `dialect` has one.
pub fn assignment_form(dialect: Dialect, head: &str) -> Option<AssignmentForm> {
    assignment_forms(dialect)
        .iter()
        .copied()
        .find(|form| form.head().eq_ignore_ascii_case(head))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_places_are_the_odd_arguments() {
        // (setq a 1 b 2)
        assert_eq!(PlacePositions::Pairs.indices(5), vec![1, 3]);
    }

    #[test]
    fn push_writes_its_second_argument_and_pop_its_first() {
        // (push item place) / (pop place)
        assert_eq!(PlacePositions::SecondArgument.indices(3), vec![2]);
        assert_eq!(PlacePositions::FirstArgument.indices(2), vec![1]);
    }

    #[test]
    fn shiftf_treats_its_last_argument_as_a_value_but_rotatef_does_not() {
        // (shiftf a b new) rotates a <- b <- new; (rotatef a b c) writes all.
        assert_eq!(PlacePositions::EveryArgumentButLast.indices(4), vec![1, 2]);
        assert_eq!(PlacePositions::EveryArgument.indices(4), vec![1, 2, 3]);
    }

    #[test]
    fn a_form_with_no_arguments_has_no_places() {
        for positions in [
            PlacePositions::Pairs,
            PlacePositions::FirstArgument,
            PlacePositions::SecondArgument,
            PlacePositions::EveryArgument,
            PlacePositions::EveryArgumentButLast,
        ] {
            assert!(positions.indices(1).is_empty(), "{positions:?}");
        }
    }

    #[test]
    fn lookup_is_case_insensitive_within_a_dialect() {
        assert_eq!(
            assignment_form(Dialect::CommonLisp, "SETQ").map(AssignmentForm::places),
            Some(PlacePositions::Pairs)
        );
        assert_eq!(assignment_form(Dialect::CommonLisp, "set!"), None);
        assert_eq!(
            assignment_form(Dialect::Scheme, "set!").map(AssignmentForm::places),
            Some(PlacePositions::Pairs)
        );
    }

    #[test]
    fn a_dialect_with_immutable_bindings_has_no_assignment_forms() {
        assert!(assignment_forms(Dialect::Clojure).is_empty());
    }
}
