use anyhow::Result;

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::patterns::{binding_pattern_names, lambda_list_names};

#[derive(Debug, Clone)]
pub(super) struct BindingGroup {
    names: Vec<String>,
    pub(super) value: Option<ExpressionView>,
}

pub(super) fn generic_binding_groups(binding_form: &ExpressionView) -> Result<Vec<BindingGroup>> {
    match binding_form.delimiter {
        Some(Delimiter::Bracket) => vector_let_binding_groups(binding_form),
        Some(Delimiter::Paren) => list_pair_let_binding_groups(binding_form),
        _ => anyhow::bail!("unknown binding form delimiter"),
    }
}

fn vector_let_binding_groups(binding_form: &ExpressionView) -> Result<Vec<BindingGroup>> {
    if binding_form.kind != ExpressionKind::List
        || binding_form.delimiter != Some(Delimiter::Bracket)
    {
        anyhow::bail!("dialect expects vector let bindings: [name value ...]");
    }
    if binding_form.children.len() % 2 != 0 {
        anyhow::bail!("vector let binding form must contain name/value pairs");
    }

    binding_form
        .children
        .chunks_exact(2)
        .map(|pair| {
            let names = binding_pattern_names(&pair[0]);
            if names.is_empty() {
                anyhow::bail!("let binding pattern must contain at least one binding name");
            }
            Ok(BindingGroup {
                names,
                value: Some(pair[1].clone()),
            })
        })
        .collect()
}

fn list_pair_let_binding_groups(binding_form: &ExpressionView) -> Result<Vec<BindingGroup>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        anyhow::bail!("dialect expects list-pair let bindings: ((name value) ...)");
    }

    binding_form
        .children
        .iter()
        .map(|pair| {
            if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren) {
                if pair.kind != ExpressionKind::Atom {
                    anyhow::bail!("let binding must be a name, (name), or (name value)");
                }
                let names = binding_pattern_names(pair);
                if names.len() != 1 {
                    anyhow::bail!("bare let binding must contain one binding name");
                }
                return Ok(BindingGroup { names, value: None });
            }
            if pair.children.is_empty() || pair.children.len() > 2 {
                anyhow::bail!("let binding pair must be (name) or (name value)");
            }
            let names = binding_pattern_names(&pair.children[0]);
            if names.is_empty() {
                anyhow::bail!("let binding pattern must contain at least one binding name");
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
