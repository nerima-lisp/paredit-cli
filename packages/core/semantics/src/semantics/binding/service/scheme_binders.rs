//! Which Scheme forms bind what, and in which scope each subform is evaluated.
//!
//! Every function here mirrors its counterpart in
//! `lexical_scope::traversal::binding_forms::scheme`, for the same reason the
//! Common Lisp binders mirror theirs: the table is the reference query turned
//! inside out, and a differential test pins the two together. Where each
//! subform is *walked* is taken from there rather than re-derived -- a
//! parallel `let` evaluates its initializers outside the scope it opens, a
//! `letrec` inside it, and getting that backwards is exactly the silent
//! divergence the differential test exists to catch.

use paredit_core_syntax::scheme::{
    SchemeBindingForm, SchemeDefineTarget, SchemeDefinitionForm, SchemeLetKind, SchemeOperator,
    scheme_define_target, scheme_formal_defaults_in, scheme_formals_in, scheme_identifier_text,
};
use paredit_core_syntax::sexpr::reader::atom_symbol_span;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView};

use crate::lexical_scope::BoundName;

use super::super::model::{BindingKind, OpacityCause, OpacityCauseKind, ScopeId};
use super::builder::Walk;

impl Walk<'_> {
    /// Dispatches a Scheme list to its binding-form handler.
    ///
    /// Returns whether the form was consumed. `false` sends it to the generic
    /// call walk, which is right for `begin`, `if`, `cond` and every ordinary
    /// procedure call.
    pub(super) fn scheme_binder(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
    ) -> bool {
        let Some(operator) = SchemeOperator::from_head(head) else {
            return false;
        };

        if let Some(binding) = operator.binding_form() {
            return self.scheme_binding_form(view, scope, head, binding);
        }

        if let Some(definition) = operator.definition_form() {
            return self.scheme_definition_form(view, scope, head, definition);
        }

        false
    }

    fn scheme_binding_form(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        binding: SchemeBindingForm,
    ) -> bool {
        match binding {
            SchemeBindingForm::Let { kind, namespace } => {
                let kind = match namespace {
                    paredit_core_syntax::scheme::SchemeBindingNamespace::Value => {
                        (kind, BindingKind::Variable)
                    }
                    paredit_core_syntax::scheme::SchemeBindingNamespace::Syntax => {
                        (kind, BindingKind::Macro)
                    }
                };
                if is_named_let(view) {
                    self.scheme_named_let(view, scope, head, kind.0, kind.1);
                } else {
                    self.scheme_let(view, scope, head, kind.0, kind.1);
                }
                true
            }
            SchemeBindingForm::NamedLet => {
                self.scheme_named_let(
                    view,
                    scope,
                    head,
                    SchemeLetKind::Parallel,
                    BindingKind::Variable,
                );
                true
            }
            SchemeBindingForm::LetValues(kind) => {
                self.scheme_let_values(view, scope, head, kind);
                true
            }
            SchemeBindingForm::Do => {
                self.scheme_do(view, scope, head);
                true
            }
            SchemeBindingForm::Lambda => self.scheme_lambda(view, scope, head, 1),
            SchemeBindingForm::CaseLambda => {
                self.scheme_case_lambda(view, scope, head);
                true
            }
            SchemeBindingForm::Guard => {
                self.scheme_guard(view, scope, head);
                true
            }
            SchemeBindingForm::DynamicBinding => {
                self.scheme_dynamic_binding(view, scope);
                true
            }
        }
    }

    /// `let`, `let*`, `letrec`, `letrec*`, `let-syntax`, `letrec-syntax`.
    fn scheme_let(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        let_kind: SchemeLetKind,
        binding_kind: BindingKind,
    ) {
        let Some(binding_form) = view.children.get(1) else {
            return;
        };
        let Some(groups) = scheme_binding_groups(binding_form) else {
            // A shape the shared binding-list reader rejects. It collects no
            // references there, so the table must record nothing either.
            self.mark_opaque(OpacityCause::new(
                OpacityCauseKind::UnreadableBinderList,
                binding_form.span,
            ));
            return;
        };

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        self.scheme_declare_groups(view, scope, inner, head, &groups, let_kind, binding_kind);
        self.body(view.children.get(2..).unwrap_or_default(), inner);
        self.stack.rewind(depth);
    }

    /// The shared initializer-and-declaration order of every `let` flavour.
    #[allow(clippy::too_many_arguments)]
    fn scheme_declare_groups(
        &mut self,
        view: &ExpressionView,
        outer: ScopeId,
        inner: ScopeId,
        head: &str,
        groups: &[SchemeBindingGroup<'_>],
        let_kind: SchemeLetKind,
        binding_kind: BindingKind,
    ) {
        match let_kind {
            // A parallel `let`'s initializers cannot see any name it is about
            // to bind, so they are walked in the *outer* scope.
            SchemeLetKind::Parallel => {
                for group in groups {
                    if let Some(value) = group.value {
                        self.form(value, outer, 0);
                    }
                }
                for group in groups {
                    self.declare_scheme_group(view, inner, head, group, binding_kind);
                }
            }
            SchemeLetKind::Sequential => {
                for group in groups {
                    if let Some(value) = group.value {
                        self.form(value, inner, 0);
                    }
                    self.declare_scheme_group(view, inner, head, group, binding_kind);
                }
            }
            // `letrec` binds the whole group first: every initializer sees
            // every name, which is what lets mutually recursive procedures
            // refer to each other.
            SchemeLetKind::Recursive => {
                for group in groups {
                    self.declare_scheme_group(view, inner, head, group, binding_kind);
                }
                for group in groups {
                    if let Some(value) = group.value {
                        self.form(value, inner, 0);
                    }
                }
            }
        }
    }

    /// `(let name ((var init) ...) body ...)`.
    ///
    /// The loop name is bound over the body only; the initializers are
    /// evaluated in the enclosing scope exactly as a plain `let`'s are.
    fn scheme_named_let(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        let_kind: SchemeLetKind,
        binding_kind: BindingKind,
    ) {
        let Some(loop_name) = view.children.get(1).and_then(bare_scheme_name) else {
            return;
        };
        let Some(binding_form) = view.children.get(2) else {
            return;
        };
        let Some(groups) = scheme_binding_groups(binding_form) else {
            self.mark_opaque(OpacityCause::new(
                OpacityCauseKind::UnreadableBinderList,
                binding_form.span,
            ));
            return;
        };

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        self.scheme_declare_groups(view, scope, inner, head, &groups, let_kind, binding_kind);
        // Declared after the initializers so it cannot capture them, and
        // before the body so a recursive call resolves to it.
        self.declare(view, inner, head, &loop_name, BindingKind::Function, None);
        self.body(view.children.get(3..).unwrap_or_default(), inner);
        self.stack.rewind(depth);
    }

    /// `(let-values (((a b) producer) ...) body ...)`.
    fn scheme_let_values(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        let_kind: SchemeLetKind,
    ) {
        let Some(binding_form) = view.children.get(1) else {
            return;
        };
        if !is_binding_container(binding_form) {
            self.mark_opaque(OpacityCause::new(
                OpacityCauseKind::UnreadableBinderList,
                binding_form.span,
            ));
            return;
        }

        let groups: Vec<SchemeBindingGroup<'_>> = binding_form
            .children
            .iter()
            .filter(|entry| is_binding_container(entry))
            .map(|entry| SchemeBindingGroup {
                names: entry
                    .children
                    .first()
                    .map(formals_bound_names)
                    .unwrap_or_default(),
                value: entry.children.get(1),
            })
            .collect();

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        self.scheme_declare_groups(
            view,
            scope,
            inner,
            head,
            &groups,
            let_kind,
            BindingKind::Variable,
        );
        self.body(view.children.get(2..).unwrap_or_default(), inner);
        self.stack.rewind(depth);
    }

    /// `(do ((var init step) ...) (test result ...) body ...)`.
    fn scheme_do(&mut self, view: &ExpressionView, scope: ScopeId, head: &str) {
        let Some(binding_form) = view.children.get(1) else {
            return;
        };
        if !is_binding_container(binding_form) {
            self.mark_opaque(OpacityCause::new(
                OpacityCauseKind::UnreadableBinderList,
                binding_form.span,
            ));
            return;
        }

        // Initializers run once, before the loop variables exist.
        for spec in &binding_form.children {
            if let Some(init_form) = spec.children.get(1) {
                self.form(init_form, scope, 0);
            }
        }

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        for spec in &binding_form.children {
            let Some(bound) = specification_name(spec) else {
                continue;
            };
            let init_form = spec.children.get(1).map(|init| init.span);
            self.declare(view, inner, head, &bound, BindingKind::Variable, init_form);
        }

        // Step forms run once per iteration with every variable bound, so
        // they belong to the inner scope even though they sit in the binder.
        for spec in &binding_form.children {
            if let Some(step_form) = spec.children.get(2) {
                self.form(step_form, inner, 0);
            }
        }

        // Children 2.. are the test/result clause and then the body; all run
        // inside the loop's scope.
        self.body(view.children.get(2..).unwrap_or_default(), inner);
        self.stack.rewind(depth);
    }

    /// `(lambda formals body ...)`.
    fn scheme_lambda(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        formals_index: usize,
    ) -> bool {
        let Some(formals) = view.children.get(formals_index) else {
            return false;
        };
        let body_start = formals_index + 1;

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        // `(lambda args body)` collects every argument into one binding.
        if let Some(rest) = bare_scheme_name(formals) {
            self.declare(view, inner, head, &rest, BindingKind::Variable, None);
        } else if is_binding_container(formals) {
            // Default-value expressions are evaluated outside the scope the
            // parameters open, so they are walked before anything is declared.
            for default_form in scheme_formal_defaults_in(&formals.children) {
                self.form(default_form, scope, 0);
            }
            for formal in scheme_formals_in(&formals.children) {
                let bound = BoundName {
                    name: formal.name,
                    span: formal.span,
                };
                self.declare(view, inner, head, &bound, BindingKind::Variable, None);
            }
        } else {
            self.stack.rewind(depth);
            return false;
        }

        self.body(view.children.get(body_start..).unwrap_or_default(), inner);
        self.stack.rewind(depth);
        true
    }

    /// `(case-lambda (formals body ...) ...)`: each clause is its own scope.
    fn scheme_case_lambda(&mut self, view: &ExpressionView, scope: ScopeId, head: &str) {
        for index in 1..view.children.len() {
            let clause = &view.children[index];
            if !is_binding_container(clause) {
                continue;
            }
            self.scheme_lambda(clause, scope, head, 0);
        }
    }

    /// `(guard (var clause ...) body ...)`.
    ///
    /// The guarded body runs before the handler exists, so it belongs to the
    /// enclosing scope. Only the clauses see `var`.
    fn scheme_guard(&mut self, view: &ExpressionView, scope: ScopeId, head: &str) {
        self.body(view.children.get(2..).unwrap_or_default(), scope);

        let Some(handler) = view.children.get(1) else {
            return;
        };
        if !is_binding_container(handler) {
            return;
        }
        let Some(bound) = handler.children.first().and_then(bare_scheme_name) else {
            return;
        };

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, handler.span);
        self.declare(view, inner, head, &bound, BindingKind::Variable, None);
        self.body(handler.children.get(1..).unwrap_or_default(), inner);
        self.stack.rewind(depth);
    }

    /// `(parameterize ((param value) ...) body ...)` and `fluid-let`.
    ///
    /// Binds nothing lexically: the left-hand side of each pair *references*
    /// an existing parameter object or variable. Both halves and the body are
    /// ordinary expressions in the enclosing scope.
    fn scheme_dynamic_binding(&mut self, view: &ExpressionView, scope: ScopeId) {
        if let Some(binding_form) = view.children.get(1) {
            if is_binding_container(binding_form) {
                for entry in &binding_form.children {
                    self.form(entry, scope, 0);
                }
            }
        }

        self.body(view.children.get(2..).unwrap_or_default(), scope);
    }

    fn scheme_definition_form(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        definition: SchemeDefinitionForm,
    ) -> bool {
        match definition {
            SchemeDefinitionForm::Define | SchemeDefinitionForm::DefineContract => {
                self.scheme_define(view, scope, head)
            }
            SchemeDefinitionForm::DefineValues => {
                // The producer runs in the enclosing scope; the formals name
                // globals, which this file-local table does not register.
                if let Some(producer) = view.children.get(2) {
                    self.form(producer, scope, 0);
                }
                true
            }
            // Records, structs and syntax definitions hold names and
            // templates, not evaluated expressions. Descending would invent
            // references; a transformer body is code the expander runs.
            SchemeDefinitionForm::DefineRecordType
            | SchemeDefinitionForm::Struct
            | SchemeDefinitionForm::DefineStruct
            | SchemeDefinitionForm::DefineSyntax
            | SchemeDefinitionForm::DefineSyntaxRule => true,
            // A library's `begin` declarations do evaluate, and the generic
            // walk reaches them correctly.
            SchemeDefinitionForm::DefineLibrary => false,
        }
    }

    /// `(define name value)` and `(define (name . formals) body ...)`.
    ///
    /// The definition's own name is deliberately not registered: it names a
    /// *global*, and this table is the lexical binding context of one file.
    fn scheme_define(&mut self, view: &ExpressionView, scope: ScopeId, head: &str) -> bool {
        let Some(target) = view.children.get(1).and_then(scheme_define_target) else {
            return false;
        };
        let body = view.children.get(2..).unwrap_or_default();

        match target {
            SchemeDefineTarget::Variable { .. } => {
                self.body(body, scope);
            }
            SchemeDefineTarget::Procedure { formals, .. } => {
                // A curried `(define ((f a) b) ...)` binds every level's
                // parameters over the one body.
                let parameters: Vec<ExpressionView> = formals
                    .iter()
                    .flat_map(|level| level.children.iter().skip(1))
                    .cloned()
                    .collect();

                for default_form in scheme_formal_defaults_in(&parameters) {
                    self.form(default_form, scope, 0);
                }

                let depth = self.stack.depth();
                let inner = self.builder.open_scope(scope, view.span);
                for formal in scheme_formals_in(&parameters) {
                    let bound = BoundName {
                        name: formal.name,
                        span: formal.span,
                    };
                    self.declare(view, inner, head, &bound, BindingKind::Variable, None);
                }
                self.body(body, inner);
                self.stack.rewind(depth);
            }
        }

        true
    }

    fn declare_scheme_group(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        group: &SchemeBindingGroup<'_>,
        kind: BindingKind,
    ) {
        let init_form = group.value.map(|value| value.span);
        for bound in &group.names {
            self.declare(view, scope, head, bound, kind, init_form);
        }
    }
}

