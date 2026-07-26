use anyhow::{Context, Result};

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionKind, ExpressionView, SymbolName};

use super::super::list_edit::{atom_text, is_dotted_list_separator};
use super::types::{KeywordArgumentLocation, ParameterLocation, ParameterSection};

struct LambdaListBinding<'a> {
    name: &'a str,
    keyword: Option<String>,
}

pub(crate) fn parameter_locations(
    dialect: Dialect,
    parameter_form: &ExpressionView,
    protected_prefix_count: usize,
    allow_specialized_required_parameters: bool,
    operation: &str,
) -> Result<Vec<ParameterLocation>> {
    match parameter_form.kind {
        ExpressionKind::List => parameter_locations_from_children(
            dialect,
            &parameter_form.children,
            protected_prefix_count,
            allow_specialized_required_parameters,
            operation,
        ),
        _ => anyhow::bail!("{operation} function parameter form must be a list or vector"),
    }
}

fn parameter_locations_from_children(
    dialect: Dialect,
    children: &[ExpressionView],
    protected_prefix_count: usize,
    allow_specialized_required_parameters: bool,
    operation: &str,
) -> Result<Vec<ParameterLocation>> {
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
                anyhow::bail!("{operation} dotted lambda-list separators are not supported");
            }
            if section != ParameterSection::Required
                || !positional
                || allow_lambda_list_spec
                || keyword_parameters
                || !accepts_parameters
            {
                anyhow::bail!(
                    "{operation} dotted lambda-list separator must follow required parameters"
                );
            }
            if locations.is_empty() {
                anyhow::bail!(
                    "{operation} dotted lambda-list separator must follow at least one parameter"
                );
            }
            let tail_index = item_index + 1;
            let tail = children.get(tail_index).with_context(|| {
                format!("{operation} dotted lambda-list separator must be followed by a parameter")
            })?;
            let tail_name = atom_text(tail)
                .with_context(|| format!("{operation} dotted lambda-list tail must be a symbol"))?;
            SymbolName::new(tail_name.to_owned()).with_context(|| {
                format!("{operation} found invalid parameter symbol '{tail_name}'")
            })?;
            if tail_index + 1 != children.len() {
                anyhow::bail!("{operation} dotted lambda-list tail must be the final parameter");
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
                anyhow::bail!(
                    "{operation} function parameter modifiers are not supported: {marker}"
                );
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
                        anyhow::bail!(
                            "{operation} lambda-list marker &allow-other-keys is only supported after &key"
                        );
                    }
                    accepts_parameters = false;
                    positional = false;
                    allow_lambda_list_spec = false;
                    keyword_parameters = false;
                    section = ParameterSection::Other;
                }
                _ => anyhow::bail!("{operation} unsupported lambda-list marker: {marker}"),
            }
            continue;
        }

        if !accepts_parameters {
            anyhow::bail!(
                "{operation} does not support parameters after &allow-other-keys before another lambda-list marker"
            );
        }
        let allow_specialized_required =
            allow_specialized_required_parameters && positional && !allow_lambda_list_spec;
        let binding = lambda_list_binding(
            child,
            allow_lambda_list_spec,
            keyword_parameters,
            allow_specialized_required,
        )
        .with_context(|| format!("{operation} currently supports only simple parameters"))?;
        SymbolName::new(binding.name.to_owned()).with_context(|| {
            format!(
                "{operation} found invalid parameter symbol '{}'",
                binding.name
            )
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

pub(crate) fn default_keyword_for_parameter(name: &str) -> String {
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
