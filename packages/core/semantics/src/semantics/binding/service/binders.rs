//! Which forms bind what, and in which scope each subform is evaluated.
//!
//! Every function here mirrors its counterpart in
//! `lexical_scope::traversal::binding_forms`. The dispatch order, the early
//! returns, and above all *where each subform is walked* are taken from there
//! rather than re-derived: a parallel `let` evaluates its values outside the
//! scope it opens, a `let*` inside the progressively extended one, and getting
//! that backwards is exactly the kind of silent divergence the differential
//! test exists to catch.

use crate::lexical_scope::{BoundName, binding_pattern_bound_names};
use paredit_core_syntax::common_lisp::{CommonLispLocalCallableForm, CommonLispOperator};
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_span;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView};

use super::super::model::{BindingKind, OpacityCause, OpacityCauseKind, ScopeId};
use super::builder::{Walk, head_text};
use super::lambda_lists::lambda_list_names;

/// One entry of a `((name value) ...)` binding list.
struct BindingGroup<'a> {
    names: Vec<BoundName>,
    value: Option<&'a ExpressionView>,
}

impl Walk<'_> {
    /// Dispatches a list to its binding-form handler, mirroring
    /// `collect_shadow_aware_special_form`. Returns whether it was consumed.
    pub(super) fn binder(&mut self, view: &ExpressionView, scope: ScopeId) -> bool {
        if view.kind != ExpressionKind::List || view.children.len() < 2 {
            return false;
        }

        if self.dialect == Dialect::EmacsLisp {
            return self.emacs_lisp_binder(view, scope);
        }

        let Some(head) = head_text(&view.children[0]) else {
            return false;
        };

        // Scheme resolves against its own table. Routing it through the
        // Common Lisp one would recognise `let`, `let*` and `lambda` and
        // nothing else Scheme binds with.
        if matches!(self.dialect, Dialect::Scheme | Dialect::Racket) {
            return self.scheme_binder(view, scope, head);
        }

        let Some(operator) = CommonLispOperator::from_head(head) else {
            return false;
        };

        match operator {
            operator if operator.is_parallel_let_binding() => {
                let kind = if operator == CommonLispOperator::SymbolMacrolet {
                    BindingKind::SymbolMacro
                } else {
                    BindingKind::Variable
                };
                self.let_binding(view, scope, head, kind, false);
                true
            }
            operator if operator.is_sequential_let_binding() => {
                self.let_binding(view, scope, head, BindingKind::Variable, true);
                true
            }
            operator if operator.is_value_binding() => {
                self.value_binding(view, scope, head);
                true
            }
            operator if operator.is_clause_binding() => {
                self.clause_binding(view, scope, head);
                true
            }
            operator if operator.is_handler_bind_binding() => {
                self.handler_bind(view, scope, operator.includes_restart_bind_options());
                true
            }
            operator if operator.is_iteration_binding() => {
                self.iteration_binding(view, scope, head);
                true
            }
            operator if operator.is_do_binding() || operator.is_prog_binding() => {
                self.do_like(view, scope, head, operator.is_sequential_variable_binding());
                true
            }
            operator if operator.is_slot_binding() => {
                self.slot_binding(view, scope, head);
                true
            }
            operator if operator.is_local_callable_binding() => {
                let Some(form) = operator.local_callable_form() else {
                    return false;
                };
                self.local_callable(view, scope, head, form);
                true
            }
            // `(locally (declare ...) body)`: no bindings, and the
            // declaration at index 1 is skipped rather than scanned.
            CommonLispOperator::Locally => {
                self.body(&view.children[2..], scope);
                true
            }
            operator if operator.is_lambda_like() => match view.children.get(1) {
                Some(parameter_form) => {
                    self.lambda_scope(parameter_form, &view.children[2..], scope, view, head)
                }
                None => false,
            },
            operator if operator.is_defun_like() => self.definition(view, scope, head, operator),
            _ => false,
        }
    }

    /// `let`, `let*`, and `symbol-macrolet`.
    ///
    /// The single distinction is `sequential`: `let*` makes each name visible
    /// to the *next* initial-value form, `let` makes none of them visible
    /// until the body. Everything else is shared, which is why they are one
    /// function.
    fn let_binding(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        kind: BindingKind,
        sequential: bool,
    ) {
        let Some(binding_form) = view.children.get(1) else {
            return;
        };
        let Some(groups) = binding_groups(binding_form) else {
            // A shape the shared binding-list parser rejects. It collects no
            // references there, so the table must record nothing either.
            self.mark_opaque(OpacityCause::new(
                OpacityCauseKind::UnreadableBinderList,
                binding_form.span,
            ));
            return;
        };

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        if sequential {
            for group in &groups {
                if let Some(value) = group.value {
                    self.form(value, inner, 0);
                }
                self.declare_group(view, inner, head, group, kind);
            }
        } else {
            for group in &groups {
                if let Some(value) = group.value {
                    // The outer scope: a parallel `let`'s values cannot see
                    // any of the names it is about to bind.
                    self.form(value, scope, 0);
                }
            }
            for group in &groups {
                self.declare_group(view, inner, head, group, kind);
            }
        }

        self.body(&view.children[2..], inner);
        self.stack.rewind(depth);
    }

    /// `destructuring-bind` and `multiple-value-bind`.
    fn value_binding(&mut self, view: &ExpressionView, scope: ScopeId, head: &str) {
        let Some(binding_form) = view.children.get(1) else {
            return;
        };
        let Some(value_form) = view.children.get(2) else {
            return;
        };

        self.form(value_form, scope, 0);

        if binding_form.kind != ExpressionKind::List {
            return;
        }

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);
        for bound in lambda_list_names(binding_form) {
            self.declare(view, inner, head, &bound, BindingKind::Variable, None);
        }
        self.body(&view.children[3..], inner);
        self.stack.rewind(depth);
    }

    /// `handler-case` and `restart-case`: each clause binds its own condition
    /// variable, and the protected form sees none of them.
    fn clause_binding(&mut self, view: &ExpressionView, scope: ScopeId, head: &str) {
        let Some(protected_form) = view.children.get(1) else {
            return;
        };
        self.form(protected_form, scope, 0);

        for index in 2..view.children.len() {
            let clause = &view.children[index];
            if clause.kind != ExpressionKind::List {
                self.form(clause, scope, 0);
                continue;
            }
            let Some(parameter_form) = clause.children.get(1) else {
                continue;
            };
            if parameter_form.kind != ExpressionKind::List {
                self.body(&clause.children[2..], scope);
                continue;
            }

            let depth = self.stack.depth();
            let inner = self.builder.open_scope(scope, clause.span);
            for bound in lambda_list_names(parameter_form) {
                self.declare(clause, inner, head, &bound, BindingKind::Variable, None);
            }
            self.body(&clause.children[2..], inner);
            self.stack.rewind(depth);
        }
    }

    /// `handler-bind` and `restart-bind` bind nothing lexically: the handler
    /// functions are ordinary values evaluated in the enclosing scope.
    fn handler_bind(&mut self, view: &ExpressionView, scope: ScopeId, restart_options: bool) {
        let Some(binding_form) = view.children.get(1) else {
            return;
        };

        for spec in &binding_form.children {
            if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
                continue;
            }
            if let Some(function_form) = spec.children.get(1) {
                self.form(function_form, scope, 0);
            }
            if restart_options {
                // `(restart-bind ((name fn :report-function r)) ...)`: the
                // option values are evaluated, the keywords are not.
                let mut index = 2;
                while index + 1 < spec.children.len() {
                    self.form(&spec.children[index + 1], scope, 0);
                    index += 2;
                }
            }
        }

        self.body(&view.children[2..], scope);
    }

    /// `dolist` and `dotimes`: `(dolist (var source result) body...)`.
    fn iteration_binding(&mut self, view: &ExpressionView, scope: ScopeId, head: &str) {
        let Some(binding_form) = view.children.get(1) else {
            return;
        };

        if let Some(source_form) = binding_form.children.get(1) {
            self.form(source_form, scope, 0);
        }

        let Some(bound) = binding_form.children.first().and_then(bare_name) else {
            return;
        };

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);
        self.declare(view, inner, head, &bound, BindingKind::Variable, None);

        // The result form is evaluated with the loop variable still bound.
        if let Some(result_form) = binding_form.children.get(2) {
            self.form(result_form, inner, 0);
        }
        self.body(&view.children[2..], inner);
        self.stack.rewind(depth);
    }

    /// `do`, `do*`, `prog`, and `prog*`.
    fn do_like(&mut self, view: &ExpressionView, scope: ScopeId, head: &str, sequential: bool) {
        let Some(binding_form) = view.children.get(1) else {
            return;
        };

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        if sequential {
            for spec in &binding_form.children {
                if let Some(init_form) = variable_spec_child(spec, 1) {
                    self.form(init_form, inner, 0);
                }
                self.declare_variable_spec(view, inner, head, spec);
            }
        } else {
            for spec in &binding_form.children {
                if let Some(init_form) = variable_spec_child(spec, 1) {
                    self.form(init_form, scope, 0);
                }
            }
            for spec in &binding_form.children {
                self.declare_variable_spec(view, inner, head, spec);
            }
        }

        // Step forms are evaluated once per iteration with every variable
        // bound, so they belong to the inner scope for both `do` and `do*`.
        for spec in &binding_form.children {
            if let Some(step_form) = variable_spec_child(spec, 2) {
                self.form(step_form, inner, 0);
            }
        }

        self.body(&view.children[2..], inner);
        self.stack.rewind(depth);
    }

    /// `with-slots` and `with-accessors`.
    fn slot_binding(&mut self, view: &ExpressionView, scope: ScopeId, head: &str) {
        let Some(slot_specs) = view.children.get(1) else {
            return;
        };
        let Some(instance_form) = view.children.get(2) else {
            return;
        };

        self.form(instance_form, scope, 0);

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);
        for spec in &slot_specs.children {
            let Some(bound) = slot_spec_name(spec) else {
                continue;
            };
            self.declare(view, inner, head, &bound, BindingKind::Slot, None);
        }
        self.body(&view.children[3..], inner);
        self.stack.rewind(depth);
    }

    /// `flet`, `labels`, `macrolet`, and `compiler-macrolet`.
    fn local_callable(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        form: CommonLispLocalCallableForm,
    ) {
        let Some(binding_form) = view.children.get(1) else {
            return;
        };

        let kind = match form {
            CommonLispLocalCallableForm::Flet | CommonLispLocalCallableForm::Labels => {
                BindingKind::Function
            }
            _ => BindingKind::Macro,
        };
        // `labels` (and the macro forms) make the whole group visible inside
        // each definition, so a local function can recurse and call its
        // siblings. `flet` does not: its definitions are closed over the
        // *enclosing* scope, which is the one difference between them.
        let group_is_self_visible = !matches!(form, CommonLispLocalCallableForm::Flet);

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        if group_is_self_visible {
            self.declare_callables(view, inner, head, binding_form, kind);
            self.walk_callable_specs(binding_form, inner, head);
        } else {
            self.walk_callable_specs(binding_form, inner, head);
            self.declare_callables(view, inner, head, binding_form, kind);
        }

        self.body(&view.children[2..], inner);
        self.stack.rewind(depth);
    }

    fn declare_callables(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        binding_form: &ExpressionView,
        kind: BindingKind,
    ) {
        for spec in &binding_form.children {
            if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
                continue;
            }
            let Some(bound) = spec.children.first().and_then(bare_name) else {
                continue;
            };
            self.declare(view, scope, head, &bound, kind, None);
        }
    }

    fn walk_callable_specs(&mut self, binding_form: &ExpressionView, scope: ScopeId, head: &str) {
        for spec in &binding_form.children {
            if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
                continue;
            }
            let Some(parameter_form) = spec.children.get(1) else {
                continue;
            };
            self.lambda_scope(parameter_form, &spec.children[2..], scope, spec, head);
        }
    }

    /// `defun` and friends. The definition's own name is deliberately not
    /// registered: it names a *global*, and this table is the lexical binding
    /// context of one file.
    fn definition(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        operator: CommonLispOperator,
    ) -> bool {
        let Some(shape) = definition_shape(self.dialect, view, head) else {
            return false;
        };

        // A setf expander's and a compiler macro's body is code the compiler
        // runs, not code this file evaluates; `lexical_scope` skips both and
        // so must this, or the two would disagree about the whole body.
        let body_forms: &[ExpressionView] = if matches!(
            operator,
            CommonLispOperator::DefineSetfExpander | CommonLispOperator::DefineCompilerMacro
        ) {
            &[]
        } else {
            shape.body_forms(view)
        };

        match shape.lambda_list(view) {
            Some(parameter_form) => {
                self.lambda_scope(parameter_form, body_forms, scope, view, head);
            }
            None => self.body(body_forms, scope),
        }
        true
    }

    fn declare_group(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        group: &BindingGroup<'_>,
        kind: BindingKind,
    ) {
        let init_form = group.value.map(|value| value.span);
        for bound in &group.names {
            self.declare(view, scope, head, bound, kind, init_form);
        }
    }

    fn declare_variable_spec(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        spec: &ExpressionView,
    ) {
        let Some(bound) = variable_spec_name(spec) else {
            return;
        };
        let init_form = variable_spec_child(spec, 1).map(|init| init.span);
        self.declare(view, scope, head, &bound, BindingKind::Variable, init_form);
    }
}