/// One entry of a Scheme binding list.
struct SchemeBindingGroup<'a> {
    names: Vec<BoundName>,
    value: Option<&'a ExpressionView>,
}

/// Reads a `((name value) ...)` binding list, accepting bracketed entries.
///
/// `None` means a shape the shared reader rejects. The reference query then
/// collects nothing anywhere in the form, so the table must record nothing
/// either.
fn scheme_binding_groups(binding_form: &ExpressionView) -> Option<Vec<SchemeBindingGroup<'_>>> {
    if !is_binding_container(binding_form) {
        return None;
    }

    binding_form
        .children
        .iter()
        .map(|entry| {
            if let Some(bound) = bare_scheme_name(entry) {
                return Some(SchemeBindingGroup {
                    names: vec![bound],
                    value: None,
                });
            }
            if !is_binding_container(entry) {
                return None;
            }
            let bound = entry.children.first().and_then(bare_scheme_name)?;
            Some(SchemeBindingGroup {
                names: vec![bound],
                value: entry.children.get(1),
            })
        })
        .collect()
}

fn formals_bound_names(formals: &ExpressionView) -> Vec<BoundName> {
    if let Some(bound) = bare_scheme_name(formals) {
        return vec![bound];
    }
    scheme_formals_in(&formals.children)
        .into_iter()
        .map(|formal| BoundName {
            name: formal.name,
            span: formal.span,
        })
        .collect()
}

/// The name a `do` variable specification binds: `x` or `(x init step)`.
fn specification_name(spec: &ExpressionView) -> Option<BoundName> {
    bare_scheme_name(spec).or_else(|| spec.children.first().and_then(bare_scheme_name))
}

/// A plain Scheme identifier atom read as a bound name.
fn bare_scheme_name(view: &ExpressionView) -> Option<BoundName> {
    Some(BoundName {
        name: scheme_identifier_text(view)?.to_owned(),
        span: atom_symbol_span(view)?,
    })
}

/// Whether `(let ...)` is the named variant.
fn is_named_let(view: &ExpressionView) -> bool {
    view.children
        .get(1)
        .is_some_and(|child| scheme_identifier_text(child).is_some())
}

/// Whether a node can hold binding entries or parameters.
fn is_binding_container(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && matches!(view.delimiter, Some(Delimiter::Paren | Delimiter::Bracket))
        && view.reader_prefixes.is_empty()
}
