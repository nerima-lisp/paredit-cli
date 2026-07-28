//! Emacs Lisp binding forms, and the file-level fact that decides what `let`
//! means.
//!
//! The shapes come from [`EmacsLispBinderShape`], so this module is the
//! *traversal* rather than the knowledge: given "the binders are a pair list
//! at child 1, sequentially visible, body from child 2", walking it is one
//! function no matter whether the head was `let*`, `dlet`, `when-let*`, or
//! `and-let*`. That is what makes the `subr-x` family cheap to cover — each
//! new spelling is a table row, not a branch.
//!
//! What is *not* shared with Common Lisp is dynamic binding. There, a name is
//! special because a declaration says so. Here, it is special because a
//! `defvar` somewhere named it — or because the file's first line never said
//! `lexical-binding: t`, in which case every variable binding in it is
//! dynamic. See [`EmacsLispDynamicScope`].

use std::collections::HashSet;

use crate::lexical_scope::{BoundName, binding_pattern_bound_names};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::emacs_lisp::{
    EmacsLispBinderShape, EmacsLispBindingScope, EmacsLispBindingVisibility,
    EmacsLispLocalCallableForm, EmacsLispOperator, emacs_lisp_file_header,
};
use paredit_core_syntax::sexpr::reader::atom_symbol_span;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView, SyntaxTree};

use super::super::model::{BindingKind, OpacityCause, OpacityCauseKind, ScopeId};
use super::builder::{Walk, head_text};
use super::lambda_lists::lambda_list_names;

/// Whether a variable binding in this file is lexical, and which names are
/// dynamic regardless.
///
/// Two independent facts, because Emacs Lisp answers the question twice.
/// Without `lexical-binding: t` on line 1 *every* `let` in the file binds
/// dynamically, so nothing in it is lexically scoped at all. With it, a
/// binding is still dynamic if the name was declared by a `defvar`-family
/// form — and that declaration may sit in another file entirely, which is why
/// this index is a lower bound rather than an answer.
#[derive(Debug, Default)]
pub(super) struct EmacsLispDynamicScope {
    /// `false` when the file has no `lexical-binding: t`, which makes every
    /// variable binding in it dynamic.
    file_is_lexical: bool,
    /// Names some `defvar`/`defconst`/`defcustom`/`defvar-local` in this file
    /// declares special.
    declared: HashSet<String>,
    /// Whether this dialect has the concept at all.
    applies: bool,
}

impl EmacsLispDynamicScope {
    /// Scans a file, or produces an inert index for a dialect this does not
    /// apply to.
    pub(super) fn of(dialect: Dialect, tree: &SyntaxTree, input: &str) -> Self {
        if dialect != Dialect::EmacsLisp {
            return Self::default();
        }

        let mut scope = Self {
            file_is_lexical: emacs_lisp_file_header(input).lexical_binding().is_lexical(),
            declared: HashSet::new(),
            applies: true,
        };
        scope.visit(&tree.root_view());
        scope
    }

    /// Whether a `let`-style binding of `name` by `operator` is dynamic.
    pub(super) fn is_dynamic(&self, operator: Option<EmacsLispOperator>, name: &str) -> bool {
        if !self.applies {
            return false;
        }
        match operator.map(EmacsLispOperator::binding_scope) {
            Some(EmacsLispBindingScope::AlwaysDynamic) => true,
            Some(EmacsLispBindingScope::AlwaysLexical) => false,
            _ => !self.file_is_lexical || self.declared.contains(name),
        }
    }

    /// Collects every name a `defvar`-family form in the file declares.
    ///
    /// Nested forms count. A bare `(defvar x)` inside a function body marks
    /// `x` special only for the rest of that lexical unit in real Emacs, but
    /// treating it as file-wide is the conservative direction here: an extra
    /// dynamic binding costs a report that stays silent, while a missed one
    /// costs a "this binding is never used" claim about a name whose whole
    /// purpose is to be read by a callee.
    fn visit(&mut self, view: &ExpressionView) {
        if view.kind == ExpressionKind::List
            && let Some(head) = view.children.first().and_then(head_text)
            && EmacsLispOperator::from_head(head)
                .is_some_and(EmacsLispOperator::declares_dynamic_variable)
            && let Some(name) = view.children.get(1).and_then(head_text)
        {
            self.declared.insert(name.to_owned());
        }

        for child in &view.children {
            self.visit(child);
        }
    }
}

