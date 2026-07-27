use crate::error::{BindingFormError, BindingFormResult};

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::patterns::{binding_pattern_names, lambda_list_names};

#[derive(Debug, Clone)]
pub(super) struct BindingGroup {
    names: Vec<String>,
    pub(super) value: Option<ExpressionView>,
}

pub(super) fn generic_binding_groups(
    binding_form: &ExpressionView,
) -> BindingFormResult<Vec<BindingGroup>> {
    match binding_form.delimiter {
        Some(Delimiter::Bracket) => vector_let_binding_groups(binding_form),
        Some(Delimiter::Paren) => list_pair_let_binding_groups(binding_form),
        _ => Err(BindingFormError::UnknownDelimiter),
    }
}

fn vector_let_binding_groups(
    binding_form: &ExpressionView,
) -> BindingFormResult<Vec<BindingGroup>> {
    if binding_form.kind != ExpressionKind::List
        || binding_form.delimiter != Some(Delimiter::Bracket)
    {
        return Err(BindingFormError::ExpectedVectorBindings);
    }
    if binding_form.children.len() % 2 != 0 {
        return Err(BindingFormError::VectorBindingsNotPaired);
    }

    binding_form
        .children
        .chunks_exact(2)
        .map(|pair| {
            let names = binding_pattern_names(&pair[0]);
            if names.is_empty() {
                return Err(BindingFormError::PatternBindsNothing);
            }
            Ok(BindingGroup {
                names,
                value: Some(pair[1].clone()),
            })
        })
        .collect()
}

fn list_pair_let_binding_groups(
    binding_form: &ExpressionView,
) -> BindingFormResult<Vec<BindingGroup>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        return Err(BindingFormError::ExpectedListPairBindings);
    }

    binding_form
        .children
        .iter()
        .map(|pair| {
            if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren) {
                if pair.kind != ExpressionKind::Atom {
                    return Err(BindingFormError::BindingNotANameOrPair);
                }
                let names = binding_pattern_names(pair);
                if names.len() != 1 {
                    return Err(BindingFormError::BareBindingNotSingle);
                }
                return Ok(BindingGroup { names, value: None });
            }
            if pair.children.is_empty() || pair.children.len() > 2 {
                return Err(BindingFormError::BindingPairWrongArity);
            }
            let names = binding_pattern_names(&pair.children[0]);
            if names.is_empty() {
                return Err(BindingFormError::PatternBindsNothing);
            }
            Ok(BindingGroup {
                names,
                value: pair.children.get(1).cloned(),
            })
        })
        .collect()
}

pub(super) fn parameter_form_binds(parameter_form: &ExpressionView, symbol: &SymbolName) -> bool {
    parameter_form.kind == ExpressionKind::List
        && lambda_list_names(parameter_form)
            .iter()
            .any(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()))
}

pub(super) fn binding_binds(binding: &BindingGroup, symbol: &SymbolName) -> bool {
    binding
        .names
        .iter()
        .any(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()))
}
