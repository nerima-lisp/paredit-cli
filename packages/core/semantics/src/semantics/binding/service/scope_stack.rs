//! The bindings visible at the current point of the walk.

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;

use super::super::model::{BindingId, BindingKind};

/// One binding visible at the current point of the walk.
#[derive(Debug, Clone)]
struct VisibleBinding {
    id: BindingId,
    /// The atom's own spelling. Kept verbatim rather than normalized because
    /// every comparison goes through [`common_lisp_symbol_reference_eq`],
    /// which already folds case and package qualification.
    name: String,
    kind: BindingKind,
}

/// Which namespace a symbol occurrence is read in.
///
/// Common Lisp keeps variables and functions apart, so `(flet ((x ...)) x)`
/// reads a *variable* `x`. Symbol macros and `with-slots` slots expand in
/// value position, so they share the value namespace with variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Namespace {
    Value,
    Function,
}

impl Namespace {
    fn admits(self, dialect: Dialect, kind: BindingKind) -> bool {
        // Scheme is a Lisp-1: `(let ((f car)) (f x))` calls the *variable* `f`,
        // and `(letrec ((f (lambda ...))) f)` reads the same binding it calls.
        // Splitting the namespaces there would leave every local procedure's
        // call sites unresolved.
        if matches!(dialect, Dialect::Scheme | Dialect::Racket) {
            return true;
        }

        match self {
            Self::Value => matches!(
                kind,
                BindingKind::Variable | BindingKind::SymbolMacro | BindingKind::Slot
            ),
            Self::Function => matches!(kind, BindingKind::Function | BindingKind::Macro),
        }
    }
}

/// The scope stack: every binding visible right now, outermost first.
///
/// A stack rather than a map from name to binding because shadowing is
/// resolved by *position*, not by replacement — the shadowed binding must
/// still be reachable once the walk leaves the inner scope. Leaving a scope is
/// a [`ScopeStack::rewind`] to the depth recorded on entry, which is why every
/// binder saves the depth before it declares anything.
#[derive(Debug)]
pub(super) struct ScopeStack {
    visible: Vec<VisibleBinding>,
    /// Decides both name equality and whether the namespaces are separate.
    /// Common Lisp folds case and package qualification and keeps variables
    /// apart from functions; Scheme does neither.
    dialect: Dialect,
}

impl ScopeStack {
    pub(super) const fn new(dialect: Dialect) -> Self {
        Self {
            visible: Vec::new(),
            dialect,
        }
    }

    pub(super) fn depth(&self) -> usize {
        self.visible.len()
    }

    pub(super) fn rewind(&mut self, depth: usize) {
        self.visible.truncate(depth);
    }

    pub(super) fn push(&mut self, id: BindingId, name: &str, kind: BindingKind) {
        self.visible.push(VisibleBinding {
            id,
            name: name.to_owned(),
            kind,
        });
    }

    /// The innermost visible binding of `name` in `namespace`.
    ///
    /// Searched from the top of the stack so the innermost binding wins, which
    /// is what makes `(let ((x 1)) (let ((x 2)) x))` resolve to the inner `x`.
    pub(super) fn resolve(&self, name: &str, namespace: Namespace) -> Option<BindingId> {
        self.visible
            .iter()
            .rev()
            .find(|binding| {
                namespace.admits(self.dialect, binding.kind)
                    && self.names_equal(&binding.name, name)
            })
            .map(|binding| binding.id)
    }

    /// Name equality in the dialect's own terms.
    ///
    /// Common Lisp folds case and package qualification, so `X`, `x` and
    /// `cl-user:x` are one symbol. R7RS 2.1 makes Scheme case-sensitive, where
    /// folding would resolve `Loop` to a binding named `loop`.
    fn names_equal(&self, candidate: &str, name: &str) -> bool {
        match self.dialect {
            Dialect::Scheme | Dialect::Racket => candidate == name,
            _ => common_lisp_symbol_reference_eq(candidate, name),
        }
    }

    pub(super) fn visible_ids(&self) -> Vec<BindingId> {
        self.visible.iter().map(|binding| binding.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack() -> ScopeStack {
        let mut stack = ScopeStack::new(Dialect::CommonLisp);
        stack.push(BindingId::new(0), "x", BindingKind::Variable);
        stack.push(BindingId::new(1), "f", BindingKind::Function);
        stack.push(BindingId::new(2), "x", BindingKind::Variable);
        stack
    }

    #[test]
    fn the_innermost_binding_of_a_name_wins() {
        assert_eq!(
            stack().resolve("x", Namespace::Value),
            Some(BindingId::new(2))
        );
    }

    #[test]
    fn rewinding_uncovers_the_shadowed_binding_again() {
        let mut stack = stack();
        stack.rewind(2);
        assert_eq!(
            stack.resolve("x", Namespace::Value),
            Some(BindingId::new(0))
        );
    }

    #[test]
    fn the_namespaces_do_not_see_each_other() {
        let stack = stack();
        assert_eq!(stack.resolve("f", Namespace::Value), None);
        assert_eq!(stack.resolve("x", Namespace::Function), None);
        assert_eq!(
            stack.resolve("f", Namespace::Function),
            Some(BindingId::new(1))
        );
    }

    #[test]
    fn scheme_resolves_one_namespace_and_compares_names_exactly() {
        // A Lisp-1 with case-sensitive identifiers: a local procedure is
        // reachable from value position, and `X` is not `x`.
        let mut stack = ScopeStack::new(Dialect::Scheme);
        stack.push(BindingId::new(0), "f", BindingKind::Function);
        stack.push(BindingId::new(1), "x", BindingKind::Variable);

        assert_eq!(stack.resolve("f", Namespace::Value), Some(BindingId::new(0)));
        assert_eq!(
            stack.resolve("x", Namespace::Function),
            Some(BindingId::new(1))
        );
        assert_eq!(stack.resolve("X", Namespace::Value), None);
    }

    #[test]
    fn lookup_folds_case_and_package_qualification() {
        let stack = stack();
        assert_eq!(
            stack.resolve("X", Namespace::Value),
            Some(BindingId::new(2))
        );
        assert_eq!(
            stack.resolve("cl-user:x", Namespace::Value),
            Some(BindingId::new(2))
        );
    }
}