impl Walk<'_> {
    /// Dispatches an Emacs Lisp list to its binding-form handler. Returns
    /// whether it was consumed.
    pub(super) fn emacs_lisp_binder(&mut self, view: &ExpressionView, scope: ScopeId) -> bool {
        let Some(head) = head_text(&view.children[0]) else {
            return false;
        };
        let Some(operator) = EmacsLispOperator::from_head(head) else {
            return false;
        };

        // A definition's own name is a global, so it is not registered; only
        // its parameters open a scope. The check comes first because
        // `defun` is not a binder shape but does open a lambda scope.
        if operator.is_definition() {
            return self.emacs_lisp_definition(view, scope, head, operator);
        }

        let Some(shape) = operator.binder_shape() else {
            return false;
        };

        match shape {
            EmacsLispBinderShape::PairList {
                container,
                body_start,
                body_end,
                visibility,
            } => self.emacs_lisp_pair_list(
                view,
                scope,
                head,
                operator,
                container,
                body_start,
                body_end,
                visibility,
                PatternKind::BareName,
            ),
            EmacsLispBinderShape::PatternPairList {
                container,
                body_start,
                visibility,
            } => self.emacs_lisp_pair_list(
                view,
                scope,
                head,
                operator,
                container,
                body_start,
                None,
                visibility,
                PatternKind::Destructuring,
            ),
            EmacsLispBinderShape::SinglePattern {
                pattern,
                value,
                body_start,
            }
            | EmacsLispBinderShape::ArgumentList {
                arglist: pattern,
                value,
                body_start,
            } => self.emacs_lisp_single_pattern(view, scope, head, pattern, value, body_start),
            EmacsLispBinderShape::Iteration {
                spec,
                body_start,
                has_result_form,
            } => self.emacs_lisp_iteration(view, scope, head, spec, body_start, has_result_form),
            EmacsLispBinderShape::PatternIteration { spec, body_start } => {
                self.emacs_lisp_pattern_iteration(view, scope, head, spec, body_start)
            }
            EmacsLispBinderShape::VariableSpecs {
                container,
                body_start,
                visibility,
            } => {
                self.emacs_lisp_variable_specs(view, scope, head, container, body_start, visibility)
            }
            EmacsLispBinderShape::LocalCallables {
                container,
                body_start,
                form,
            } => self.emacs_lisp_local_callables(view, scope, head, container, body_start, form),
            EmacsLispBinderShape::NamedLet {
                name,
                container,
                body_start,
            } => self.emacs_lisp_named_let(view, scope, head, name, container, body_start),
            EmacsLispBinderShape::ConditionCase {
                variable,
                protected,
                first_handler,
            } => self.emacs_lisp_condition_case(
                view,
                scope,
                head,
                variable,
                protected,
                first_handler,
            ),
            EmacsLispBinderShape::Parameters {
                arglist,
                body_start,
            } => match view.children.get(arglist) {
                Some(parameter_form) => self.lambda_scope(
                    parameter_form,
                    &view.children[body_start..],
                    scope,
                    view,
                    head,
                ),
                None => false,
            },
            EmacsLispBinderShape::Slots {
                container,
                object,
                body_start,
            } => self.emacs_lisp_slots(view, scope, head, container, object, body_start),
        }
    }

    /// `let`, `let*`, `dlet`, `letrec`, `cl-symbol-macrolet`, and the whole
    /// `if-let*` / `when-let*` / `and-let*` / `while-let` family.
    #[allow(clippy::too_many_arguments)]
    fn emacs_lisp_pair_list(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        operator: EmacsLispOperator,
        container: usize,
        body_start: usize,
        body_end: Option<usize>,
        visibility: EmacsLispBindingVisibility,
        pattern_kind: PatternKind,
    ) -> bool {
        let Some(binding_form) = view.children.get(container) else {
            return false;
        };
        let Some(groups) = pair_list_groups(binding_form, pattern_kind) else {
            self.mark_opaque(OpacityCause::new(
                OpacityCauseKind::UnreadableBinderList,
                binding_form.span,
            ));
            return true;
        };

        let kind = if operator.binds_symbol_macro() {
            BindingKind::SymbolMacro
        } else {
            BindingKind::Variable
        };

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        match visibility {
            // `letrec` makes every name visible to every initializer,
            // including its own — that is the whole reason it exists, so a
            // closure built in one initializer can call a sibling.
            EmacsLispBindingVisibility::Recursive => {
                for group in &groups {
                    self.declare_emacs_lisp_group(view, inner, head, operator, group, kind);
                }
                for group in &groups {
                    if let Some(value) = group.value {
                        self.form(value, inner, 0);
                    }
                }
            }
            EmacsLispBindingVisibility::Sequential => {
                for group in &groups {
                    if let Some(value) = group.value {
                        self.form(value, inner, 0);
                    }
                    self.declare_emacs_lisp_group(view, inner, head, operator, group, kind);
                }
            }
            EmacsLispBindingVisibility::Parallel => {
                for group in &groups {
                    if let Some(value) = group.value {
                        // The *outer* scope: a parallel binder's values cannot
                        // see any of the names it is about to bind.
                        self.form(value, scope, 0);
                    }
                }
                for group in &groups {
                    self.declare_emacs_lisp_group(view, inner, head, operator, group, kind);
                }
            }
        }

        let end = body_end
            .unwrap_or(view.children.len())
            .min(view.children.len());
        let start = body_start.min(end);
        self.body(&view.children[start..end], inner);
        self.stack.rewind(depth);

        // Anything past `body_end` — an `if-let*` ELSE branch — is evaluated
        // with the bindings gone, so it is walked in the enclosing scope.
        if end < view.children.len() {
            self.body(&view.children[end..], scope);
        }
        true
    }

    /// `seq-let`, `cl-destructuring-bind`, and `cl-multiple-value-bind`: one
    /// pattern, one value form evaluated outside the scope it opens.
    fn emacs_lisp_single_pattern(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        pattern: usize,
        value: usize,
        body_start: usize,
    ) -> bool {
        let Some(pattern_form) = view.children.get(pattern) else {
            return false;
        };
        let Some(value_form) = view.children.get(value) else {
            return false;
        };

        self.form(value_form, scope, 0);

        if pattern_form.kind != ExpressionKind::List {
            return false;
        }

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);
        for bound in lambda_list_names(pattern_form) {
            self.declare(view, inner, head, &bound, BindingKind::Variable, None);
        }
        self.body(&view.children[body_start.min(view.children.len())..], inner);
        self.stack.rewind(depth);
        true
    }

    /// `dolist` and `dotimes`: `(dolist (VAR LIST [RESULT]) BODY…)`.
    fn emacs_lisp_iteration(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        spec: usize,
        body_start: usize,
        has_result_form: bool,
    ) -> bool {
        let Some(spec_form) = view.children.get(spec) else {
            return false;
        };

        if let Some(source) = spec_form.children.get(1) {
            self.form(source, scope, 0);
        }

        let Some(bound) = spec_form.children.first().and_then(bare_name) else {
            return false;
        };

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);
        self.declare(view, inner, head, &bound, BindingKind::Variable, None);

        // The result form runs after the loop but with the variable still
        // bound — in `dolist` it is the idiomatic place to read the
        // accumulator the body pushed onto.
        if has_result_form && let Some(result) = spec_form.children.get(2) {
            self.form(result, inner, 0);
        }
        self.body(&view.children[body_start.min(view.children.len())..], inner);
        self.stack.rewind(depth);
        true
    }

    /// `pcase-dolist`: `(pcase-dolist (PATTERN LIST) BODY…)`.
    fn emacs_lisp_pattern_iteration(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        spec: usize,
        body_start: usize,
    ) -> bool {
        let Some(spec_form) = view.children.get(spec) else {
            return false;
        };

        if let Some(source) = spec_form.children.get(1) {
            self.form(source, scope, 0);
        }

        let Some(pattern) = spec_form.children.first() else {
            return false;
        };

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);
        for bound in pcase_pattern_names(pattern) {
            self.declare(view, inner, head, &bound, BindingKind::Variable, None);
        }
        self.body(&view.children[body_start.min(view.children.len())..], inner);
        self.stack.rewind(depth);
        true
    }

    /// `cl-do` and `cl-do*`.
    fn emacs_lisp_variable_specs(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        container: usize,
        body_start: usize,
        visibility: EmacsLispBindingVisibility,
    ) -> bool {
        let Some(binding_form) = view.children.get(container) else {
            return false;
        };

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);
        let sequential = visibility.is_sequential();

        for spec in &binding_form.children {
            if sequential && let Some(init) = variable_spec_child(spec, 1) {
                self.form(init, inner, 0);
            } else if !sequential && let Some(init) = variable_spec_child(spec, 1) {
                self.form(init, scope, 0);
            }
            if sequential {
                self.declare_variable_spec_name(view, inner, head, spec);
            }
        }
        if !sequential {
            for spec in &binding_form.children {
                self.declare_variable_spec_name(view, inner, head, spec);
            }
        }

        // Step forms run once per iteration with every variable bound, so
        // they belong to the inner scope for both spellings.
        for spec in &binding_form.children {
            if let Some(step) = variable_spec_child(spec, 2) {
                self.form(step, inner, 0);
            }
        }

        // `(cl-do VARLIST (END RESULT…) BODY…)`: the end test and its result
        // forms see the loop variables too.
        if let Some(end_clause) = view.children.get(2) {
            self.form(end_clause, inner, 0);
        }
        self.body(&view.children[body_start.min(view.children.len())..], inner);
        self.stack.rewind(depth);
        true
    }

    /// `cl-flet`, `cl-flet*`, `cl-labels`, `cl-macrolet`, and the two
    /// obsolete `cl.el` spellings.
    fn emacs_lisp_local_callables(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        container: usize,
        body_start: usize,
        form: EmacsLispLocalCallableForm,
    ) -> bool {
        let Some(binding_form) = view.children.get(container) else {
            return false;
        };

        let kind = if form.is_macro() {
            BindingKind::Macro
        } else {
            BindingKind::Function
        };

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        if form.group_is_self_visible() {
            // `cl-labels` binds the whole group before any definition body is
            // walked, so a local function can recurse and call its siblings.
            self.declare_emacs_lisp_callables(view, inner, head, binding_form, kind);
            self.walk_emacs_lisp_callable_specs(binding_form, inner, head);
        } else if form.is_sequential() {
            // `cl-flet*` extends the group one definition at a time.
            for spec in &binding_form.children {
                self.walk_emacs_lisp_callable_spec(spec, inner, head);
                if let Some(bound) = spec.children.first().and_then(bare_name) {
                    self.declare(view, inner, head, &bound, kind, None);
                }
            }
        } else {
            self.walk_emacs_lisp_callable_specs(binding_form, inner, head);
            self.declare_emacs_lisp_callables(view, inner, head, binding_form, kind);
        }

        self.body(&view.children[body_start.min(view.children.len())..], inner);
        self.stack.rewind(depth);
        true
    }

    /// `(named-let NAME ((var init) …) BODY…)`: the name is bound over the
    /// body as a function, the variables as ordinary parallel bindings.
    fn emacs_lisp_named_let(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        name: usize,
        container: usize,
        body_start: usize,
    ) -> bool {
        let Some(binding_form) = view.children.get(container) else {
            return false;
        };
        let Some(groups) = pair_list_groups(binding_form, PatternKind::BareName) else {
            self.mark_opaque(OpacityCause::new(
                OpacityCauseKind::UnreadableBinderList,
                binding_form.span,
            ));
            return true;
        };

        // The initial values are evaluated before the loop starts, outside
        // the scope it opens.
        for group in &groups {
            if let Some(value) = group.value {
                self.form(value, scope, 0);
            }
        }

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        if let Some(bound) = view.children.get(name).and_then(bare_name) {
            self.declare(view, inner, head, &bound, BindingKind::Function, None);
        }
        for group in &groups {
            self.declare_emacs_lisp_group(
                view,
                inner,
                head,
                EmacsLispOperator::NamedLet,
                group,
                BindingKind::Variable,
            );
        }

        self.body(&view.children[body_start.min(view.children.len())..], inner);
        self.stack.rewind(depth);
        true
    }

    /// `(condition-case VAR BODYFORM HANDLERS…)`.
    ///
    /// `VAR` is bound in the handlers and *not* in `BODYFORM`, which is the
    /// one thing about this form worth getting right — it is the opposite of
    /// how it reads.
    fn emacs_lisp_condition_case(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        variable: usize,
        protected: usize,
        first_handler: usize,
    ) -> bool {
        if let Some(protected_form) = view.children.get(protected) {
            self.form(protected_form, scope, 0);
        }

        let bound = view.children.get(variable).and_then(bare_name);
        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);

        // `(condition-case nil …)` names no variable; `nil` there is the
        // documented way to say "no binding".
        if let Some(bound) = bound.filter(|bound| bound.name != "nil") {
            self.declare(view, inner, head, &bound, BindingKind::Variable, None);
        }

        for handler in view
            .children
            .get(first_handler..)
            .unwrap_or_default()
            .iter()
        {
            // A handler is `(CONDITION BODY…)`; the condition name is a
            // symbol the reader never evaluates.
            self.body(handler.children.get(1..).unwrap_or_default(), inner);
        }

        self.stack.rewind(depth);
        true
    }

    /// EIEIO's `(with-slots SPECS OBJECT BODY…)`.
    fn emacs_lisp_slots(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        container: usize,
        object: usize,
        body_start: usize,
    ) -> bool {
        let Some(slot_specs) = view.children.get(container) else {
            return false;
        };
        if let Some(object_form) = view.children.get(object) {
            self.form(object_form, scope, 0);
        }

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, view.span);
        for spec in &slot_specs.children {
            let Some(bound) = bare_name(spec).or_else(|| spec.children.first().and_then(bare_name))
            else {
                continue;
            };
            self.declare(view, inner, head, &bound, BindingKind::Slot, None);
        }
        self.body(&view.children[body_start.min(view.children.len())..], inner);
        self.stack.rewind(depth);
        true
    }

    /// A `def…` form: its name is a global, so only its parameters — when it
    /// has any — open a scope.
    fn emacs_lisp_definition(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        operator: EmacsLispOperator,
    ) -> bool {
        let arglist_index = operator
            .callable_shape()
            .map(|shape| shape.arglist_child_index())
            .or_else(|| {
                // `cl-defmethod`'s optional qualifier shifts the arglist, so
                // it is found by scanning rather than at a fixed index.
                let from = operator.definition_arglist_is_first_list_at_or_after()?;
                view.children
                    .iter()
                    .enumerate()
                    .skip(from)
                    .find(|(_, child)| child.kind == ExpressionKind::List)
                    .map(|(index, _)| index)
            });

        let Some(arglist_index) = arglist_index else {
            // A variable or customization definition: no scope, but its value
            // form is ordinary code.
            self.body(view.children.get(2..).unwrap_or_default(), scope);
            return true;
        };

        let Some(parameter_form) = view.children.get(arglist_index) else {
            return false;
        };
        let body = view.children.get(arglist_index + 1..).unwrap_or_default();

        if operator == EmacsLispOperator::ClDefmethod {
            return self.emacs_lisp_method_scope(parameter_form, body, scope, view, head);
        }
        self.lambda_scope(parameter_form, body, scope, view, head)
    }

    /// A `cl-defmethod` arglist, whose required parameters may be specialized.
    ///
    /// `(cl-defmethod handle ((obj my-type) arg) …)` binds `obj` and `arg`.
    /// `my-type` names the class the method dispatches on — it is read, never
    /// evaluated, and never bound. The ordinary lambda-list reader cannot know
    /// that: in every other form `(a b)` in required position is a
    /// destructuring pattern binding both names, so running it here would
    /// register the type as a variable and let a rename rewrite it.
    ///
    /// Only the required section differs. Once `&optional` or `&rest`
    /// appears, specializers are no longer allowed and the rest of the list is
    /// an ordinary lambda list.
    fn emacs_lisp_method_scope(
        &mut self,
        parameter_form: &ExpressionView,
        body_forms: &[ExpressionView],
        scope: ScopeId,
        target: &ExpressionView,
        head: &str,
    ) -> bool {
        if parameter_form.kind != ExpressionKind::List {
            return false;
        }

        let specialized_end = parameter_form
            .children
            .iter()
            .position(|child| head_text(child).is_some_and(|text| text.starts_with('&')))
            .unwrap_or(parameter_form.children.len());

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, target.span);

        for parameter in &parameter_form.children[..specialized_end] {
            // `(obj my-type)` — the name is the first element; `obj` alone is
            // an unspecialized parameter.
            let bound =
                bare_name(parameter).or_else(|| parameter.children.first().and_then(bare_name));
            if let Some(bound) = bound {
                self.declare(target, inner, head, &bound, BindingKind::Variable, None);
            }
        }

        for parameter in &parameter_form.children[specialized_end..] {
            if head_text(parameter).is_some_and(|text| text.starts_with('&')) {
                continue;
            }
            for bound in binding_pattern_bound_names(parameter) {
                self.declare(target, inner, head, &bound, BindingKind::Variable, None);
            }
        }

        self.body(body_forms, inner);
        self.stack.rewind(depth);
        true
    }

    fn declare_emacs_lisp_group(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        operator: EmacsLispOperator,
        group: &EmacsLispBindingGroup<'_>,
        kind: BindingKind,
    ) {
        // `operator` is not consulted here: `Walk::declare` resolves it from
        // `head` and asks `EmacsLispDynamicScope` itself, so every binder —
        // including the lambda lists, which have no operator to hand — gets
        // the same answer from one place.
        debug_assert_eq!(EmacsLispOperator::from_head(head), Some(operator));

        let init_form = group.value.map(|value| value.span);
        for bound in &group.names {
            self.declare(view, scope, head, bound, kind, init_form);
        }
    }

    fn declare_emacs_lisp_callables(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        binding_form: &ExpressionView,
        kind: BindingKind,
    ) {
        for spec in &binding_form.children {
            let Some(bound) = spec.children.first().and_then(bare_name) else {
                continue;
            };
            self.declare(view, scope, head, &bound, kind, None);
        }
    }

    fn walk_emacs_lisp_callable_specs(
        &mut self,
        binding_form: &ExpressionView,
        scope: ScopeId,
        head: &str,
    ) {
        for spec in &binding_form.children {
            self.walk_emacs_lisp_callable_spec(spec, scope, head);
        }
    }

    fn walk_emacs_lisp_callable_spec(&mut self, spec: &ExpressionView, scope: ScopeId, head: &str) {
        if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
            return;
        }
        let Some(parameter_form) = spec.children.get(1) else {
            return;
        };
        self.lambda_scope(parameter_form, &spec.children[2..], scope, spec, head);
    }

    fn declare_variable_spec_name(
        &mut self,
        view: &ExpressionView,
        scope: ScopeId,
        head: &str,
        spec: &ExpressionView,
    ) {
        let Some(bound) = bare_name(spec).or_else(|| spec.children.first().and_then(bare_name))
        else {
            return;
        };
        let init_form = variable_spec_child(spec, 1).map(|init| init.span);
        self.declare(view, scope, head, &bound, BindingKind::Variable, init_form);
    }
}

