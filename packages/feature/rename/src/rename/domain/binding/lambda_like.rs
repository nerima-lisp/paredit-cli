use anyhow::{Context, Result};

use crate::domain::common_lisp::{CommonLispHandlerBindingForm, common_lisp_symbol_reference_eq};
use crate::domain::definition::{DefinitionCategory, definition_shape};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::build_binding_rename_parts;
use super::collect_symbol_atom_spans_unshadowed;
use super::common_lisp;
use super::destructure::binding_pattern_name_spans;
use super::forms::{parameter_name_spans, specialized_parameter_name_spans};
use super::types::BindingRenameParts;

pub(super) fn parameter_binding_rename_parts(
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    parameter_index: usize,
    body_start_index: usize,
    input: &str,
) -> Result<BindingRenameParts> {
    let parameter_form = view
        .children
        .get(parameter_index)
        .with_context(|| format!("selected {form} form must contain parameters"))?;
    let parameters = parameter_name_spans(parameter_form, input)?;
    let target = parameters
        .iter()
        .find(|parameter| common_lisp_symbol_reference_eq(&parameter.name, from.as_str()))
        .ok_or_else(|| anyhow::anyhow!("binding '{from}' was not found in selected {form}"))?;

    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
    collect_lambda_list_parameter_references(
        parameter_form,
        from,
        input,
        &mut reference_spans,
        &mut shadowed_scope_count,
    );
    for body in &view.children[body_start_index..] {
        collect_symbol_atom_spans_unshadowed(
            body,
            from,
            &mut reference_spans,
            &mut shadowed_scope_count,
            input,
        );
    }

    Ok(build_binding_rename_parts(
        form,
        view.span,
        target.name_span,
        target.binding_edit.clone(),
        reference_spans,
        shadowed_scope_count,
    ))
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LambdaListMode {
    Required,
    Optional,
    Key,
    Aux,
}

fn collect_lambda_list_parameter_references(
    parameter_form: &ExpressionView,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<crate::domain::sexpr::ByteSpan>,
    shadowed_scope_count: &mut usize,
) {
    if parameter_form.kind != ExpressionKind::List {
        return;
    }

    let mut mode = LambdaListMode::Required;
    let mut index = 0usize;
    let mut pending_binding_only = false;

    while index < parameter_form.children.len() {
        let child = &parameter_form.children[index];

        if let Some(marker) = super::super::selection::atom_text(child) {
            match marker {
                "&optional" => {
                    mode = LambdaListMode::Optional;
                    index += 1;
                    continue;
                }
                "&key" => {
                    mode = LambdaListMode::Key;
                    index += 1;
                    continue;
                }
                "&aux" => {
                    mode = LambdaListMode::Aux;
                    index += 1;
                    continue;
                }
                "&rest" | "&body" | "&whole" | "&environment" => {
                    pending_binding_only = true;
                    index += 1;
                    continue;
                }
                "&allow-other-keys" => {
                    index += 1;
                    continue;
                }
                _ if marker.starts_with('&') => {
                    index += 1;
                    continue;
                }
                _ => {}
            }
        }

        let binds_target = if pending_binding_only {
            pending_binding_only = false;
            binding_pattern_name_spans(child, input)
                .iter()
                .any(|name| common_lisp_symbol_reference_eq(&name.name, from.as_str()))
        } else {
            lambda_list_spec_binds(child, mode, from)
        };

        if !binds_target {
            collect_lambda_list_spec_references(
                child,
                mode,
                from,
                input,
                output,
                shadowed_scope_count,
            );
        }

        index += 1;
    }
}

pub(in crate::domain::rename) fn collect_enclosing_lambda_list_references(
    parameter_form: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<crate::domain::sexpr::ByteSpan>,
    shadowed_scope_count: &mut usize,
) -> bool {
    if parameter_form.kind != ExpressionKind::List {
        return false;
    }

    let mut mode = LambdaListMode::Required;
    let mut index = 0usize;
    let mut pending_binding_only = false;
    let mut shadows_symbol = false;

    while index < parameter_form.children.len() {
        let child = &parameter_form.children[index];

        if let Some(marker) = super::super::selection::atom_text(child) {
            match marker {
                "&optional" => {
                    mode = LambdaListMode::Optional;
                    index += 1;
                    continue;
                }
                "&key" => {
                    mode = LambdaListMode::Key;
                    index += 1;
                    continue;
                }
                "&aux" => {
                    mode = LambdaListMode::Aux;
                    index += 1;
                    continue;
                }
                "&rest" | "&body" | "&whole" | "&environment" => {
                    pending_binding_only = true;
                    index += 1;
                    continue;
                }
                "&allow-other-keys" => {
                    index += 1;
                    continue;
                }
                _ if marker.starts_with('&') => {
                    index += 1;
                    continue;
                }
                _ => {}
            }
        }

        let binds_symbol = if pending_binding_only {
            pending_binding_only = false;
            binding_pattern_name_spans(child, input)
                .iter()
                .any(|name| common_lisp_symbol_reference_eq(&name.name, symbol.as_str()))
        } else {
            lambda_list_spec_binds(child, mode, symbol)
        };

        if !shadows_symbol {
            collect_lambda_list_spec_references(
                child,
                mode,
                symbol,
                input,
                output,
                shadowed_scope_count,
            );
        }
        shadows_symbol |= binds_symbol;
        index += 1;
    }

    if shadows_symbol {
        *shadowed_scope_count += 1;
    }
    shadows_symbol
}

fn collect_lambda_list_spec_references(
    spec: &ExpressionView,
    mode: LambdaListMode,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<crate::domain::sexpr::ByteSpan>,
    shadowed_scope_count: &mut usize,
) {
    match mode {
        LambdaListMode::Required => {}
        LambdaListMode::Optional | LambdaListMode::Key | LambdaListMode::Aux => {
            if let Some(init_form) = common_lisp::variable_spec_init_form(spec) {
                collect_symbol_atom_spans_unshadowed(
                    init_form,
                    symbol,
                    output,
                    shadowed_scope_count,
                    input,
                );
            }
        }
    }
}

fn lambda_list_spec_binds(spec: &ExpressionView, mode: LambdaListMode, from: &SymbolName) -> bool {
    let binds = |pattern: &ExpressionView| {
        binding_pattern_name_spans(pattern, "")
            .iter()
            .any(|name| common_lisp_symbol_reference_eq(&name.name, from.as_str()))
    };

    if spec.kind != ExpressionKind::List {
        return binds(spec);
    }

    match mode {
        LambdaListMode::Required => binds(spec),
        LambdaListMode::Optional => {
            spec.children.first().is_some_and(binds) || spec.children.get(2).is_some_and(binds)
        }
        LambdaListMode::Aux => spec.children.first().is_some_and(binds),
        LambdaListMode::Key => {
            let Some(name_spec) = spec.children.first() else {
                return false;
            };
            let binding = if let Some(designator) = super::super::selection::atom_text(name_spec) {
                if designator.starts_with(':') {
                    spec.children.get(1).unwrap_or(name_spec)
                } else {
                    name_spec
                }
            } else {
                name_spec
            };

            binds(binding) || spec.children.get(2).is_some_and(binds)
        }
    }
}

pub(super) fn defmethod_binding_rename_parts(
    dialect: Dialect,
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    input: &str,
) -> Result<BindingRenameParts> {
    let shape = definition_shape(dialect, view, &form)
        .filter(|shape| shape.category == DefinitionCategory::Method)
        .with_context(|| format!("selected {form} form must be a method definition"))?;
    let parameter_form = shape
        .lambda_list(view)
        .with_context(|| format!("selected {form} form must contain a specialized lambda list"))?;
    let parameters = specialized_parameter_name_spans(parameter_form, input)?;
    let target = parameters
        .iter()
        .find(|parameter| common_lisp_symbol_reference_eq(&parameter.name, from.as_str()))
        .ok_or_else(|| anyhow::anyhow!("binding '{from}' was not found in selected {form}"))?;

    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
    for body in shape.body_forms(view) {
        collect_symbol_atom_spans_unshadowed(
            body,
            from,
            &mut reference_spans,
            &mut shadowed_scope_count,
            input,
        );
    }

    Ok(build_binding_rename_parts(
        form,
        view.span,
        target.name_span,
        target.binding_edit.clone(),
        reference_spans,
        shadowed_scope_count,
    ))
}

pub(super) fn local_callable_lambda_binding_rename_parts(
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    input: &str,
) -> Result<BindingRenameParts> {
    let binding_form = view
        .children
        .get(1)
        .with_context(|| format!("selected {form} form must contain local callable bindings"))?;
    let mut target = None;
    let mut duplicate_count = 0usize;

    for binding in &binding_form.children {
        if binding.kind != ExpressionKind::List || binding.delimiter != Some(Delimiter::Paren) {
            continue;
        }

        let Some(parameter_form) = binding.children.get(1) else {
            continue;
        };
        let parameters = parameter_name_spans(parameter_form, input)?;
        let Some(parameter) = parameters
            .iter()
            .find(|parameter| common_lisp_symbol_reference_eq(&parameter.name, from.as_str()))
        else {
            continue;
        };

        duplicate_count += 1;
        target = Some((binding, parameter.clone()));
    }

    if duplicate_count > 1 {
        anyhow::bail!(
            "binding '{from}' was found in multiple selected {form} local callable lambda lists; select an unambiguous binding form"
        );
    }

    let (target_binding, target_parameter) = target.ok_or_else(|| {
        anyhow::anyhow!(
            "binding '{from}' was not found in selected {form} local callable lambda lists"
        )
    })?;

    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
    if let Some(parameter_form) = target_binding.children.get(1) {
        collect_lambda_list_parameter_references(
            parameter_form,
            from,
            input,
            &mut reference_spans,
            &mut shadowed_scope_count,
        );
    }
    for body in &target_binding.children[2..] {
        collect_symbol_atom_spans_unshadowed(
            body,
            from,
            &mut reference_spans,
            &mut shadowed_scope_count,
            input,
        );
    }

    Ok(build_binding_rename_parts(
        form,
        view.span,
        target_parameter.name_span,
        target_parameter.binding_edit,
        reference_spans,
        shadowed_scope_count,
    ))
}

pub(super) fn handler_bind_lambda_binding_rename_parts(
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    handler_form: CommonLispHandlerBindingForm,
    input: &str,
) -> Result<BindingRenameParts> {
    let mut target = None;
    let mut duplicate_count = 0usize;

    for function_form in common_lisp::handler_bind_function_forms(view, handler_form) {
        common_lisp::collect_lambda_binding_targets(
            function_form,
            from,
            input,
            &mut target,
            &mut duplicate_count,
        )?;
    }

    if duplicate_count > 1 {
        anyhow::bail!(
            "binding '{from}' was found in multiple selected {form} handler functions; select an unambiguous binding form"
        );
    }

    let (target_lambda, target_parameter) = target
        .ok_or_else(|| anyhow::anyhow!("binding '{from}' was not found in selected {form}"))?;

    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
    for body in &target_lambda.children[2..] {
        collect_symbol_atom_spans_unshadowed(
            body,
            from,
            &mut reference_spans,
            &mut shadowed_scope_count,
            input,
        );
    }

    Ok(build_binding_rename_parts(
        form,
        view.span,
        target_parameter.name_span,
        target_parameter.binding_edit,
        reference_spans,
        shadowed_scope_count,
    ))
}
