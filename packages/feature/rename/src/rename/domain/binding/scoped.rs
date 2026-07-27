use anyhow::{Context, Result};

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::build_binding_rename_parts;
use super::collect_symbol_atom_spans_unshadowed;
use super::common_lisp;
use super::forms::parameter_name_spans;
use super::types::BindingRenameParts;

pub(super) fn clause_binding_rename_parts(
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    input: &str,
) -> Result<BindingRenameParts> {
    let mut target = None;
    let mut duplicate_count = 0usize;

    for clause in &view.children[2..] {
        if clause.kind != ExpressionKind::List || clause.delimiter != Some(Delimiter::Paren) {
            continue;
        }

        let Some(parameter_form) = clause.children.get(1) else {
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
        target = Some((clause, parameter.clone()));
    }

    if duplicate_count > 1 {
        anyhow::bail!(
            "binding '{from}' was found in multiple selected {form} clauses; select an unambiguous binding form"
        );
    }

    let (target_clause, target_parameter) = target
        .ok_or_else(|| anyhow::anyhow!("binding '{from}' was not found in selected {form}"))?;

    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
    for body in &target_clause.children[2..] {
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

pub(super) fn loop_binding_rename_parts(
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    input: &str,
) -> Result<BindingRenameParts> {
    let mut target = None;
    let mut duplicate_count = 0usize;

    for spec in common_lisp::loop_binding_specs(view, input) {
        if !common_lisp_symbol_reference_eq(&spec.name, from.as_str()) {
            continue;
        }
        duplicate_count += 1;
        target = Some(spec.clone());
    }

    if duplicate_count > 1 {
        anyhow::bail!(
            "binding '{from}' was found in multiple selected {form} clauses; select an unambiguous binding form"
        );
    }

    let target = target
        .ok_or_else(|| anyhow::anyhow!("binding '{from}' was not found in selected {form}"))?;

    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
    for body in &view.children[target.reference_start_index..] {
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
        target.binding_edit,
        reference_spans,
        shadowed_scope_count,
    ))
}

pub(super) fn slot_binding_rename_parts(
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    input: &str,
) -> Result<BindingRenameParts> {
    let slot_specs = view
        .children
        .get(1)
        .with_context(|| format!("selected {form} form must contain slot specs"))?;
    let mut target = None;
    let mut duplicate_count = 0usize;

    for spec in &slot_specs.children {
        let Some((name, span, edit)) = common_lisp::slot_spec_binding_name(spec) else {
            continue;
        };
        if !common_lisp_symbol_reference_eq(name, from.as_str()) {
            continue;
        }
        duplicate_count += 1;
        target = Some((span, edit));
    }

    if duplicate_count > 1 {
        anyhow::bail!(
            "binding '{from}' was found in multiple selected {form} specs; select an unambiguous binding form"
        );
    }

    let (binding_span, binding_edit) = target
        .ok_or_else(|| anyhow::anyhow!("binding '{from}' was not found in selected {form}"))?;

    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
    for body in &view.children[3..] {
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
        binding_span,
        binding_edit,
        reference_spans,
        shadowed_scope_count,
    ))
}
