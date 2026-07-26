use crate::domain::common_lisp::{
    CommonLispLocalCallableForm, common_lisp_symbol_reference_eq, local_callable_names,
};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::super::body::collect_body_forms;
use super::super::lambda_lists::collect_lambda_list_references;

pub(super) fn collect_local_callable_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    form: CommonLispLocalCallableForm,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };

    let local_names = local_callable_names(view);
    let body_is_shadowed = matches!(form, CommonLispLocalCallableForm::Labels)
        && local_names
            .iter()
            .any(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()));

    for spec in &binding_form.children {
        if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
            continue;
        }

        let Some(parameter_form) = spec.children.get(1) else {
            continue;
        };

        let spec_body_forms: &[ExpressionView] = if body_is_shadowed {
            &[]
        } else {
            &spec.children[2..]
        };

        collect_lambda_list_references(
            dialect,
            parameter_form,
            spec_body_forms,
            symbol,
            input,
            output,
        );
    }

    if local_names
        .iter()
        .any(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()))
    {
        return;
    }

    collect_body_forms(dialect, &view.children[2..], symbol, input, output);
}
