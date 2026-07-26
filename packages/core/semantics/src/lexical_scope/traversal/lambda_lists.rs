use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SymbolName};

use super::body::collect_body_forms;
use super::{collect_unshadowed_symbol_references_in_context, symbol_name_matches};
use crate::domain::lexical_scope::patterns::binding_pattern_names;

#[derive(Clone, Copy, Eq, PartialEq)]
enum LambdaListMode {
    Required,
    Optional,
    Key,
    Aux,
}

pub(super) fn collect_lambda_list_references(
    dialect: Dialect,
    parameter_form: &ExpressionView,
    body_forms: &[ExpressionView],
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) -> bool {
    if parameter_form.kind != ExpressionKind::List {
        return false;
    }

    if matches!(
        dialect,
        Dialect::Scheme
            | Dialect::Racket
            | Dialect::Lfe
            | Dialect::Clojure
            | Dialect::Hy
            | Dialect::Carp
            | Dialect::Janet
            | Dialect::Fennel
    ) {
        return collect_simple_parameter_list_references(
            dialect,
            parameter_form,
            body_forms,
            symbol,
            input,
            output,
        );
    }

    let mut mode = LambdaListMode::Required;
    let mut index = 0usize;

    while index < parameter_form.children.len() {
        let child = &parameter_form.children[index];

        if let Some(next_index) =
            collect_lambda_list_marker(parameter_form, child, symbol, &mut mode, index)
        {
            if next_index == usize::MAX {
                return true;
            }
            index = next_index;
            continue;
        }

        collect_lambda_list_spec_references(dialect, child, mode, symbol, input, output);

        if lambda_list_binding_names(child, mode)
            .iter()
            .any(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()))
        {
            return true;
        }

        index += 1;
    }

    collect_body_forms(dialect, body_forms, symbol, input, output);
    true
}

fn collect_simple_parameter_list_references(
    dialect: Dialect,
    parameter_form: &ExpressionView,
    body_forms: &[ExpressionView],
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) -> bool {
    let is_shadowed = parameter_form.children.iter().any(|parameter| {
        lambda_list_binding_names(parameter, LambdaListMode::Required)
            .iter()
            .any(|name| symbol_name_matches(dialect, name, symbol.as_str()))
    });

    if !is_shadowed {
        collect_body_forms(dialect, body_forms, symbol, input, output);
    }
    true
}

fn collect_lambda_list_marker(
    parameter_form: &ExpressionView,
    child: &ExpressionView,
    symbol: &SymbolName,
    mode: &mut LambdaListMode,
    index: usize,
) -> Option<usize> {
    let marker = super::super::syntax::atom_text(child)?;

    match marker {
        "&optional" => {
            *mode = LambdaListMode::Optional;
            Some(index + 1)
        }
        "&key" => {
            *mode = LambdaListMode::Key;
            Some(index + 1)
        }
        "&aux" => {
            *mode = LambdaListMode::Aux;
            Some(index + 1)
        }
        "&rest" | "&body" | "&whole" | "&environment" => {
            let shadowed = parameter_form
                .children
                .get(index + 1)
                .into_iter()
                .flat_map(|next| lambda_list_binding_names(next, *mode))
                .any(|name| common_lisp_symbol_reference_eq(&name, symbol.as_str()));
            Some(if shadowed { usize::MAX } else { index + 2 })
        }
        "&allow-other-keys" => Some(index + 1),
        _ if marker.starts_with('&') => Some(index + 1),
        _ => None,
    }
}

fn collect_lambda_list_spec_references(
    dialect: Dialect,
    spec: &ExpressionView,
    mode: LambdaListMode,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    match mode {
        LambdaListMode::Required => {}
        LambdaListMode::Optional | LambdaListMode::Key | LambdaListMode::Aux => {
            if let Some(default_form) = spec.children.get(1) {
                collect_unshadowed_symbol_references_in_context(
                    dialect,
                    default_form,
                    symbol,
                    input,
                    output,
                    0,
                );
            }
        }
    }
}

fn lambda_list_binding_names(spec: &ExpressionView, mode: LambdaListMode) -> Vec<String> {
    match mode {
        LambdaListMode::Required => binding_pattern_names(spec),
        LambdaListMode::Optional | LambdaListMode::Aux => leading_binding_pattern_names(spec),
        LambdaListMode::Key => key_binding_pattern_names(spec),
    }
}

fn leading_binding_pattern_names(spec: &ExpressionView) -> Vec<String> {
    if spec.kind == ExpressionKind::List {
        if let Some(binding) = spec.children.first() {
            return binding_pattern_names(binding);
        }
    }

    binding_pattern_names(spec)
}

fn key_binding_pattern_names(spec: &ExpressionView) -> Vec<String> {
    if spec.kind == ExpressionKind::List && !spec.children.is_empty() {
        if let Some(designator) = super::super::syntax::atom_text(&spec.children[0]) {
            if designator.starts_with(':') && spec.children.len() >= 2 {
                return binding_pattern_names(&spec.children[1]);
            }
        }
    }

    leading_binding_pattern_names(spec)
}
