use crate::domain::sexpr::ExpressionView;

use super::super::syntax::atom_text;
use super::ExtractFunctionSemantic;
use super::symbols::is_extract_function_param_candidate;

pub(super) fn parameter_names(
    semantic: ExtractFunctionSemantic,
    parameter_form: &ExpressionView,
) -> Vec<String> {
    let mut names = Vec::new();
    collect_lambda_list_parameter_names(semantic, parameter_form, &mut names);
    names
}

pub(super) fn extract_function_pattern_names(
    semantic: ExtractFunctionSemantic,
    pattern: &ExpressionView,
) -> Vec<String> {
    let mut names = Vec::new();
    collect_extract_function_pattern_names(semantic, pattern, &mut names);
    names
}

fn collect_extract_function_pattern_names(
    semantic: ExtractFunctionSemantic,
    pattern: &ExpressionView,
    names: &mut Vec<String>,
) {
    if let Some(text) = atom_text(pattern) {
        push_extract_function_pattern_name(semantic, text, names);
        return;
    }

    for child in &pattern.children {
        collect_extract_function_pattern_names(semantic, child, names);
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LambdaListMode {
    Required,
    Optional,
    Key,
    Aux,
}

fn collect_lambda_list_parameter_names(
    semantic: ExtractFunctionSemantic,
    parameter_form: &ExpressionView,
    names: &mut Vec<String>,
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
                        collect_extract_function_pattern_names(semantic, next, names);
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

        collect_lambda_list_parameter_spec_names(semantic, child, mode, names);
        index += 1;
    }
}

fn collect_lambda_list_parameter_spec_names(
    semantic: ExtractFunctionSemantic,
    spec: &ExpressionView,
    mode: LambdaListMode,
    names: &mut Vec<String>,
) {
    if atom_text(spec).is_some() || mode == LambdaListMode::Required {
        collect_extract_function_pattern_names(semantic, spec, names);
        return;
    }

    if spec.children.is_empty() {
        return;
    }

    match mode {
        LambdaListMode::Required => collect_extract_function_pattern_names(semantic, spec, names),
        LambdaListMode::Optional => {
            collect_extract_function_pattern_names(semantic, &spec.children[0], names);
            collect_supplied_p_name(semantic, spec, names);
        }
        LambdaListMode::Key => {
            collect_key_parameter_name(semantic, &spec.children[0], names);
            collect_supplied_p_name(semantic, spec, names);
        }
        LambdaListMode::Aux => {
            collect_extract_function_pattern_names(semantic, &spec.children[0], names);
        }
    }
}

fn collect_key_parameter_name(
    semantic: ExtractFunctionSemantic,
    spec_name: &ExpressionView,
    names: &mut Vec<String>,
) {
    if spec_name.children.len() >= 2 {
        if let Some(designator) = atom_text(&spec_name.children[0]) {
            if designator.starts_with(':') {
                collect_extract_function_pattern_names(semantic, &spec_name.children[1], names);
                return;
            }
        }
    }

    collect_extract_function_pattern_names(semantic, spec_name, names);
}

fn collect_supplied_p_name(
    semantic: ExtractFunctionSemantic,
    spec: &ExpressionView,
    names: &mut Vec<String>,
) {
    if let Some(supplied_p) = spec.children.get(2) {
        collect_extract_function_pattern_names(semantic, supplied_p, names);
    }
}

fn push_extract_function_pattern_name(
    semantic: ExtractFunctionSemantic,
    text: &str,
    names: &mut Vec<String>,
) {
    if text != "_"
        && is_extract_function_param_candidate(text)
        && !names
            .iter()
            .any(|name| super::extract_function_param_name_eq(semantic, name, text))
    {
        names.push(text.to_owned());
    }
}
