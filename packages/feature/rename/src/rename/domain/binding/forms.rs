use crate::error::{BindingListError, RenameResult};

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::destructure::{
    binding_pattern_name_spans, lambda_list_name_spans, specialized_lambda_list_name_spans,
};
use super::types::{BindingGroup, ParameterNameSpan};

pub fn binding_groups(
    dialect: Dialect,
    binding_form: &ExpressionView,
    input: &str,
) -> RenameResult<Vec<BindingGroup>> {
    match dialect {
        Dialect::Clojure | Dialect::Hy | Dialect::Carp | Dialect::Janet | Dialect::Fennel => {
            vector_let_binding_groups(binding_form, input)
        }
        Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Unknown => list_pair_let_binding_groups(binding_form, input),
    }
}

pub fn generic_binding_groups(
    binding_form: &ExpressionView,
    input: &str,
) -> RenameResult<Vec<BindingGroup>> {
    match binding_form.delimiter {
        Some(Delimiter::Bracket) => vector_let_binding_groups(binding_form, input),
        Some(Delimiter::Paren) => list_pair_let_binding_groups(binding_form, input),
        _ => Err(BindingListError::UnknownDelimiter.into()),
    }
}

pub fn parameter_name_spans(
    parameter_form: &ExpressionView,
    input: &str,
) -> RenameResult<Vec<ParameterNameSpan>> {
    if parameter_form.kind != ExpressionKind::List {
        return Err(BindingListError::ParameterFormNotAList.into());
    }

    Ok(lambda_list_name_spans(parameter_form, input))
}

pub fn parameter_form_binds(
    parameter_form: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
) -> bool {
    parameter_form.kind == ExpressionKind::List
        && lambda_list_name_spans(parameter_form, input)
            .iter()
            .any(|name| common_lisp_symbol_reference_eq(&name.name, symbol.as_str()))
}

pub fn specialized_parameter_name_spans(
    parameter_form: &ExpressionView,
    input: &str,
) -> RenameResult<Vec<ParameterNameSpan>> {
    if parameter_form.kind != ExpressionKind::List {
        return Err(BindingListError::SpecializedParameterFormNotAList.into());
    }

    Ok(specialized_lambda_list_name_spans(parameter_form, input))
}

pub fn binding_binds(binding: &BindingGroup, symbol: &SymbolName) -> bool {
    binding
        .names
        .iter()
        .any(|name| common_lisp_symbol_reference_eq(&name.name, symbol.as_str()))
}

fn vector_let_binding_groups(
    binding_form: &ExpressionView,
    input: &str,
) -> RenameResult<Vec<BindingGroup>> {
    if binding_form.kind != ExpressionKind::List
        || binding_form.delimiter != Some(Delimiter::Bracket)
    {
        return Err(BindingListError::ExpectedVectorLet.into());
    }
    if binding_form.children.len() % 2 != 0 {
        return Err(BindingListError::VectorNotPaired.into());
    }

    binding_form
        .children
        .chunks_exact(2)
        .map(|pair| {
            let names = binding_pattern_name_spans(&pair[0], input);
            if names.is_empty() {
                return Err(BindingListError::PatternBindsNothing.into());
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
    input: &str,
) -> RenameResult<Vec<BindingGroup>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        return Err(BindingListError::ExpectedListPairLet.into());
    }

    binding_form
        .children
        .iter()
        .map(|pair| {
            if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren) {
                if pair.kind != ExpressionKind::Atom {
                    return Err(BindingListError::BindingNotANameOrPair.into());
                }
                let names = binding_pattern_name_spans(pair, input);
                if names.len() != 1 {
                    return Err(BindingListError::BareBindingNotSingle.into());
                }
                return Ok(BindingGroup { names, value: None });
            }
            if pair.children.is_empty() || pair.children.len() > 2 {
                return Err(BindingListError::BindingPairWrongArity.into());
            }
            let names = binding_pattern_name_spans(&pair.children[0], input);
            if names.is_empty() {
                return Err(BindingListError::PatternBindsNothing.into());
            }
            Ok(BindingGroup {
                names,
                value: pair.children.get(1).cloned(),
            })
        })
        .collect()
}
