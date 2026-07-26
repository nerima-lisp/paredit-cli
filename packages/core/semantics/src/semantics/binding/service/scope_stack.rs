//! The bindings visible at the current point of the walk.

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;

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
    const fn admits(self, kind: BindingKind) -> bool {
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
#[derive(Debug, Default)]
pub(super) struct ScopeStack {
    visible: Vec<VisibleBinding>,
}

impl ScopeStack {
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
                namespace.admits(binding.kind)
                    && common_lisp_symbol_reference_eq(&binding.name, name)
            })
            .map(|binding| binding.id)
    }

    pub(super) fn visible_ids(&self) -> Vec<BindingId> {
        self.visible.iter().map(|binding| binding.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack() -> ScopeStack {
        let mut stack = ScopeStack::default();
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
