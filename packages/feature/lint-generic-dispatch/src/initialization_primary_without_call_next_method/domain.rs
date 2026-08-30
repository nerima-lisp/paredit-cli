//! `initialization-primary-without-call-next-method`: a **primary** method on
//! one of the instance-initialization generic functions whose body never reaches
//! `call-next-method`.
//!
//! # What actually happens
//!
//! CLHS 7.1.7 makes `initialize-instance`'s system-supplied primary method the
//! one that calls `shared-initialize`, and CLHS 7.1.4 makes `shared-initialize`
//! the thing that fills slots from `:initarg`s and `:initform`s. A user-written
//! *primary* method overrides that system method rather than running beside it,
//! so a primary that does not call `call-next-method` runs instead of the whole
//! initialization protocol. Nothing fills the slots.
//!
//! Verified against SBCL 2.6.0:
//!
//! ```text
//! (defclass base () ((a :initarg :a :initform :default-a) (b :initform :default-b)))
//! (defclass no-cnm (base) ())
//! (defmethod initialize-instance ((o no-cnm) &key &allow-other-keys) o)
//! (let ((o (make-instance 'no-cnm :a 7)))
//!   (list (slot-boundp o 'a) (slot-boundp o 'b)))
//! ```
//!
//! prints `slot A bound? NIL` and `slot B bound? NIL`. The `:a 7` was passed and
//! discarded, and the `:initform`s never ran. The same class with
//! `(call-next-method)` in the body gives `slot A 7   slot B :DEFAULT-B`.
//!
//! `shared-initialize` behaves the same way: a primary method on it that does
//! not call the next one leaves both slots unbound.
//!
//! # What it deliberately does not report
//!
//! - **`:after` methods.** `(defmethod initialize-instance :after ((o c) &key) …)`
//!   is *the* idiom, and an auxiliary method is under no obligation to call
//!   anything. Verified: the `:after` class above initializes correctly.
//! - **`:before` methods**, for the same reason.
//! - **`:around` methods.** An `:around` that short-circuits is a real idiom — a
//!   cache, a guard that refuses — and it is already the subject of
//!   `around-method-missing-call-next-method` in
//!   `paredit-feature-lint-object-system`
//!   (`src/around_method_missing_call_next_method/domain.rs:75-88`, which returns
//!   early unless a qualifier is `:around`). This rule fires only when the
//!   qualifier list is **empty**, so the two cannot both report on one method.
//!
//! # What the corpus audit changed
//!
//! The first version of this rule modelled five generic functions and accepted
//! only `call-next-method`. Over 1396 third-party files it produced **five
//! findings, all of them in SBCL's own `src/pcl/init.lisp`** — the file that
//! *implements* the initialization protocol, where every one of them was
//! correct. Two narrowings came out of reading them, and both are principled
//! rather than a suppression list:
//!
//! - a direct call to **`shared-initialize`** satisfies the rule. PCL's
//!   `initialize-instance` primary is `(apply #'shared-initialize instance t
//!   initargs)`, which is what the system-supplied primary is *defined* to do
//!   (CLHS 7.1.7). It reaches the protocol without the indirection.
//! - **`shared-initialize`**, `update-instance-for-different-class` and
//!   `update-instance-for-redefined-class` are no longer modelled at all. See
//!   `INITIALIZATION_GENERICS`.
//!
//! With both, the rule reports nothing anywhere in that corpus.
//!
//! `call-next-method` counts wherever it appears in the body, a bare function
//! designator included: `(apply #'call-next-method args)` reaches it. An
//! occurrence inside quoted data also silences the rule, which costs a finding
//! and never invents one.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{list_head, symbol_is};

use crate::support::{mentions, method_parts, symbol_name};

/// The two generic functions whose system-supplied primary method has exactly
/// one documented job: call `shared-initialize`.
///
/// CLHS 7.1.7 for `initialize-instance` — "the system-supplied primary method
/// ... calls `shared-initialize`" — and CLHS 7.3 for `reinitialize-instance`,
/// which says the same with `nil` for the slot-names argument. Because their
/// contract is that one call, a primary method that makes neither it nor
/// `call-next-method` provably skips the whole protocol.
///
/// **`shared-initialize` itself is deliberately not here, and neither are
/// `update-instance-for-different-class` and
/// `update-instance-for-redefined-class`.** That is the corpus audit's doing:
/// all five findings this rule produced over 1396 files were in SBCL's
/// `src/pcl/init.lisp`, and the three excluded generics are the ones where PCL's
/// own primary method legitimately reaches neither call. `shared-initialize` is
/// where slots are *filled*, so its primary fills them directly through
/// `slot-value-using-class`; there is no way to tell a correct
/// re-implementation of the filling from a method that forgot to chain, so the
/// rule declines to guess. The same is true of the two `update-instance-for-*`
/// generics, whose primaries call `shared-initialize` — which is why the
/// remaining two are still worth reporting on, and why a direct
/// `shared-initialize` call satisfies this rule.
const INITIALIZATION_GENERICS: [&str; 2] = ["initialize-instance", "reinitialize-instance"];

