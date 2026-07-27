use anyhow::Result;

use crate::domain::common_lisp::CommonLispHandlerBindingForm;
use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::super::selection::atom_text;
use super::destructure::binding_pattern_name_spans;
use super::forms::parameter_name_spans;
use super::types::{BindingEdit, ParameterNameSpan};

#[derive(Clone)]
pub(super) struct LoopBindingSpec {
    pub(super) name: String,
    pub(super) name_span: ByteSpan,
    pub(super) binding_edit: BindingEdit,
    pub(super) reference_start_index: usize,
}

pub(super) fn loop_binding_specs(view: &ExpressionView, input: &str) -> Vec<LoopBindingSpec> {
    let mut specs = Vec::new();
    let mut index = 1usize;

    while index < view.children.len() {
        let child = &view.children[index];
        if loop_keyword_is(child, "for") || loop_keyword_is(child, "as") {
            if let Some(name_form) = view.children.get(index + 1) {
                let reference_start_index =
                    loop_for_reference_start_index(&view.children, index + 2);
                push_loop_binding_specs(&mut specs, name_form, reference_start_index, input);
            }
            index += 2;
            continue;
        }

        if loop_keyword_is(child, "with") {
            if let Some(name_form) = view.children.get(index + 1) {
                let reference_start_index =
                    loop_with_reference_start_index(&view.children, index + 2);
                push_loop_binding_specs(&mut specs, name_form, reference_start_index, input);
            }
            index += 2;
            continue;
        }

        index += 1;
    }

    specs
}

pub(super) fn variable_spec_binding_name(spec: &ExpressionView) -> Option<(&str, ByteSpan)> {
    match &spec.kind {
        ExpressionKind::Atom => Some((atom_text(spec)?, spec.span)),
        ExpressionKind::List => {
            let first = spec.children.first()?;
            Some((atom_text(first)?, first.span))
        }
        ExpressionKind::Root => None,
    }
}

pub(super) fn variable_spec_init_form(spec: &ExpressionView) -> Option<&ExpressionView> {
    (spec.kind == ExpressionKind::List)
        .then(|| spec.children.get(1))
        .flatten()
}

pub(super) fn do_variable_spec_step_form(spec: &ExpressionView) -> Option<&ExpressionView> {
    (spec.kind == ExpressionKind::List)
        .then(|| spec.children.get(2))
        .flatten()
}

pub(super) fn handler_bind_function_forms(
    view: &ExpressionView,
    handler_form: CommonLispHandlerBindingForm,
) -> Vec<&ExpressionView> {
    let Some(binding_form) = view.children.get(1) else {
        return Vec::new();
    };

    let mut forms = Vec::new();
    for spec in &binding_form.children {
        if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
            continue;
        }

        if let Some(function_form) = spec.children.get(1) {
            forms.push(function_form);
        }

        if handler_form.includes_restart_options() {
            let mut index = 2usize;
            while index + 1 < spec.children.len() {
                forms.push(&spec.children[index + 1]);
                index += 2;
            }
        }
    }

    forms
}

pub(super) fn collect_lambda_binding_targets<'a>(
    view: &'a ExpressionView,
    from: &SymbolName,
    input: &str,
    target: &mut Option<(&'a ExpressionView, ParameterNameSpan)>,
    duplicate_count: &mut usize,
) -> Result<()> {
    if view.kind == ExpressionKind::List
        && view.delimiter == Some(Delimiter::Paren)
        && atom_text(view.children.first().unwrap_or(view)) == Some("lambda")
    {
        if let Some(parameter_form) = view.children.get(1) {
            let parameters = parameter_name_spans(parameter_form, input)?;
            if let Some(parameter) = parameters
                .iter()
                .find(|parameter| common_lisp_symbol_reference_eq(&parameter.name, from.as_str()))
            {
                *duplicate_count += 1;
                *target = Some((view, parameter.clone()));
            }
        }
    }

    for child in &view.children {
        collect_lambda_binding_targets(child, from, input, target, duplicate_count)?;
    }

    Ok(())
}

pub(super) fn slot_spec_binding_name(
    spec: &ExpressionView,
) -> Option<(&str, ByteSpan, BindingEdit)> {
    match &spec.kind {
        ExpressionKind::Atom => {
            let name = atom_text(spec)?;
            Some((
                name,
                spec.span,
                BindingEdit::bare_slot_spec(spec.span, name.to_owned()),
            ))
        }
        ExpressionKind::List => {
            let first = spec.children.first()?;
            let name = atom_text(first)?;
            Some((name, first.span, BindingEdit::rename_atom(first.span)))
        }
        ExpressionKind::Root => None,
    }
}

fn push_loop_binding_specs(
    specs: &mut Vec<LoopBindingSpec>,
    name_form: &ExpressionView,
    reference_start_index: usize,
    input: &str,
) {
    specs.extend(
        binding_pattern_name_spans(name_form, input)
            .into_iter()
            .map(|name| LoopBindingSpec {
                name: name.name,
                name_span: name.name_span,
                binding_edit: name.binding_edit,
                reference_start_index,
            }),
    );
}

pub(super) fn loop_for_reference_start_index(
    children: &[ExpressionView],
    mut index: usize,
) -> usize {
    let Some(keyword) = children.get(index).and_then(atom_text) else {
        return index;
    };

    if matches_loop_keyword(keyword, &["in", "on", "across"]) {
        return (index + 2).min(children.len());
    }

    if matches_loop_keyword(keyword, &["=", "from", "downfrom", "upfrom"]) {
        index = (index + 2).min(children.len());
        while children.get(index).and_then(atom_text).is_some_and(|text| {
            matches_loop_keyword(text, &["to", "upto", "downto", "below", "above", "by"])
        }) {
            index = (index + 2).min(children.len());
        }
    }

    index
}

pub(super) fn loop_with_reference_start_index(children: &[ExpressionView], index: usize) -> usize {
    if children
        .get(index)
        .is_some_and(|child| loop_keyword_is(child, "="))
    {
        return (index + 2).min(children.len());
    }

    index
}

pub(super) fn loop_keyword_is(view: &ExpressionView, keyword: &str) -> bool {
    atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(keyword))
}

pub(super) fn loop_syntax_atom(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| {
        matches_loop_keyword(
            text,
            &[
                "=", "in", "on", "across", "from", "downfrom", "upfrom", "to", "upto", "downto",
                "below", "above", "by",
            ],
        )
    })
}

fn matches_loop_keyword(text: &str, keywords: &[&str]) -> bool {
    keywords
        .iter()
        .any(|keyword| text.eq_ignore_ascii_case(keyword))
}
