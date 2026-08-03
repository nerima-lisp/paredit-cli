//! Turning a symbol occurrence into a resolved reference.

use crate::semantics::NodeKey;
use paredit_core_syntax::common_lisp::{
    common_lisp_operator_head_eq, normalize_common_lisp_operator_head,
};
use paredit_core_syntax::sexpr::reader::{atom_symbol_span, atom_symbol_text};
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, ReaderPrefix};

use super::super::model::{OpacityCause, OpacityCauseKind, ScopeId};
use super::builder::{Walk, head_text, is_reader_dispatch};
use super::scope_stack::Namespace;

impl Walk<'_> {
    /// Resolves a symbol occurrence in value position.
    pub(super) fn atom(&mut self, view: &ExpressionView, quasiquote_depth: usize) {
        self.atom_in(view, quasiquote_depth, Namespace::Value);
    }

    /// Resolves one symbol occurrence against the scope stack.
    ///
    /// `namespace` is decided by position, which is the whole of Common Lisp's
    /// two-namespace rule: the head of a call names a function, everything
    /// else names a variable. Without it `(flet ((x ...)) x)` would resolve to
    /// the local function, and the value layer would propagate a function
    /// definition into a variable reference.
    pub(super) fn atom_in(
        &mut self,
        view: &ExpressionView,
        quasiquote_depth: usize,
        namespace: Namespace,
    ) {
        if quasiquote_depth > 0 {
            return;
        }

        // `#'x` designates the *function* namespace. The prefix overrides the
        // position rather than cancelling the occurrence: `#'g` inside
        // `(flet ((g () …)) …)` is a reference to that local `g`, and dropping
        // it made a called function read as unused to every consumer of this
        // table.
        //
        // Overriding rather than deleting the guard is what keeps the fix
        // narrow. A plain `Namespace::Value` lookup would resolve
        // `(let ((g 1)) #'g)` to the *variable* `g`, which is what the guard
        // existed to prevent — `#'` on a variable names nothing in Common
        // Lisp. `Namespace::Function` admits only `Function`/`Macro` bindings,
        // so that lookup still misses, while `#'car` still resolves to nothing
        // unless the file really did `flet` a local `car`, in which case it is
        // a reference to it.
        //
        // Only Common Lisp and Emacs Lisp reach here with this prefix set:
        // `#'` is a function designator in both, and in Scheme and Racket the
        // reader rejects `#'` outright rather than producing the prefix.
        let namespace = if view.reader_prefixes.contains(&ReaderPrefix::Function) {
            Namespace::Function
        } else {
            namespace
        };

        if is_reader_dispatch(view) {
            self.mark_opaque(OpacityCause::new(
                OpacityCauseKind::ReaderDispatch,
                view.span,
            ));
            return;
        }

        let (Some(text), Some(span)) = (atom_symbol_text(view), atom_symbol_span(view)) else {
            return;
        };

        // The atom that spells a binding is its definition, not a use of it.
        if self.definitions.contains(&span) {
            return;
        }

        // No fallback across namespaces: a miss means the name is free in that
        // namespace, and the table records absence rather than a guess.
        let Some(id) = self.stack.resolve(text, namespace) else {
            return;
        };

        self.builder.resolve_to(NodeKey::atom(span), id);
        self.builder.draft_mut(id).push_reference(span);
    }

    /// Handles the explicit spellings of the reader macros, mirroring
    /// `collect_explicit_reader_form`. Returns whether the form was consumed.
    pub(super) fn reader_form(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        quasiquote_depth: usize,
    ) -> bool {
        if view.kind != ExpressionKind::List || view.children.len() < 2 {
            return false;
        }

        let Some(head) = head_text(&view.children[0]) else {
            return false;
        };
        let normalized_head = normalize_common_lisp_operator_head(head);

        if common_lisp_operator_head_eq(normalized_head, "quote") {
            // The explicit spelling of `'…`, and inert for the same reason:
            // `(quote (setq x 2))` evaluates to a list. The walk stops because
            // there are no references inside, not because the scope became
            // unreadable.
            return true;
        }

        // `(function name)` is the explicit `#'name`. `(function (args) ret)`
        // is the unrelated FUNCTION *type specifier*, which holds ordinary
        // atoms that must still be scanned — the list second element is what
        // tells them apart.
        if common_lisp_operator_head_eq(normalized_head, "function")
            && view.children.len() == 2
            && view.children[1].kind == ExpressionKind::Atom
        {
            // The designated name is still a reference, in the function
            // namespace, exactly as for the `#'` spelling. The head `function`
            // is syntax and is consumed with the form.
            self.atom_in(&view.children[1], quasiquote_depth, Namespace::Function);
            return true;
        }

        match normalized_head.to_ascii_lowercase().as_str() {
            "quasiquote" => {
                for child in &view.children[1..] {
                    self.form(child, scope, quasiquote_depth + 1);
                }
                true
            }
            "unquote" | "unquote-splicing" if quasiquote_depth > 0 => {
                for child in &view.children[1..] {
                    self.form(child, scope, quasiquote_depth - 1);
                }
                true
            }
            _ => false,
        }
    }
}