/// Splits a `((name value) ...)` binding list, mirroring
/// `lexical_scope::bindings::generic_binding_groups` for Common Lisp's
/// paren shape.
///
/// `None` means a shape that parser rejects. The reference query then collects
/// nothing anywhere in the form, so the table must record nothing either.
fn binding_groups(binding_form: &ExpressionView) -> Option<Vec<BindingGroup<'_>>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        return None;
    }

    binding_form
        .children
        .iter()
        .map(|pair| {
            if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren) {
                // `(let (rows) ...)` is CLHS shorthand for `((rows nil))`.
                if pair.kind != ExpressionKind::Atom {
                    return None;
                }
                let names = binding_pattern_bound_names(pair);
                return (names.len() == 1).then_some(BindingGroup { names, value: None });
            }
            if pair.children.is_empty() || pair.children.len() > 2 {
                return None;
            }
            let names = binding_pattern_bound_names(&pair.children[0]);
            (!names.is_empty()).then(|| BindingGroup {
                names,
                value: pair.children.get(1),
            })
        })
        .collect()
}

/// The name of a `do`/`prog` variable spec: `x` or `(x init step)`.
fn variable_spec_name(spec: &ExpressionView) -> Option<BoundName> {
    bare_name(spec).or_else(|| spec.children.first().and_then(bare_name))
}

fn variable_spec_child(spec: &ExpressionView, index: usize) -> Option<&ExpressionView> {
    (spec.kind == ExpressionKind::List)
        .then(|| spec.children.get(index))
        .flatten()
}

/// The name of a `with-slots` spec: `slot` or `(alias slot)`.
fn slot_spec_name(spec: &ExpressionView) -> Option<BoundName> {
    bare_name(spec).or_else(|| spec.children.first().and_then(bare_name))
}

/// A plain symbol atom read as a bound name.
fn bare_name(view: &ExpressionView) -> Option<BoundName> {
    Some(BoundName {
        name: head_text(view)?.to_owned(),
        span: atom_symbol_span(view)?,
    })
}
