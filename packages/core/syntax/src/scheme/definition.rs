//! Reading what a Scheme `define` actually defines.
//!
//! `define` is overloaded three ways, and the difference is in the *shape* of
//! child 1 rather than in the head:
//!
//! ```scheme
//! (define answer 42)              ; a variable
//! (define (answer x) x)           ; a procedure
//! (define ((adder n) x) (+ n x))  ; a procedure returning a procedure
//! ```
//!
//! The curried form is standard in MIT Scheme and Racket and appears in Guile
//! via `define-curried`. It nests: `(define ((f a) b) body)` is shorthand for
//! `(define (f a) (lambda (b) body))`, so the name sits at the bottom of the
//! leftmost spine and each level contributes one formals list.

use crate::sexpr::{Delimiter, ExpressionKind, ExpressionView};

use super::symbol::is_scheme_identifier;

/// What a `define` form binds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemeDefineTarget<'a> {
    /// `(define name value)`.
    Variable {
        /// The atom naming the variable.
        name: &'a ExpressionView,
    },
    /// `(define (name . formals) body ...)` and its curried nestings.
    Procedure {
        /// The atom naming the procedure.
        name: &'a ExpressionView,
        /// One formals list per curry level, in *application* order.
        ///
        /// In every entry the parameters are `children[1..]`: child 0 is the
        /// name for the first entry and the next formals list for the rest.
        /// [`Self::parameters`] does that slicing.
        ///
        /// A plain `(define (f x) ...)` yields exactly one entry. Application
        /// order is the reverse of syntactic nesting -- in
        /// `(define ((adder n) x) ...)` the inner `(adder n)` supplies the
        /// argument consumed first -- and the names in every entry are in
        /// scope throughout the body.
        formals: Vec<&'a ExpressionView>,
    },
}

impl<'a> SchemeDefineTarget<'a> {
    /// The atom naming whatever is being defined.
    #[must_use]
    pub const fn name(&self) -> &'a ExpressionView {
        match self {
            Self::Variable { name } | Self::Procedure { name, .. } => name,
        }
    }

    /// Every parameter node the definition binds, across all curry levels.
    ///
    /// Each formals entry contributes `children[1..]`, skipping the child 0
    /// that holds the name or the next-inner formals list.
    #[must_use]
    pub fn parameters(&self) -> Vec<&'a ExpressionView> {
        match self {
            Self::Variable { .. } => Vec::new(),
            Self::Procedure { formals, .. } => formals
                .iter()
                .flat_map(|level| level.children.iter().skip(1))
                .collect(),
        }
    }

    /// How many nested formals lists the definition carries.
    ///
    /// `0` for a variable, `1` for an ordinary procedure, more for a curried
    /// one.
    #[must_use]
    pub fn curry_depth(&self) -> usize {
        match self {
            Self::Variable { .. } => 0,
            Self::Procedure { formals, .. } => formals.len(),
        }
    }
}

/// Resolves the target of a `(define ...)` form given its child 1.
///
/// Returns `None` for a shape this layer cannot read, which is the same answer
/// it gives for a form that is not a definition at all: in both cases nothing
/// about the binding has been proved.
#[must_use]
pub fn scheme_define_target(target: &ExpressionView) -> Option<SchemeDefineTarget<'_>> {
    if target.kind == ExpressionKind::Atom {
        // `(define "x" 1)` is not a definition of anything. Only an identifier
        // can be bound, and `"x"`, `42` and `#t` all reach here as atoms.
        return is_scheme_identifier(target)
            .then_some(SchemeDefineTarget::Variable { name: target });
    }

    if !is_formals_list(target) {
        return None;
    }

    // Walk down the leftmost spine, collecting one formals list per level.
    // `((f a) b)` yields `[(f a), b]` on the way down and bottoms out at `f`.
    let mut formals = vec![target];
    let mut head = target.children.first()?;
    while head.kind == ExpressionKind::List {
        if !is_formals_list(head) {
            return None;
        }
        formals.push(head);
        head = head.children.first()?;
    }

    if !is_scheme_identifier(head) {
        return None;
    }

    // Collected innermost-last while descending; the caller wants the
    // outermost formals list first, matching argument application order.
    formals.reverse();
    Some(SchemeDefineTarget::Procedure {
        name: head,
        formals,
    })
}

fn is_formals_list(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && matches!(view.delimiter, Some(Delimiter::Paren | Delimiter::Bracket))
        && view.reader_prefixes.is_empty()
        && !view.children.is_empty()
}