/// One entry of a binding list.
struct EmacsLispBindingGroup<'a> {
    names: Vec<BoundName>,
    value: Option<&'a ExpressionView>,
}

/// How the left-hand side of a binding entry is read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PatternKind {
    /// A symbol, as in `let`: `(x 1)` or bare `x`.
    BareName,
    /// A `pcase` pattern, as in `pcase-let`: `` (`(,a . ,b) expr) ``.
    Destructuring,
}

/// Splits `((name value) …)` into its entries.
///
/// `None` means a shape this layer cannot read, which the caller turns into an
/// opacity mark rather than a silent zero-binding result.
fn pair_list_groups(
    binding_form: &ExpressionView,
    pattern_kind: PatternKind,
) -> Option<Vec<EmacsLispBindingGroup<'_>>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        return None;
    }

    binding_form
        .children
        .iter()
        .map(|pair| {
            if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren) {
                // `(let (x) …)` is the documented shorthand for `((x nil))`.
                if pair.kind != ExpressionKind::Atom {
                    return None;
                }
                let names = binding_pattern_bound_names(pair);
                return (names.len() == 1).then_some(EmacsLispBindingGroup { names, value: None });
            }

            // `when-let*` allows a one-element entry `(EXPR)` that binds
            // nothing and is tested for nil, and a bare `(SYMBOL)` that binds
            // the symbol to itself. Both have to survive the read; only the
            // over-long entry is a shape this layer refuses.
            if pair.children.is_empty() || pair.children.len() > 2 {
                return None;
            }

            let names = match pattern_kind {
                PatternKind::BareName => binding_pattern_bound_names(&pair.children[0]),
                PatternKind::Destructuring => pcase_pattern_names(&pair.children[0]),
            };
            Some(EmacsLispBindingGroup {
                names,
                value: pair.children.get(1),
            })
        })
        .collect()
}