/// Whether `name` is one of the generic functions this rule models.
///
/// Public so the corpus audit can count the rule's *internal* denominator —
/// how many primary methods on these two generics a corpus contains at all —
/// rather than only the textual occurrences of their names.
#[must_use]
pub fn is_initialization_generic(name: &str) -> bool {
    INITIALIZATION_GENERICS.contains(&name)
}

/// The calls that mean the initialization protocol still runs.
///
/// `call-next-method` reaches the system-supplied primary, which calls
/// `shared-initialize`. A direct `shared-initialize` call reaches it without the
/// indirection, which is exactly what the system primary does and what SBCL's
/// own `(defmethod initialize-instance ((instance slot-object) &rest initargs)
/// (apply #'shared-initialize instance t initargs))` does. Either one leaves the
/// slots filled; neither one is the defect.
const SATISFYING_CALLS: [&str; 2] = ["call-next-method", "shared-initialize"];

/// One primary initialization method that never chains.
#[derive(Debug, Clone)]
pub struct MissingChain {
    pub span: ByteSpan,
    /// The initialization generic function this method specializes.
    pub generic: String,
}

impl MissingChain {
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "this primary {} method never calls call-next-method: it replaces the \
             system-supplied primary method, so shared-initialize never runs and no :initarg or \
             :initform reaches a slot",
            self.generic
        )
    }
}

