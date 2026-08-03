//! `class-allocated-slot-with-initarg`: a `defclass` slot that is both
//! `:allocation :class` and reachable by an `:initarg`.
//!
//! # Why the two options cannot both mean what they look like
//!
//! CLHS 7.5.3 makes a `:class`-allocated slot **one storage location shared by
//! every instance**, and CLHS 7.1 makes an `:initarg` a value supplied *per
//! construction*. Putting both on one slot means each `make-instance` writes the
//! value every other instance — including every instance already built — reads
//! back.
//!
//! Verified against SBCL 2.6.0:
//!
//! ```text
//! (defclass shared-init ()
//!   ((registry :initarg :registry :initform :none :allocation :class)))
//! (defvar *x* (make-instance 'shared-init :registry :from-x))
//! (defvar *y* (make-instance 'shared-init))
//! (defvar *z* (make-instance 'shared-init :registry :from-z))
//! ```
//!
//! prints
//!
//! ```text
//!    x registry = :FROM-X
//!    y registry = :FROM-X        ; Y was built with no initarg at all
//!    after making Z with :registry :from-z, X's registry = :FROM-Z
//! ```
//!
//! `*y*`, built with no initarg, reads `*x*`'s value; and building `*z*` changed
//! what `*x*` reads, retroactively. Nothing signals, at either definition time
//! or construction time.
//!
//! # Why the mutation rule this replaced was dropped
//!
//! The candidate this rule stands in for was "`(setf (slot-value o 'x) …)` on a
//! `:class` slot changes it for every instance". That is true — SBCL confirms
//! it — but it is also the entire point of class allocation: a shared counter
//! incremented through any instance is the idiom, not the bug. There is no shape
//! that separates the two, so no rule was written for it.
//!
//! The `:initarg` case is different because the two options *contradict each
//! other*: one says per-instance, the other says per-class, and no reading of
//! the program makes both true.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};

use crate::support::symbol_name;

/// One slot that is shared by every instance and settable per construction.
#[derive(Debug, Clone)]
pub struct ContradictorySlot {
    /// The span of the slot specification, not of the whole `defclass`.
    pub span: ByteSpan,
    pub slot: String,
    /// The initarg keyword the slot accepts, as written.
    pub initarg: String,
}

impl ContradictorySlot {
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "slot {} is :allocation :class, so every instance shares one location, yet it also \
             accepts the initarg {}: each make-instance overwrites the value every other \
             instance reads, including instances already built",
            self.slot, self.initarg
        )
    }
}

/// Examines one matched `defclass`.
///
/// Local to the form and to its slot list: nothing outside the `defclass` is
/// read, so this is constant work per slot and no correlation at all.
#[must_use]
pub fn examine_defclass(view: &ExpressionView) -> Vec<ContradictorySlot> {
    let mut found = Vec::new();
    if !list_head(view).is_some_and(|head| symbol_is(head, "defclass")) {
        return found;
    }
    // There is deliberately **no reader-conditional guard here**, and that is a
    // mutation-testing result rather than an oversight. One was written, found
    // to kill no test, narrowed to the positional prefix, and found to kill no
    // test again — so it was chased to the reason and removed.
    //
    // The reason is that `defclass`'s geometry is self-checking. A folded `#+`
    // atom shifts the later children, but the only things it can put at index 3
    // are the superclass list, an empty list, or the folded atom itself, and the
    // two shape requirements below reject all three: the child must be a `(…)`
    // list, and a slot in it must itself be a `(name option…)` list carrying
    // both `:initarg` and `:allocation :class`. A superclass list holds bare
    // symbols; a folded atom is not a list at all. No shift produces a report.
    //
    // Keeping the guard therefore bought nothing and cost a real finding on
    // `(defclass c () ((x …)) #+sbcl (:documentation "d"))`, where the `#+` is
    // *after* the slot list and shifts nothing.
    //
    // (defclass name (super…) (slot…) option…)
    let Some(slots) = view.children.get(3).filter(|child| is_paren_list(child)) else {
        return found;
    };
    for slot in &slots.children {
        if let Some(item) = examine_slot(slot) {
            found.push(item);
        }
    }
    found
}

