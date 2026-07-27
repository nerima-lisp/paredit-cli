mod common_lisp;
mod destructure;
mod forms;
mod lambda_like;
mod rewrite;
mod scope;
mod scoped;
mod semantic;
mod types;
mod value_like;

use anyhow::{Context, Result};

use crate::domain::common_lisp::CommonLispBindingRefactorForm;
use crate::domain::dialect::{RenameBindingOperation, VerifiedSemanticPolicy};
use crate::domain::sexpr::{ByteSpan, ExpressionView, SymbolName};

use lambda_like::{
    defmethod_binding_rename_parts, handler_bind_lambda_binding_rename_parts,
    local_callable_lambda_binding_rename_parts, parameter_binding_rename_parts,
};
use scoped::{clause_binding_rename_parts, loop_binding_rename_parts, slot_binding_rename_parts};
use types::BindingEdit;
use value_like::{
    common_lisp_variable_binding_rename_parts, iteration_binding_rename_parts,
    let_binding_rename_parts, value_binding_rename_parts,
};

pub(in crate::domain::rename) use forms::parameter_form_binds;
pub(in crate::domain::rename) use lambda_like::collect_enclosing_lambda_list_references;
pub(super) use scope::collect_symbol_atom_spans_unshadowed;
pub(super) use scope::collect_symbol_atom_spans_unshadowed_ignoring_declared_specials;
pub(super) use types::BindingRenameParts;

pub(super) fn collect_shadow_aware_special_form(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) -> bool {
    scope::collect_shadow_aware_special_form(view, symbol, output, shadowed_scope_count, input)
}

pub(super) fn binding_rename_parts(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    from: &SymbolName,
    input: &str,
) -> Result<BindingRenameParts> {
    let dialect = semantic.dialect();
    let form = super::selection::list_head(view)
        .context("selected form is not a supported binding form")?
        .to_owned();

    if dialect != crate::domain::dialect::Dialect::CommonLisp
        && (semantic.scope_shape(view).is_some() || semantic.definition_shape(view).is_some())
    {
        return semantic::semantic_binding_rename_parts(semantic, view, from, form, input);
    }

    let Some(refactor_form) = dialect.common_lisp_binding_refactor_form_for_head(&form) else {
        anyhow::bail!("selected form is not a supported binding form");
    };

    match refactor_form {
        CommonLispBindingRefactorForm::Let(let_form) => {
            let_binding_rename_parts(dialect, view, from, form, let_form, input)
        }
        CommonLispBindingRefactorForm::Value => {
            value_binding_rename_parts(view, from, form, 1, 3, input)
        }
        CommonLispBindingRefactorForm::LambdaLike => {
            parameter_binding_rename_parts(view, from, form, 1, 2, input)
        }
        CommonLispBindingRefactorForm::MethodDefinition => {
            defmethod_binding_rename_parts(dialect, view, from, form, input)
        }
        CommonLispBindingRefactorForm::FunctionDefinition => {
            parameter_binding_rename_parts(view, from, form, 2, 3, input)
        }
        CommonLispBindingRefactorForm::LocalCallable(_) => {
            local_callable_lambda_binding_rename_parts(view, from, form, input)
        }
        CommonLispBindingRefactorForm::Clause => {
            clause_binding_rename_parts(view, from, form, input)
        }
        CommonLispBindingRefactorForm::Handler(handler_form) => {
            handler_bind_lambda_binding_rename_parts(view, from, form, handler_form, input)
        }
        CommonLispBindingRefactorForm::Iteration => {
            iteration_binding_rename_parts(view, from, form, input)
        }
        CommonLispBindingRefactorForm::Loop => loop_binding_rename_parts(view, from, form, input),
        CommonLispBindingRefactorForm::Do(variable_form) => {
            common_lisp_variable_binding_rename_parts(view, from, form, variable_form, true, input)
        }
        CommonLispBindingRefactorForm::Prog(variable_form) => {
            common_lisp_variable_binding_rename_parts(view, from, form, variable_form, false, input)
        }
        CommonLispBindingRefactorForm::Slot(_) => {
            slot_binding_rename_parts(view, from, form, input)
        }
    }
}

fn build_binding_rename_parts(
    form: String,
    form_span: ByteSpan,
    binding_span: ByteSpan,
    binding_edit: BindingEdit,
    mut reference_spans: Vec<ByteSpan>,
    shadowed_scope_count: usize,
) -> BindingRenameParts {
    reference_spans.sort_by_key(|span| span.start());
    BindingRenameParts {
        form,
        form_span,
        binding_span,
        binding_edit,
        reference_spans,
        shadowed_scope_count,
    }
}
