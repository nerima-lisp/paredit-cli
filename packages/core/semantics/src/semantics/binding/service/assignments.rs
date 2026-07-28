//! Recording which bindings a form reassigns.

use paredit_core_syntax::sexpr::ExpressionView;
use paredit_core_syntax::sexpr::reader::{atom_symbol_span, atom_symbol_text};

use super::super::policy::assignment_form;
use super::builder::Walk;
use super::scope_stack::Namespace;

impl Walk<'_> {
    /// Records an assignment for every place of `view` that is a bare variable
    /// resolving to a visible binding.
    ///
    /// A list place is deliberately skipped. `(setf (car x) 1)` mutates the
    /// object `x` points at; the binding `x` still holds what it always held,
    /// and recording it as reassigned would stop the value layer from
    /// propagating a value that never changed. `(setf x 1)` does rebind, and
    /// only the bare-atom shape can tell the two apart.
    pub(super) fn record_assignments(&mut self, view: &ExpressionView, head: &str) {
        // Asked of the walk's dialect, not assumed. Scheme's `set!` was
        // already in the policy table and unreachable: hardcoding Common Lisp
        // here meant a Scheme binding could never be recorded as reassigned,
        // and the value layer would happily propagate through a `set!`.
        let Some(form) = assignment_form(self.dialect, head) else {
            return;
        };

        for place in form.places().places_in(view) {
            // `atom_symbol_text` already refuses lists, which is the whole
            // bare-variable test.
            let (Some(text), Some(span)) = (atom_symbol_text(place), atom_symbol_span(place))
            else {
                continue;
            };

            if let Some(id) = self.stack.resolve(text, Namespace::Value) {
                self.builder.draft_mut(id).push_assignment(span);
            }
        }
    }
}
