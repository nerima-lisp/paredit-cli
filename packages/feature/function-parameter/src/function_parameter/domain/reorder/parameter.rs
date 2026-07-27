use anyhow::{Context, Result};

use paredit_core_syntax::sexpr::SymbolName;

use super::super::definition::{ParameterLocation, ParameterSection};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterGroup {
    Required,
    Optional,
    Keyword,
}

#[derive(Clone, Debug)]
pub struct ReorderableParameter {
    pub name: SymbolName,
    pub item_index: usize,
    pub group: ParameterGroup,
    pub call_index: Option<usize>,
    pub keyword: Option<String>,
    pub positional_prefix_count: Option<usize>,
}

pub fn reorderable_parameters(
    parameters: &[ParameterLocation],
    operation: &str,
) -> Result<Vec<ReorderableParameter>> {
    let parameters = parameters
        .iter()
        .map(|parameter| {
            let name = SymbolName::new(parameter.name.clone()).with_context(|| {
                format!(
                    "{operation} found invalid parameter symbol '{}'",
                    parameter.name
                )
            })?;
            if let Some(keyword_argument) = parameter.keyword_argument.as_ref() {
                return Ok(Some(ReorderableParameter {
                    name,
                    item_index: parameter.item_index,
                    group: ParameterGroup::Keyword,
                    call_index: None,
                    keyword: Some(keyword_argument.keyword.clone()),
                    positional_prefix_count: Some(keyword_argument.positional_prefix_count),
                }));
            }
            if let Some(call_index) = parameter.call_index {
                return Ok(match parameter.section {
                    ParameterSection::Required => Some(ReorderableParameter {
                        name,
                        item_index: parameter.item_index,
                        group: ParameterGroup::Required,
                        call_index: Some(call_index),
                        keyword: None,
                        positional_prefix_count: None,
                    }),
                    ParameterSection::Optional => Some(ReorderableParameter {
                        name,
                        item_index: parameter.item_index,
                        group: ParameterGroup::Optional,
                        call_index: Some(call_index),
                        keyword: None,
                        positional_prefix_count: None,
                    }),
                    ParameterSection::Keyword => Some(ReorderableParameter {
                        name,
                        item_index: parameter.item_index,
                        group: ParameterGroup::Keyword,
                        call_index: Some(call_index),
                        keyword: None,
                        positional_prefix_count: None,
                    }),
                    ParameterSection::Other => None,
                });
            }

            Ok(match parameter.section {
                ParameterSection::Other => None,
                ParameterSection::Required
                | ParameterSection::Optional
                | ParameterSection::Keyword => {
                    anyhow::bail!(
                        "{operation} does not support reordering parameter '{}' because it is not a direct call argument",
                        parameter.name
                    )
                }
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(parameters.into_iter().flatten().collect())
}

pub fn ensure_parameter_is_reorderable(
    parameters: &[ReorderableParameter],
    item_index: usize,
    parameter_name: &SymbolName,
    operation: &str,
) -> Result<usize> {
    parameters
        .iter()
        .position(|candidate| candidate.item_index == item_index)
        .with_context(|| {
            format!(
                "{operation} does not support reordering parameter '{parameter_name}' because it is not a direct call argument"
            )
        })
}