/// Examines one matched `defmethod`.
///
/// Local to the form: nothing here reads any other node in the file, so the cost
/// is the size of the method body and no more.
#[must_use]
pub fn examine_defmethod(view: &ExpressionView) -> Option<MissingChain> {
    if !list_head(view).is_some_and(|head| symbol_is(head, "defmethod")) {
        return None;
    }
    let parts = method_parts(view)?;
    // Auxiliary methods are under no obligation, and `:around` belongs to
    // `around-method-missing-call-next-method` in the object-system package.
    if !parts.is_primary() {
        return None;
    }
    let generic = symbol_name(parts.name)?;
    if !is_initialization_generic(&generic) {
        return None;
    }
    if parts
        .body
        .iter()
        .any(|form| mentions(form, &SATISFYING_CALLS))
    {
        return None;
    }
    Some(MissingChain {
        span: view.span,
        generic,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn findings(source: &str) -> Vec<MissingChain> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        tree.root_view()
            .children
            .iter()
            .filter_map(examine_defmethod)
            .collect()
    }

    #[test]
    fn flags_a_primary_initialize_instance_that_never_chains() {
        let found = findings("(defmethod initialize-instance ((o widget) &key) (setup o))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].generic, "initialize-instance");
        assert!(found[0].message().contains("shared-initialize"));
    }

    #[test]
    fn flags_an_empty_bodied_primary() {
        assert_eq!(
            findings("(defmethod initialize-instance ((o widget) &key))").len(),
            1
        );
    }

    #[test]
    fn flags_every_initialization_generic() {
        for generic in INITIALIZATION_GENERICS {
            assert_eq!(
                findings(&format!("(defmethod {generic} ((o widget) &key) o)")).len(),
                1,
                "{generic}"
            );
        }
    }

    /// The three generics the corpus audit removed, each with its own reason.
    ///
    /// `shared-initialize` is where slots are filled, so a primary that fills
    /// them directly is doing the system primary's job rather than skipping it;
    /// the two `update-instance-for-*` primaries call `shared-initialize`
    /// themselves. All three shapes appear in SBCL's `src/pcl/init.lisp`, and
    /// reporting them was five findings and zero defects.
    #[test]
    fn declines_the_three_generics_the_corpus_audit_removed() {
        for generic in [
            "shared-initialize",
            "update-instance-for-different-class",
            "update-instance-for-redefined-class",
        ] {
            assert!(
                findings(&format!("(defmethod {generic} ((o widget) slots &key) o)")).is_empty(),
                "{generic}"
            );
        }
    }

    /// The other adjudication the corpus forced: PCL's own
    /// `initialize-instance` primary calls `shared-initialize` directly instead
    /// of chaining, which is what the system-supplied primary is *defined* to do
    /// and leaves every slot filled.
    #[test]
    fn accepts_a_primary_that_calls_shared_initialize_directly() {
        assert!(
            findings(
                "(defmethod initialize-instance ((o slot-object) &rest initargs)\n\
                 \x20 (apply #'shared-initialize o t initargs))"
            )
            .is_empty(),
            "this is SBCL src/pcl/init.lisp:54 verbatim"
        );
        assert!(
            findings(
                "(defmethod reinitialize-instance ((o slot-object) &rest initargs)\n\
                 \x20 (check-ri-initargs o initargs)\n\
                 \x20 (apply #'shared-initialize o nil initargs)\n  o)"
            )
            .is_empty(),
            "this is SBCL src/pcl/init.lisp:57 verbatim"
        );
    }

    #[test]
    fn folds_case_and_package_qualification_in_the_generics_name() {
        assert_eq!(
            findings("(defmethod CL:Initialize-Instance ((o widget) &key) o)").len(),
            1
        );
    }

    // -- the near misses -------------------------------------------------------

    #[test]
    fn accepts_a_primary_that_chains_anywhere_in_its_body() {
        assert!(
            findings(
                "(defmethod initialize-instance ((o widget) &key)\n  (let ((r (call-next-method))) r))"
            )
            .is_empty()
        );
    }

    #[test]
    fn accepts_a_primary_that_only_passes_the_function_along() {
        assert!(
            findings(
                "(defmethod initialize-instance ((o w) &rest a) (apply #'call-next-method a))"
            )
            .is_empty()
        );
    }

    /// `:after` is the idiom, and SBCL initializes the instance correctly with
    /// one. Reporting it would be a false positive on the single most common
    /// shape in the whole protocol.
    #[test]
    fn accepts_the_after_idiom() {
        assert!(
            findings("(defmethod initialize-instance :after ((o widget) &key) (setup o))")
                .is_empty()
        );
    }

    #[test]
    fn accepts_a_before_method() {
        assert!(
            findings("(defmethod initialize-instance :before ((o widget) &key) (check o))")
                .is_empty()
        );
    }

    /// The boundary with `around-method-missing-call-next-method` in
    /// `paredit-feature-lint-object-system`: that rule reports only `:around`,
    /// this one reports only unqualified, and the two sets are disjoint.
    #[test]
    fn accepts_an_around_method_which_belongs_to_the_object_system_package() {
        assert!(
            findings("(defmethod initialize-instance :around ((o widget) &key) (guard o))")
                .is_empty()
        );
    }

    #[test]
    fn accepts_a_primary_method_on_an_ordinary_generic() {
        assert!(findings("(defmethod draw ((o widget)) o)").is_empty());
    }

    #[test]
    fn accepts_a_make_instance_method_which_is_a_different_protocol() {
        assert!(findings("(defmethod make-instance ((c (eql 'widget)) &rest a) a)").is_empty());
    }

    /// A folded `#+`/`#-` atom in the **body** is the case the guard exists
    /// for, and the only one: `mentions` reads an atom's symbol text, and a
    /// reader conditional's text is `"#+sbcl (call-next-method)"`, which
    /// normalizes to no symbol at all. Without the guard this method — which
    /// chains under `#+sbcl` — is reported, and that is a false positive on
    /// working code.
    ///
    /// Mutation-tested: the qualifier case below is declined by the
    /// `is_primary` check whether the guard is there or not, so it killed
    /// nothing on its own.
    #[test]
    fn a_reader_conditional_makes_the_method_opaque() {
        assert!(
            findings("(defmethod initialize-instance ((o widget) &key) #+sbcl (call-next-method))")
                .is_empty(),
            "the body does chain, but the call is folded into an atom this cannot read"
        );
        assert!(
            findings("(defmethod initialize-instance #+sbcl :after ((o w) &key) (setup o))")
                .is_empty()
        );
    }

    #[test]
    fn a_setf_method_names_no_initialization_generic() {
        assert!(findings("(defmethod (setf width) (v (o widget)) v)").is_empty());
    }
}
