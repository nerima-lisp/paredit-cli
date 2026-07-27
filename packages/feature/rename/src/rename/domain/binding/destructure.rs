use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

use super::super::selection::atom_text;
use super::types::{BindingEdit, ParameterNameSpan};

pub(super) fn binding_pattern_name_spans(
    pattern: &ExpressionView,
    input: &str,
) -> Vec<ParameterNameSpan> {
    let mut names = Vec::new();
    let _ = input;
    collect_binding_pattern_name_spans(pattern, &mut names);
    names
}

pub(super) fn lambda_list_name_spans(
    parameter_form: &ExpressionView,
    input: &str,
) -> Vec<ParameterNameSpan> {
    let mut names = Vec::new();
    let _ = input;
    collect_lambda_list_name_spans(parameter_form, &mut names);
    names
}

pub(super) fn specialized_lambda_list_name_spans(
    parameter_form: &ExpressionView,
    input: &str,
) -> Vec<ParameterNameSpan> {
    let mut names = Vec::new();
    let _ = input;
    collect_specialized_lambda_list_name_spans(parameter_form, &mut names);
    names
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LambdaListMode {
    Required,
    Optional,
    Key,
    Aux,
}

#[derive(Clone, Copy)]
enum RequiredParameterStyle {
    Plain,
    Specialized,
}

fn collect_lambda_list_name_spans(
    parameter_form: &ExpressionView,
    output: &mut Vec<ParameterNameSpan>,
) {
    collect_lambda_list_name_spans_with_style(
        parameter_form,
        RequiredParameterStyle::Plain,
        output,
    );
}

fn collect_specialized_lambda_list_name_spans(
    parameter_form: &ExpressionView,
    output: &mut Vec<ParameterNameSpan>,
) {
    collect_lambda_list_name_spans_with_style(
        parameter_form,
        RequiredParameterStyle::Specialized,
        output,
    );
}

fn collect_lambda_list_name_spans_with_style(
    parameter_form: &ExpressionView,
    required_parameter_style: RequiredParameterStyle,
    output: &mut Vec<ParameterNameSpan>,
) {
    let mut mode = LambdaListMode::Required;
    let mut index = 0usize;

    while index < parameter_form.children.len() {
        let child = &parameter_form.children[index];
        if let Some(marker) = atom_text(child) {
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
                    if let Some(next) = parameter_form.children.get(index + 1) {
                        collect_binding_pattern_name_spans(next, output);
                    }
                    index += 2;
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

        match (mode, required_parameter_style) {
            (LambdaListMode::Required, RequiredParameterStyle::Specialized) => {
                collect_specialized_required_parameter_name_span(child, output);
            }
            _ => collect_lambda_list_parameter_spec_name_spans(child, mode, output),
        }
        index += 1;
    }
}

fn collect_specialized_required_parameter_name_span(
    spec: &ExpressionView,
    output: &mut Vec<ParameterNameSpan>,
) {
    if spec.kind == ExpressionKind::List && spec.delimiter == Some(Delimiter::Paren) {
        if let Some(parameter) = spec.children.first() {
            collect_binding_pattern_name_spans(parameter, output);
            return;
        }
    }

    collect_binding_pattern_name_spans(spec, output);
}

fn collect_lambda_list_parameter_spec_name_spans(
    spec: &ExpressionView,
    mode: LambdaListMode,
    output: &mut Vec<ParameterNameSpan>,
) {
    if atom_text(spec).is_some() || mode == LambdaListMode::Required {
        collect_binding_pattern_name_spans(spec, output);
        return;
    }

    if spec.kind != ExpressionKind::List || spec.children.is_empty() {
        return;
    }

    match mode {
        LambdaListMode::Required => collect_binding_pattern_name_spans(spec, output),
        LambdaListMode::Optional => {
            collect_binding_pattern_name_spans(&spec.children[0], output);
            collect_supplied_p_name_span(spec, output);
        }
        LambdaListMode::Key => {
            collect_key_parameter_name_spans(&spec.children[0], output);
            collect_supplied_p_name_span(spec, output);
        }
        LambdaListMode::Aux => collect_binding_pattern_name_spans(&spec.children[0], output),
    }
}

fn collect_key_parameter_name_spans(
    spec_name: &ExpressionView,
    output: &mut Vec<ParameterNameSpan>,
) {
    if spec_name.kind == ExpressionKind::List && spec_name.children.len() >= 2 {
        if let Some(designator) = atom_text(&spec_name.children[0]) {
            if designator.starts_with(':') {
                collect_binding_pattern_name_spans(&spec_name.children[1], output);
                return;
            }
        }
    }

    collect_binding_pattern_name_spans(spec_name, output);
}

fn collect_supplied_p_name_span(spec: &ExpressionView, output: &mut Vec<ParameterNameSpan>) {
    if let Some(supplied_p) = spec.children.get(2) {
        collect_binding_pattern_name_spans(supplied_p, output);
    }
}

fn collect_binding_pattern_name_spans(
    pattern: &ExpressionView,
    output: &mut Vec<ParameterNameSpan>,
) {
    if let Some(name) = atom_text(pattern) {
        if !is_binding_pattern_marker(name) {
            output.push(ParameterNameSpan {
                name: name.to_owned(),
                name_span: pattern.span,
                binding_edit: BindingEdit::rename_atom(pattern.span),
            });
        }
        return;
    }

    let mut index = 0usize;
    while index < pattern.children.len() {
        let child = &pattern.children[index];
        if let Some(marker) = atom_text(child) {
            if marker == ":keys" {
                if let Some(keys_form) = pattern.children.get(index + 1) {
                    collect_clojure_keys_shorthand_name_spans(pattern, keys_form, output);
                }
                index += 2;
                continue;
            }
            if matches!(marker, ":strs" | ":syms") {
                index += 2;
                continue;
            }
            if marker == ":as" {
                if let Some(next) = pattern.children.get(index + 1) {
                    collect_binding_pattern_name_spans(next, output);
                    index += 2;
                    continue;
                }
            }
        }
        collect_binding_pattern_name_spans(child, output);
        index += 1;
    }
}

fn collect_clojure_keys_shorthand_name_spans(
    map_pattern: &ExpressionView,
    keys_form: &ExpressionView,
    output: &mut Vec<ParameterNameSpan>,
) {
    if keys_form.kind != ExpressionKind::List || keys_form.delimiter != Some(Delimiter::Bracket) {
        return;
    }

    for key in &keys_form.children {
        let Some(name) = atom_text(key) else {
            continue;
        };
        if is_binding_pattern_marker(name) {
            continue;
        }
        output.push(ParameterNameSpan {
            name: name.to_owned(),
            name_span: key.span,
            binding_edit: BindingEdit::clojure_keys_map(
                map_pattern.clone(),
                map_pattern.span,
                name.to_owned(),
            ),
        });
    }
}

fn is_binding_pattern_marker(name: &str) -> bool {
    name == "_" || name.starts_with('&') || name.starts_with(':')
}