/// One `(name option…)` slot specification.
///
/// A bare `name` slot carries no options and can never match.
///
/// There is deliberately **no reader-conditional guard here**. One was written
/// and mutation-testing found it killed nothing, so it was chased rather than
/// left: a folded `#+sbcl :initarg` atom does not start with `:`, so the option
/// stride below steps over it and reads no option out of it, and a folded atom
/// in a *value* position is read as that option's value and compared against
/// `:class`, which it is not. Both shapes are already declined by the stride.
/// The guard was dead code and is gone.
///
/// The shape check that *is* here has to test the reader prefixes as well as
/// the delimiter. `#(x :initarg :x :allocation :class)` is a **vector**, and
/// under this parser a vector is a paren list carrying a `#` prefix — so
/// `is_paren_list` alone accepts it and the slot walk reads it as a slot
/// specification. It is malformed Common Lisp, and a rule about class
/// allocation must not report on it as though it were a slot. Mutation-testing
/// `is_paren_list` on its own killed nothing, because a bare atom is already
/// declined by having no children at all; the vector is the input that gives
/// this check something to do.
fn examine_slot(view: &ExpressionView) -> Option<ContradictorySlot> {
    if !is_paren_list(view) || !view.reader_prefixes.is_empty() {
        return None;
    }
    let name = view.children.first().and_then(symbol_name)?;
    let mut allocation_is_class = false;
    let mut initarg = None;

    // Options are walked two at a time, so a *value* that happens to look like a
    // keyword is never read as an option itself. Reading them one at a time
    // makes `:allocation :class` look like an option named `:class`.
    let mut index = 1;
    while index < view.children.len() {
        let Some(key) = symbol_name(&view.children[index]).filter(|key| key.starts_with(':'))
        else {
            index += 1;
            continue;
        };
        let value = view.children.get(index + 1);
        match key.as_str() {
            ":allocation" => {
                allocation_is_class = value.and_then(symbol_name).as_deref() == Some(":class");
            }
            ":initarg" if initarg.is_none() => {
                initarg = value.and_then(symbol_name);
            }
            _ => {}
        }
        index += 2;
    }

    let initarg = initarg?;
    allocation_is_class.then_some(ContradictorySlot {
        span: view.span,
        slot: name,
        initarg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn findings(source: &str) -> Vec<ContradictorySlot> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        tree.root_view()
            .children
            .iter()
            .flat_map(examine_defclass)
            .collect()
    }

    #[test]
    fn flags_a_class_slot_that_also_accepts_an_initarg() {
        let found =
            findings("(defclass registry ()\n  ((entries :initarg :entries :allocation :class)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].slot, "entries");
        assert_eq!(found[0].initarg, ":entries");
        assert!(found[0].message().contains("already built"));
    }

    #[test]
    fn the_option_order_does_not_matter() {
        assert_eq!(
            findings("(defclass c () ((x :allocation :class :initform 0 :initarg :x)))").len(),
            1
        );
    }

    #[test]
    fn flags_each_offending_slot_of_a_class() {
        assert_eq!(
            findings(
                "(defclass c ()\n\
                 \x20 ((a :initarg :a :allocation :class)\n\
                 \x20  (b :initarg :b)\n\
                 \x20  (c :initarg :c :allocation :class)))"
            )
            .len(),
            2
        );
    }

    #[test]
    fn folds_case_in_the_allocation_value() {
        assert_eq!(
            findings("(defclass c () ((x :INITARG :x :ALLOCATION :CLASS)))").len(),
            1
        );
    }

    // -- the near misses -------------------------------------------------------

    /// A shared slot with no initarg is the whole point of class allocation.
    #[test]
    fn accepts_a_class_slot_with_no_initarg() {
        assert!(
            findings("(defclass counted () ((total :initform 0 :allocation :class)))").is_empty()
        );
    }

    #[test]
    fn accepts_an_instance_slot_with_an_initarg() {
        assert!(findings("(defclass c () ((x :initarg :x :initform 0)))").is_empty());
        assert!(findings("(defclass c () ((x :initarg :x :allocation :instance)))").is_empty());
    }

    #[test]
    fn accepts_a_bare_slot_name() {
        assert!(findings("(defclass c () (x y z))").is_empty());
    }

    /// A slot spec has to be a **`(…)` list**, not merely something with
    /// children. A `#(…)` vector reads with children under this parser, so
    /// without the `is_paren_list` check a vector written where a slot belongs
    /// would be read as a slot and reported. It is malformed Common Lisp either
    /// way, and a lint rule may not report on it as though it were a class slot.
    ///
    /// Mutation-tested: this is the input that gives that check something to do;
    /// a bare atom is already declined because an atom has no children.
    #[test]
    fn a_slot_spec_that_is_not_a_paren_list_is_declined() {
        assert!(
            findings("(defclass c () (#(x :initarg :x :allocation :class)))").is_empty(),
            "a vector is a paren list with a # prefix, and is not a slot specification"
        );
    }

    #[test]
    fn accepts_a_class_with_no_slots() {
        assert!(findings("(defclass c ())").is_empty());
        assert!(findings("(defclass c (base) ())").is_empty());
    }

    /// The two-at-a-time stride's reason for existing: `:class` here is the
    /// *value* of `:allocation`, and reading options one at a time would see a
    /// slot option named `:class` and then read `:initarg` as a value.
    #[test]
    fn an_option_value_is_never_read_as_an_option() {
        // `:documentation ":initarg"` is a string, but an author writing
        // `:reader :initarg` would trip a one-at-a-time reader.
        assert!(findings("(defclass c () ((x :reader :initarg :allocation :class)))").is_empty());
    }

    /// A class-allocated slot reachable through `:default-initargs` rather than
    /// an explicit `:initarg` is not reported: the initarg is then a property of
    /// the class, applied once, which is a different question.
    #[test]
    fn accepts_a_default_initarg_on_a_class_slot() {
        assert!(
            findings("(defclass c ()\n  ((x :allocation :class))\n  (:default-initargs :x 0))")
                .is_empty()
        );
    }

    /// A folded reader conditional before the slot list shifts every later
    /// index — and cannot make this rule report, because the only things a shift
    /// can put at index 3 are a superclass list (bare symbols), an empty list,
    /// or the folded atom itself, and none of them survives the shape checks.
    ///
    /// This is why there is no reader-conditional guard in
    /// [`examine_defclass`]: mutation-testing one killed nothing, twice, and the
    /// reason is here rather than in a guard.
    #[test]
    fn a_folded_reader_conditional_cannot_produce_a_wrong_slot_list() {
        for source in [
            // The superclass list lands at index 3.
            "(defclass c #+sbcl (base) () ((x :initarg :x :allocation :class)))",
            // A folded atom lands at index 3; it is not a list.
            "(defclass c () #+sbcl ((x :initarg :x :allocation :class)))",
            // An empty list lands at index 3.
            "(defclass c #+sbcl () () ((x :initarg :x :allocation :class)))",
            // The head itself is folded, so it names no defclass.
            "(#+sbcl defclass #-sbcl defstruct c () ((x :initarg :x :allocation :class)))",
        ] {
            assert!(findings(source).is_empty(), "for {source}");
        }
    }

    /// ...and one *after* the slot list shifts nothing, so the rule must still
    /// report. A whole-form guard would have silently lost this finding, which
    /// is what made removing it the right call rather than only the cheap one.
    #[test]
    fn a_reader_conditional_after_the_slot_list_is_harmless() {
        assert_eq!(
            findings(
                "(defclass c ()\n  ((x :initarg :x :allocation :class))\n\
                 \x20 #+sbcl (:documentation \"d\"))"
            )
            .len(),
            1,
            "the slots are exactly where they are counted"
        );
    }

    /// Inside a slot, the option stride handles a folded atom on its own: it
    /// does not begin with `:`, so it is stepped over and no option is read out
    /// of it. This is why [`examine_slot`] carries no reader-conditional guard.
    #[test]
    fn a_reader_conditional_inside_a_slot_is_handled_by_the_option_stride() {
        assert!(findings("(defclass c () ((x #+sbcl :initarg :x :allocation :class)))").is_empty());
    }

    #[test]
    fn a_slot_option_with_no_value_is_not_read_past_the_end() {
        assert!(findings("(defclass c () ((x :allocation)))").is_empty());
        assert!(findings("(defclass c () ((x :initarg)))").is_empty());
    }
}
