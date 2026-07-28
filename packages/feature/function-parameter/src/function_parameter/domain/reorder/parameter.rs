use crate::error::{FunctionParameterResult, ParameterSelectionError};

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
    operation: &'static str,
) -> FunctionParameterResult<Vec<ReorderableParameter>> {
    let parameters = parameters
        .iter()
        .map(|parameter| {
            let name = SymbolName::new(parameter.name.clone()).map_err(|_| {
                ParameterSelectionError::InvalidSymbolFor {
                    operation,
                    name: parameter.name.to_string(),
                }
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

            match parameter.section {
                ParameterSection::Other => Ok(None),
                ParameterSection::Required
                | ParameterSection::Optional
                | ParameterSection::Keyword => {
                    Err(ParameterSelectionError::NotADirectCallArgument {
                        operation,
                        name: parameter.name.to_string(),
                    }
                    .into())
                }
            }
        })
        .collect::<FunctionParameterResult<Vec<_>>>()?;

    Ok(parameters.into_iter().flatten().collect())
}

pub fn ensure_parameter_is_reorderable(
    parameters: &[ReorderableParameter],
    item_index: usize,
    parameter_name: &SymbolName,
    operation: &'static str,
) -> FunctionParameterResult<usize> {
    parameters
        .iter()
        .position(|candidate| candidate.item_index == item_index)
        .ok_or_else(|| {
            ParameterSelectionError::NotADirectCallArgument {
                operation,
                name: parameter_name.to_string(),
            }
            .into()
        })
}
