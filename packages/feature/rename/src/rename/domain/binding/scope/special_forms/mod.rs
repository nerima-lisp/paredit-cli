mod callable;
mod let_like;
mod loop_scope;
mod slots_clause;

use crate::domain::common_lisp::{
    CommonLispBindingRefactorForm, CommonLispBindingReferenceScope, CommonLispLetBindingForm,
    common_lisp_binding_refactor_form_for_head,
};
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SymbolName};

use super::super::super::selection::atom_text;
use callable::{
    collect_defmethod_references, collect_handler_bind_references,
    collect_local_callable_references,
};
use let_like::{
    collect_iteration_binding_references, collect_parallel_let_references,
    collect_sequential_let_references, collect_value_binding_references,
    collect_variable_spec_binding_references,
};
use loop_scope::collect_loop_references;
use slots_clause::{collect_clause_form_references, collect_slot_binding_references};

pub(super) fn collect_shadow_aware_special_form(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) -> bool {
    if view.kind != ExpressionKind::List || view.children.len() < 2 {
        return false;
    }

    let Some(head) = atom_text(&view.children[0]) else {
        return false;
    };

    let Some(refactor_form) = common_lisp_binding_refactor_form_for_head(head) else {
        return false;
    };

    if let Some(reference_scope) = refactor_form.reference_scope() {
        collect_binding_reference_scope(
            view,
            symbol,
            output,
            shadowed_scope_count,
            input,
            reference_scope,
        );
        return true;
    }

    match refactor_form {
        CommonLispBindingRefactorForm::Value => {
            collect_value_binding_references(view, symbol, output, shadowed_scope_count, input);
            true
        }
        CommonLispBindingRefactorForm::LambdaLike => {
            if super::super::forms::parameter_form_binds(&view.children[1], symbol, input) {
                *shadowed_scope_count += 1;
                return true;
            }
            false
        }
        CommonLispBindingRefactorForm::MethodDefinition => {
            collect_defmethod_references(view, symbol, output, shadowed_scope_count, input);
            true
        }
        CommonLispBindingRefactorForm::FunctionDefinition => {
            if view.children.len() > 2
                && super::super::forms::parameter_form_binds(&view.children[2], symbol, input)
            {
                *shadowed_scope_count += 1;
            }
            true
        }
        CommonLispBindingRefactorForm::Clause => {
            collect_clause_form_references(view, symbol, output, shadowed_scope_count, input);
            true
        }
        CommonLispBindingRefactorForm::Handler(handler_form) => {
            collect_handler_bind_references(
                view,
                symbol,
                output,
                shadowed_scope_count,
                input,
                handler_form.includes_restart_options(),
            );
            true
        }
        CommonLispBindingRefactorForm::Iteration => {
            collect_iteration_binding_references(view, symbol, output, shadowed_scope_count, input);
            true
        }
        CommonLispBindingRefactorForm::Loop => {
            collect_loop_references(view, symbol, output, shadowed_scope_count, input);
            true
        }
        CommonLispBindingRefactorForm::Let(_)
        | CommonLispBindingRefactorForm::LocalCallable(_)
        | CommonLispBindingRefactorForm::Do(_)
        | CommonLispBindingRefactorForm::Prog(_)
        | CommonLispBindingRefactorForm::Slot(_) => {
            debug_assert!(
                false,
                "forms with binding reference scopes must be handled before special-form dispatch"
            );
            false
        }
    }
}

fn collect_binding_reference_scope(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
    reference_scope: CommonLispBindingReferenceScope,
) {
    match reference_scope {
        CommonLispBindingReferenceScope::NameValuePairs(CommonLispLetBindingForm::Sequential) => {
            collect_sequential_let_references(view, symbol, output, shadowed_scope_count, input);
        }
        CommonLispBindingReferenceScope::NameValuePairs(
            CommonLispLetBindingForm::Parallel | CommonLispLetBindingForm::SymbolMacro,
        ) => {
            collect_parallel_let_references(view, symbol, output, shadowed_scope_count, input);
        }
        CommonLispBindingReferenceScope::LocalCallableDefinitions(_) => {
            collect_local_callable_references(view, symbol, output, shadowed_scope_count, input);
        }
        CommonLispBindingReferenceScope::VariableSpecs(spec_form, binding_form) => {
            collect_variable_spec_binding_references(
                view,
                symbol,
                output,
                shadowed_scope_count,
                input,
                spec_form,
                binding_form.is_sequential(),
            );
        }
        CommonLispBindingReferenceScope::BodyOnly => {
            collect_slot_binding_references(view, symbol, output, shadowed_scope_count, input);
        }
    }
}
