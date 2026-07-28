use crate::error::{FunctionParameterResult, LambdaListError, ParameterSelectionError};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SymbolName};

use super::super::list_edit::{atom_text, is_dotted_list_separator};
use super::types::{KeywordArgumentLocation, ParameterLocation, ParameterSection};

struct LambdaListBinding<'a> {
    name: &'a str,
    keyword: Option<String>,
}

pub fn parameter_locations(
    dialect: Dialect,
    parameter_form: &ExpressionView,
    protected_prefix_count: usize,
    allow_specialized_required_parameters: bool,
    operation: &'static str,
) -> FunctionParameterResult<Vec<ParameterLocation>> {
    match parameter_form.kind {
        ExpressionKind::List => parameter_locations_from_children(
            dialect,
            &parameter_form.children,
            protected_prefix_count,
            allow_specialized_required_parameters,
            operation,
        ),
        _ => Err(LambdaListError::NotAListOrVector { operation }.into()),
    }
}

fn parameter_locations_from_children(
    dialect: Dialect,
    children: &[ExpressionView],
    protected_prefix_count: usize,
    allow_specialized_required_parameters: bool,
    operation: &'static str,
) -> FunctionParameterResult<Vec<ParameterLocation>> {
    let mut locations = Vec::with_capacity(children.len().saturating_sub(protected_prefix_count));
    let mut call_index = 0usize;
    let mut positional = true;
    let mut allow_lambda_list_spec = false;
    let mut keyword_parameters = false;
    let mut accepts_parameters = true;
    let mut section = ParameterSection::Required;
    let supports_common_lisp_lambda_list =
        dialect.supports_common_lisp_lambda_list_refactor_model();

    for (item_index, child) in children.iter().enumerate().skip(protected_prefix_count) {
        if is_dotted_list_separator(child) {
            if !supports_common_lisp_lambda_list {
                return Err(LambdaListError::DottedNotSupported { operation }.into());
            }
            if section != ParameterSection::Required
                || !positional
                || allow_lambda_list_spec
                || keyword_parameters
                || !accepts_parameters
            {
                return Err(LambdaListError::DottedAfterRequired { operation }.into());
            }
            if locations.is_empty() {
                return Err(LambdaListError::DottedNeedsParameter { operation }.into());
            }
            let tail_index = item_index + 1;
            let tail = children
                .get(tail_index)
                .ok_or(LambdaListError::DottedSeparatorNeedsParameter { operation })?;
            let tail_name =
                atom_text(tail).ok_or(LambdaListError::DottedTailNotASymbol { operation })?;
            SymbolName::new(tail_name.to_owned()).map_err(|_| {
                ParameterSelectionError::InvalidSymbolFor {
                    operation,
                    name: tail_name.to_owned(),
                }
            })?;
            if tail_index + 1 != children.len() {
                return Err(LambdaListError::DottedTailNotFinal { operation }.into());
            }
            locations.push(ParameterLocation {
                name: tail_name.to_owned(),
                item_index: tail_index,
                section: ParameterSection::Other,
                call_index: None,
                keyword_argument: None,
            });
            break;
        }
        if let Some(marker) = atom_text(child).filter(|name| name.starts_with('&')) {
            if !supports_common_lisp_lambda_list {
                return Err(LambdaListError::ModifierNotSupported {
                    operation,
                    marker: marker.to_string(),
                }
                .into());
            }
            match marker {
                "&optional" => {
                    accepts_parameters = true;
                    positional = true;
                    allow_lambda_list_spec = true;
                    keyword_parameters = false;
                    section = ParameterSection::Optional;
                }
                "&key" => {
                    accepts_parameters = true;
                    positional = false;
                    allow_lambda_list_spec = true;
                    keyword_parameters = true;
                    section = ParameterSection::Keyword;
                }
                "&aux" | "&rest" | "&body" | "&whole" | "&environment" => {
                    accepts_parameters = true;
                    positional = false;
                    allow_lambda_list_spec = marker == "&aux";
                    keyword_parameters = false;
                    section = ParameterSection::Other;
                }
                "&allow-other-keys" => {
                    if !keyword_parameters {
                        return Err(LambdaListError::AllowOtherKeysWithoutKey { operation }.into());
                    }
                    accepts_parameters = false;
                    positional = false;
                    allow_lambda_list_spec = false;
                    keyword_parameters = false;
                    section = ParameterSection::Other;
                }
                _ => {
                    return Err(LambdaListError::UnsupportedMarker {
                        operation,
                        marker: marker.to_string(),
                    }
                    .into());
                }
            }
            continue;
        }

        if !accepts_parameters {
            return Err(LambdaListError::ParametersAfterAllowOtherKeys { operation }.into());
        }
        let allow_specialized_required =
            allow_specialized_required_parameters && positional && !allow_lambda_list_spec;
        let binding = lambda_list_binding(
            child,
            allow_lambda_list_spec,
            keyword_parameters,
            allow_specialized_required,
        )
        .ok_or(LambdaListError::OnlySimpleParameters { operation })?;
        SymbolName::new(binding.name.to_owned()).map_err(|_| {
            ParameterSelectionError::InvalidSymbolFor {
                operation,
                name: binding.name.to_string(),
            }
        })?;
        let call_index_for_parameter = positional.then_some(call_index);
        let keyword_argument = binding.keyword.map(|keyword| KeywordArgumentLocation {
            keyword,
            positional_prefix_count: call_index,
        });
        if positional {
            call_index += 1;
        }
        locations.push(ParameterLocation {
            name: binding.name.to_owned(),
            item_index,
            section,
            call_index: call_index_for_parameter,
            keyword_argument,
        });
    }
    Ok(locations)
}

pub fn default_keyword_for_parameter(name: &str) -> String {
    if name.starts_with(':') {
        name.to_owned()
    } else {
        format!(":{name}")
    }
}

fn lambda_list_binding<'a>(
    child: &'a ExpressionView,
    allow_spec: bool,
    keyword_parameters: bool,
    allow_specialized_required: bool,
) -> Option<LambdaListBinding<'a>> {
    if let Some(name) = atom_text(child) {
        if keyword_parameters && name.starts_with(':') {
            return None;
        }
        return Some(LambdaListBinding {
            name,
            keyword: keyword_parameters.then(|| default_keyword_for_parameter(name)),
        });
    }
    if allow_specialized_required {
        if child.kind != ExpressionKind::List || child.children.len() != 2 {
            return None;
        }
        let name = atom_text(child.children.first()?)?;
        if name.starts_with('&') || name.starts_with(':') {
            return None;
        }
        return Some(LambdaListBinding {
            name,
            keyword: None,
        });
    }
    if !allow_spec {
        return None;
    }

    let binding = child.children.first()?;
    if let Some(name) = atom_text(binding) {
        if keyword_parameters && name.starts_with(':') {
            return None;
        }
        return Some(LambdaListBinding {
            name,
            keyword: keyword_parameters.then(|| default_keyword_for_parameter(name)),
        });
    }

    if keyword_parameters && binding.children.len() != 2 {
        return None;
    }
    let keyword = atom_text(binding.children.first()?)?;
    if keyword_parameters && !keyword.starts_with(':') {
        return None;
    }
    let name = binding.children.get(1).and_then(atom_text)?;
    Some(LambdaListBinding {
        name,
        keyword: keyword_parameters.then(|| keyword.to_owned()),
    })
}
