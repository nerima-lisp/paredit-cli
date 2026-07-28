use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::scheme::{scheme_formal_defaults_in, scheme_formals_in};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SymbolName};

use super::body::collect_body_forms;
use super::{collect_unshadowed_symbol_references_in_context, symbol_name_matches};
use crate::lexical_scope::patterns::binding_pattern_names;

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
    collect_lambda_list_references_from(
        dialect,
        parameter_form,
        0,
        body_forms,
        symbol,
        input,
        output,
    )
}

/// As [`collect_lambda_list_references`], but skipping a leading prefix of the
/// parameter node that holds something other than parameters.
///
/// Scheme's `(define (f x) body)` has lambda list `(f x)`: reading it from 0
/// treats the procedure's own name as a parameter, which makes every recursive
/// call in the body look shadowed and silently drops it from a rename.
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_lambda_list_references_from(
    dialect: Dialect,
    parameter_form: &ExpressionView,
    first_parameter_index: usize,
    body_forms: &[ExpressionView],
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) -> bool {
    if parameter_form.kind != ExpressionKind::List {
        return false;
    }

    let parameters = parameter_form
        .children
        .get(first_parameter_index.min(parameter_form.children.len())..)
        .unwrap_or_default();

    if matches!(dialect, Dialect::Scheme | Dialect::Racket) {
        return collect_scheme_formals_references(
            dialect, parameters, body_forms, symbol, input, output,
        );
    }

    if matches!(
        dialect,
        Dialect::Lfe
            | Dialect::Clojure
            | Dialect::Hy
            | Dialect::Carp
            | Dialect::Janet
            | Dialect::Fennel
    ) {
        return collect_simple_parameter_list_references(
            dialect, parameters, body_forms, symbol, input, output,
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
    parameters: &[ExpressionView],
    body_forms: &[ExpressionView],
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) -> bool {
    let is_shadowed = parameters.iter().any(|parameter| {
        lambda_list_binding_names(parameter, LambdaListMode::Required)
            .iter()
            .any(|name| symbol_name_matches(dialect, name, symbol.as_str()))
    });

    if !is_shadowed {
        collect_body_forms(dialect, body_forms, symbol, input, output);
    }
    true
}

/// Scheme formals, read with Scheme's own rules.
///
/// The flat scan above gets three things wrong here. The `.` of `(a . rest)`
/// is not a parameter; the `rest` after it is. Racket's `[x default]` binds
/// only `x`, and the default is an ordinary expression in the *enclosing*
/// scope that a reference query must still walk. And `#:mode mode` binds
/// `mode`, not the keyword token.
fn collect_scheme_formals_references(
    dialect: Dialect,
    parameters: &[ExpressionView],
    body_forms: &[ExpressionView],
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) -> bool {
    // Walked before the shadowing test, because a default-value expression is
    // evaluated outside the scope the parameters open.
    for default_form in scheme_formal_defaults_in(parameters) {
        collect_unshadowed_symbol_references_in_context(
            dialect,
            default_form,
            symbol,
            input,
            output,
            0,
        );
    }

    let is_shadowed = scheme_formals_in(parameters)
        .iter()
        .any(|formal| symbol_name_matches(dialect, &formal.name, symbol.as_str()));

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
