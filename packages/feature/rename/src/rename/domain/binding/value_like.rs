use anyhow::{Context, Result};

use crate::domain::common_lisp::{
    CommonLispLetBindingForm, CommonLispVariableBindingForm, common_lisp_symbol_reference_eq,
};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Delimiter, ExpressionView, SymbolName};

use super::build_binding_rename_parts;
use super::collect_symbol_atom_spans_unshadowed;
use super::common_lisp;
use super::forms::{binding_groups, parameter_name_spans};
use super::types::{BindingEdit, BindingRenameParts};

pub(super) fn let_binding_rename_parts(
    dialect: Dialect,
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    let_form: CommonLispLetBindingForm,
    input: &str,
) -> Result<BindingRenameParts> {
    let binding_form = view
        .children
        .get(1)
        .context("selected let form must contain bindings")?;
    let bindings = binding_groups(dialect, binding_form, input)?;
    if let_form == CommonLispLetBindingForm::SymbolMacro
        && bindings.iter().any(|binding| binding.value.is_none())
    {
        anyhow::bail!("symbol-macrolet binding must contain a symbol and expansion");
    }
    let (target_index, target) = bindings
        .iter()
        .enumerate()
        .find_map(|(index, binding)| {
            binding
                .names
                .iter()
                .find(|name| common_lisp_symbol_reference_eq(&name.name, from.as_str()))
                .map(|name| (index, name))
        })
        .ok_or_else(|| anyhow::anyhow!("binding '{from}' was not found in selected let"))?;

    let sequential_scope =
        let_form.is_sequential() || binding_form.delimiter == Some(Delimiter::Bracket);
    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
    if sequential_scope {
        for later in bindings.iter().skip(target_index + 1) {
            if let Some(value) = &later.value {
                collect_symbol_atom_spans_unshadowed(
                    value,
                    from,
                    &mut reference_spans,
                    &mut shadowed_scope_count,
                    input,
                );
            }
        }
    }

    for body in &view.children[2..] {
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

pub(super) fn value_binding_rename_parts(
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    binding_index: usize,
    body_start_index: usize,
    input: &str,
) -> Result<BindingRenameParts> {
    let binding_form = view
        .children
        .get(binding_index)
        .with_context(|| format!("selected {form} form must contain bindings"))?;
    let bindings = parameter_name_spans(binding_form, input)?;
    let target = bindings
        .iter()
        .find(|binding| common_lisp_symbol_reference_eq(&binding.name, from.as_str()))
        .ok_or_else(|| anyhow::anyhow!("binding '{from}' was not found in selected {form}"))?;

    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
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

pub(super) fn iteration_binding_rename_parts(
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    input: &str,
) -> Result<BindingRenameParts> {
    let binding_form = view
        .children
        .get(1)
        .with_context(|| format!("selected {form} form must contain an iteration binding"))?;
    let target = binding_form
        .children
        .first()
        .filter(|child| {
            super::super::selection::atom_text(child)
                .is_some_and(|name| common_lisp_symbol_reference_eq(name, from.as_str()))
        })
        .ok_or_else(|| anyhow::anyhow!("binding '{from}' was not found in selected {form}"))?;

    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
    if let Some(result_form) = binding_form.children.get(2) {
        collect_symbol_atom_spans_unshadowed(
            result_form,
            from,
            &mut reference_spans,
            &mut shadowed_scope_count,
            input,
        );
    }
    for body in &view.children[2..] {
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
        target.span,
        BindingEdit::rename_atom(target.span),
        reference_spans,
        shadowed_scope_count,
    ))
}

pub(super) fn common_lisp_variable_binding_rename_parts(
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    variable_form: CommonLispVariableBindingForm,
    has_step_forms: bool,
    input: &str,
) -> Result<BindingRenameParts> {
    let binding_form = view
        .children
        .get(1)
        .with_context(|| format!("selected {form} form must contain variable specs"))?;
    let mut target = None;
    let mut duplicate_count = 0usize;

    for (index, spec) in binding_form.children.iter().enumerate() {
        let Some((name, span)) = common_lisp::variable_spec_binding_name(spec) else {
            continue;
        };
        if !common_lisp_symbol_reference_eq(name, from.as_str()) {
            continue;
        }
        duplicate_count += 1;
        target = Some((index, span));
    }

    if duplicate_count > 1 {
        anyhow::bail!(
            "binding '{from}' was found in multiple selected {form} specs; select an unambiguous binding form"
        );
    }

    let (target_index, binding_span) = target
        .ok_or_else(|| anyhow::anyhow!("binding '{from}' was not found in selected {form}"))?;

    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
    if variable_form.is_sequential() {
        for spec in binding_form.children.iter().skip(target_index + 1) {
            if let Some(init_form) = common_lisp::variable_spec_init_form(spec) {
                collect_symbol_atom_spans_unshadowed(
                    init_form,
                    from,
                    &mut reference_spans,
                    &mut shadowed_scope_count,
                    input,
                );
            }
        }
    }

    if has_step_forms {
        for spec in &binding_form.children {
            if let Some(step_form) = common_lisp::do_variable_spec_step_form(spec) {
                collect_symbol_atom_spans_unshadowed(
                    step_form,
                    from,
                    &mut reference_spans,
                    &mut shadowed_scope_count,
                    input,
                );
            }
        }
    }

    for body in &view.children[2..] {
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
        BindingEdit::rename_atom(binding_span),
        reference_spans,
        shadowed_scope_count,
    ))
}
