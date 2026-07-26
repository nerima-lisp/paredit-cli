use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView};

use super::syntax::{atom_symbol_span, atom_text};

/// A name a binding form introduces, together with the span of the atom that
/// spells it.
///
/// The span is what a persistent binding table keys on: a name alone cannot
/// distinguish two `x` bindings in sibling scopes. Shadow-aware reference
/// collection only ever needed the name, so this used to be a bare `String`;
/// both readings are now derived from the same traversal so the binding table
/// and the reference query cannot disagree about what a form binds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundName {
    pub(crate) name: String,
    pub(crate) span: ByteSpan,
}

pub(super) fn binding_pattern_names(pattern: &ExpressionView) -> Vec<String> {
    binding_pattern_bound_names(pattern)
        .into_iter()
        .map(|bound| bound.name)
        .collect()
}

pub(super) fn lambda_list_names(parameter_form: &ExpressionView) -> Vec<String> {
    lambda_list_bound_names(parameter_form)
        .into_iter()
        .map(|bound| bound.name)
        .collect()
}

/// Every name a destructuring pattern binds, with its defining span.
pub(crate) fn binding_pattern_bound_names(pattern: &ExpressionView) -> Vec<BoundName> {
    let mut names = Vec::new();
    collect_binding_pattern_names(pattern, &mut names);
    names
}

/// Every name a lambda list binds, with its defining span.
pub(crate) fn lambda_list_bound_names(parameter_form: &ExpressionView) -> Vec<BoundName> {
    let mut names = Vec::new();
    collect_lambda_list_names(parameter_form, &mut names);
    names
}

/// Records `view`'s atom text as a bound name. Skipped when the atom has no
/// recoverable symbol span, which cannot happen for the prefix-free atoms
/// [`atom_text`] accepts.
fn push_bound_name(view: &ExpressionView, name: &str, output: &mut Vec<BoundName>) {
    if let Some(span) = atom_symbol_span(view) {
        output.push(BoundName {
            name: name.to_owned(),
            span,
        });
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LambdaListMode {
    Required,
    Optional,
    Key,
    Aux,
}

fn collect_lambda_list_names(parameter_form: &ExpressionView, output: &mut Vec<BoundName>) {
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
                        collect_binding_pattern_names(next, output);
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

        collect_lambda_list_parameter_spec_names(child, mode, output);
        index += 1;
    }
}

fn collect_lambda_list_parameter_spec_names(
    spec: &ExpressionView,
    mode: LambdaListMode,
    output: &mut Vec<BoundName>,
) {
    if atom_text(spec).is_some() || mode == LambdaListMode::Required {
        collect_binding_pattern_names(spec, output);
        return;
    }

    if spec.kind != ExpressionKind::List || spec.children.is_empty() {
        return;
    }

    match mode {
        LambdaListMode::Required => collect_binding_pattern_names(spec, output),
        LambdaListMode::Optional => {
            collect_binding_pattern_names(&spec.children[0], output);
            collect_supplied_p_name(spec, output);
        }
        LambdaListMode::Key => {
            collect_key_parameter_name(&spec.children[0], output);
            collect_supplied_p_name(spec, output);
        }
        LambdaListMode::Aux => collect_binding_pattern_names(&spec.children[0], output),
    }
}

fn collect_key_parameter_name(spec_name: &ExpressionView, output: &mut Vec<BoundName>) {
    if spec_name.kind == ExpressionKind::List && spec_name.children.len() >= 2 {
        if let Some(designator) = atom_text(&spec_name.children[0]) {
            if designator.starts_with(':') {
                collect_binding_pattern_names(&spec_name.children[1], output);
                return;
            }
        }
    }

    collect_binding_pattern_names(spec_name, output);
}

fn collect_supplied_p_name(spec: &ExpressionView, output: &mut Vec<BoundName>) {
    if let Some(supplied_p) = spec.children.get(2) {
        collect_binding_pattern_names(supplied_p, output);
    }
}

fn collect_binding_pattern_names(pattern: &ExpressionView, output: &mut Vec<BoundName>) {
    if let Some(name) = atom_text(pattern) {
        if !is_binding_pattern_marker(name) {
            push_bound_name(pattern, name, output);
        }
        return;
    }

    let mut index = 0usize;
    while index < pattern.children.len() {
        let child = &pattern.children[index];
        if let Some(marker) = atom_text(child) {
            if marker == ":keys" {
                if let Some(keys_form) = pattern.children.get(index + 1) {
                    collect_clojure_keys_shorthand_names(keys_form, output);
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
                    collect_binding_pattern_names(next, output);
                    index += 2;
                    continue;
                }
            }
        }
        collect_binding_pattern_names(child, output);
        index += 1;
    }
}

fn collect_clojure_keys_shorthand_names(keys_form: &ExpressionView, output: &mut Vec<BoundName>) {
    if keys_form.kind != ExpressionKind::List || keys_form.delimiter != Some(Delimiter::Bracket) {
        return;
    }

    for key in &keys_form.children {
        let Some(name) = atom_text(key) else {
            continue;
        };
        if !is_binding_pattern_marker(name) {
            push_bound_name(key, name, output);
        }
    }
}

fn is_binding_pattern_marker(name: &str) -> bool {
    name == "_" || name.starts_with('&') || name.starts_with(':')
}