/// The names a `pcase` pattern binds.
///
/// Only the two constructs that bind unconditionally are read: a bare symbol,
/// and a `,symbol` inside a backquoted template. Everything else — `(pred …)`,
/// `(guard …)`, `(or …)` — either binds nothing or binds conditionally, and a
/// name this function does not return is simply left unresolved rather than
/// wrongly attributed.
fn pcase_pattern_names(pattern: &ExpressionView) -> Vec<BoundName> {
    let mut names = Vec::new();
    collect_pcase_names(pattern, &mut names);
    names
}

fn collect_pcase_names(pattern: &ExpressionView, output: &mut Vec<BoundName>) {
    match pattern.kind {
        ExpressionKind::Atom => {
            // `_` is pcase's explicit "match anything, bind nothing", and a
            // keyword or `nil`/`t` is a literal to compare against.
            let Some(text) = head_text(pattern) else {
                // A prefixed atom: `,x` in a backquoted template binds `x`.
                if let (Some(text), Some(span)) =
                    (pattern.text.as_deref(), atom_symbol_span(pattern))
                    && is_bindable_pcase_name(text)
                {
                    output.push(BoundName {
                        name: text.to_owned(),
                        span,
                    });
                }
                return;
            };
            if is_bindable_pcase_name(text)
                && let Some(span) = atom_symbol_span(pattern)
            {
                output.push(BoundName {
                    name: text.to_owned(),
                    span,
                });
            }
        }
        ExpressionKind::List | ExpressionKind::Root => {
            for child in &pattern.children {
                collect_pcase_names(child, output);
            }
        }
    }
}

fn is_bindable_pcase_name(text: &str) -> bool {
    !text.is_empty()
        && text != "_"
        && text != "nil"
        && text != "t"
        // The dotted-pair marker in `` `(,head . ,tail) `` is punctuation the
        // parser hands back as an atom, not a name the pattern binds.
        && text != "."
        && !text.starts_with(':')
        && !text.starts_with('&')
        // A pcase pattern head such as `pred`, `guard`, or `app` is an
        // operator, not a binding; the ones that do bind (`let`, `and`, `or`)
        // bind through their subpatterns, which this walk reaches anyway.
        && !matches!(
            text,
            "pred" | "guard" | "app" | "cl-type" | "rx" | "let" | "and" | "or" | "quote"
        )
}

fn variable_spec_child(spec: &ExpressionView, index: usize) -> Option<&ExpressionView> {
    (spec.kind == ExpressionKind::List)
        .then(|| spec.children.get(index))
        .flatten()
}

fn bare_name(view: &ExpressionView) -> Option<BoundName> {
    Some(BoundName {
        name: head_text(view)?.to_owned(),
        span: atom_symbol_span(view)?,
    })
}
