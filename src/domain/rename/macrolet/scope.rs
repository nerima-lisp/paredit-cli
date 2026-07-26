use crate::domain::callable_scope::is_macro_callable_form;
use crate::domain::common_lisp::CommonLispLocalCallableForm;
use crate::domain::common_lisp::{
    common_lisp_operator_head_eq, common_lisp_symbol_reference_eq,
    has_common_lisp_package_qualifier,
};
use crate::domain::rename::reader::atom_symbol_text;
use crate::domain::rename::selection::list_head;
use crate::domain::sexpr::{ExpressionView, SymbolName};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct MacroletRenameScope {
    active_target_depth: usize,
    shadowed_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalCallableRenameKind {
    Macro,
    Function,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TargetBindingPresence {
    Absent,
    Present,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LocalCallableScopes {
    pub(super) body: MacroletRenameScope,
    pub(super) binding_body: MacroletRenameScope,
}

impl LocalCallableRenameKind {
    pub(super) const fn matches_target_form(self, form: CommonLispLocalCallableForm) -> bool {
        match self {
            Self::Macro => is_macro_callable_form(form),
            Self::Function => matches!(
                form,
                CommonLispLocalCallableForm::Flet | CommonLispLocalCallableForm::Labels
            ),
        }
    }
}

impl MacroletRenameScope {
    pub(super) const fn is_target_active(self) -> bool {
        self.active_target_depth > 0
    }

    pub(super) const fn is_shadowed(self) -> bool {
        self.shadowed_depth > 0
    }

    const fn enter_active_target(mut self) -> Self {
        self.active_target_depth += 1;
        self
    }

    const fn enter_shadowed(mut self) -> Self {
        self.shadowed_depth += 1;
        self
    }
}

pub(super) fn allows_function_reference_rename(
    scope: MacroletRenameScope,
    target_text: &str,
) -> bool {
    !scope.is_shadowed() || has_common_lisp_package_qualifier(target_text)
}

pub(super) const fn reader_lambda_body_scope(scope: MacroletRenameScope) -> MacroletRenameScope {
    scope.enter_active_target()
}

pub(super) fn symbol_macrolet_shadowing_scope(
    scope: MacroletRenameScope,
    view: &ExpressionView,
    from: &SymbolName,
) -> MacroletRenameScope {
    if symbol_macrolet_binds_name(view, from) {
        scope.enter_shadowed()
    } else {
        scope
    }
}

fn symbol_macrolet_binds_name(view: &ExpressionView, from: &SymbolName) -> bool {
    let Some(head) = list_head(view) else {
        return false;
    };
    if !common_lisp_operator_head_eq(head, "symbol-macrolet") {
        return false;
    }

    let Some(bindings) = view.children.get(1) else {
        return false;
    };

    bindings.children.iter().any(|binding| {
        binding
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|name| common_lisp_symbol_reference_eq(name, from.as_str()))
    })
}

pub(super) fn local_callable_body_scope(
    scope: MacroletRenameScope,
    kind: LocalCallableRenameKind,
    form: CommonLispLocalCallableForm,
    target_binding: TargetBindingPresence,
) -> MacroletRenameScope {
    if target_binding == TargetBindingPresence::Absent {
        return scope;
    }

    match kind {
        LocalCallableRenameKind::Macro
            if is_macro_callable_form(form)
                && !scope.is_target_active()
                && !scope.is_shadowed() =>
        {
            scope.enter_active_target()
        }
        LocalCallableRenameKind::Function
            if matches!(
                form,
                CommonLispLocalCallableForm::Flet | CommonLispLocalCallableForm::Labels
            ) && !scope.is_target_active()
                && !scope.is_shadowed() =>
        {
            scope.enter_active_target()
        }
        LocalCallableRenameKind::Macro | LocalCallableRenameKind::Function => {
            scope.enter_shadowed()
        }
    }
}

pub(super) fn local_callable_binding_body_scope(
    scope: MacroletRenameScope,
    kind: LocalCallableRenameKind,
    form: CommonLispLocalCallableForm,
    target_binding: TargetBindingPresence,
) -> MacroletRenameScope {
    if target_binding == TargetBindingPresence::Absent {
        return scope;
    }

    match (kind, form) {
        (LocalCallableRenameKind::Macro, CommonLispLocalCallableForm::Labels) => {
            scope.enter_shadowed()
        }
        (LocalCallableRenameKind::Function, CommonLispLocalCallableForm::Labels)
            if !scope.is_target_active() && !scope.is_shadowed() =>
        {
            scope.enter_active_target()
        }
        (LocalCallableRenameKind::Function, CommonLispLocalCallableForm::Labels) => {
            scope.enter_shadowed()
        }
        (LocalCallableRenameKind::Function, CommonLispLocalCallableForm::Flet)
        | (
            LocalCallableRenameKind::Macro,
            CommonLispLocalCallableForm::Macrolet | CommonLispLocalCallableForm::CompilerMacrolet,
        ) => shadow_current_target_in_definition_body(scope),
        _ => scope,
    }
}

const fn shadow_current_target_in_definition_body(scope: MacroletRenameScope) -> MacroletRenameScope {
    if scope.is_target_active() || scope.is_shadowed() {
        scope
    } else {
        scope.enter_shadowed()
    }
}

pub(super) fn target_binding_presence(
    local_names: &[String],
    from: &SymbolName,
) -> TargetBindingPresence {
    if local_names
        .iter()
        .any(|name| common_lisp_symbol_reference_eq(name, from.as_str()))
    {
        TargetBindingPresence::Present
    } else {
        TargetBindingPresence::Absent
    }
}

pub(super) fn local_callable_scopes(
    scope: MacroletRenameScope,
    kind: LocalCallableRenameKind,
    form: CommonLispLocalCallableForm,
    target_binding: TargetBindingPresence,
) -> LocalCallableScopes {
    LocalCallableScopes {
        body: local_callable_body_scope(scope, kind, form, target_binding),
        binding_body: local_callable_binding_body_scope(scope, kind, form, target_binding),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macrolet_binding_bodies_keep_outer_target_visible() {
        let scope = MacroletRenameScope::default().enter_active_target();

        let next = local_callable_binding_body_scope(
            scope,
            LocalCallableRenameKind::Macro,
            CommonLispLocalCallableForm::Macrolet,
            TargetBindingPresence::Present,
        );

        assert!(next.is_target_active());
        assert!(!next.is_shadowed());
    }

    #[test]
    fn flet_binding_bodies_shadow_current_function_target() {
        let next = local_callable_binding_body_scope(
            MacroletRenameScope::default(),
            LocalCallableRenameKind::Function,
            CommonLispLocalCallableForm::Flet,
            TargetBindingPresence::Present,
        );

        assert!(!next.is_target_active());
        assert!(next.is_shadowed());
    }

    #[test]
    fn macrolet_binding_bodies_shadow_current_macro_target() {
        let next = local_callable_binding_body_scope(
            MacroletRenameScope::default(),
            LocalCallableRenameKind::Macro,
            CommonLispLocalCallableForm::Macrolet,
            TargetBindingPresence::Present,
        );

        assert!(!next.is_target_active());
        assert!(next.is_shadowed());
    }

    #[test]
    fn labels_binding_bodies_activate_target_for_recursive_function_rename() {
        let next = local_callable_binding_body_scope(
            MacroletRenameScope::default(),
            LocalCallableRenameKind::Function,
            CommonLispLocalCallableForm::Labels,
            TargetBindingPresence::Present,
        );

        assert!(next.is_target_active());
        assert!(!next.is_shadowed());
    }

    #[test]
    fn labels_binding_bodies_shadow_outer_macro_target() {
        let scope = MacroletRenameScope::default().enter_active_target();

        let next = local_callable_binding_body_scope(
            scope,
            LocalCallableRenameKind::Macro,
            CommonLispLocalCallableForm::Labels,
            TargetBindingPresence::Present,
        );

        assert!(next.is_target_active());
        assert!(next.is_shadowed());
    }

    #[test]
    fn callable_form_body_is_unchanged_when_target_is_not_bound() {
        let scope = MacroletRenameScope::default().enter_active_target();

        let next = local_callable_body_scope(
            scope,
            LocalCallableRenameKind::Function,
            CommonLispLocalCallableForm::Flet,
            TargetBindingPresence::Absent,
        );

        assert!(next.is_target_active());
        assert!(!next.is_shadowed());
    }

    #[test]
    fn local_callable_scopes_bundle_body_and_binding_body_transitions() {
        let scopes = local_callable_scopes(
            MacroletRenameScope::default(),
            LocalCallableRenameKind::Function,
            CommonLispLocalCallableForm::Labels,
            TargetBindingPresence::Present,
        );

        assert!(scopes.body.is_target_active());
        assert!(!scopes.body.is_shadowed());
        assert!(scopes.binding_body.is_target_active());
        assert!(!scopes.binding_body.is_shadowed());
    }
}
