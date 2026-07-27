use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionView, SymbolName};

use super::super::list_edit::{atom_text, is_dotted_list_separator};
use super::lambda_list::default_keyword_for_parameter;
use super::types::{
    KeywordParameterInsertion, OptionalParameterInsertion, PositionalParameterInsertion,
};

pub(crate) fn keyword_parameter_insertion(
    dialect: Dialect,
    parameter_form: &ExpressionView,
    protected_prefix_count: usize,
    new_parameter: &SymbolName,
) -> Result<Option<KeywordParameterInsertion>> {
    if !dialect.supports_common_lisp_lambda_list_refactor_model() {
        return Ok(None);
    }

    let mut positional_prefix_count = 0usize;
    let mut in_keyword_section = false;
    let mut first_item_index = None;
    let mut end_item_index = None;
    let mut positional_call_arguments = true;

    for (item_index, child) in parameter_form
        .children
        .iter()
        .enumerate()
        .skip(protected_prefix_count)
    {
        if is_dotted_list_separator(child) {
            break;
        }
        if let Some(marker) = atom_text(child).filter(|name| name.starts_with('&')) {
            match marker {
                "&key" => {
                    if first_item_index.is_some() {
                        anyhow::bail!("add-function-parameter found duplicate &key marker");
                    }
                    positional_call_arguments = false;
                    in_keyword_section = true;
                    first_item_index = Some(item_index + 1);
                }
                "&allow-other-keys" => {
                    positional_call_arguments = false;
                    if in_keyword_section && end_item_index.is_none() {
                        end_item_index = Some(item_index);
                    }
                    in_keyword_section = false;
                }
                "&optional" => {
                    positional_call_arguments = true;
                    if in_keyword_section && end_item_index.is_none() {
                        end_item_index = Some(item_index);
                    }
                    in_keyword_section = false;
                }
                _ => {
                    positional_call_arguments = false;
                    if in_keyword_section && end_item_index.is_none() {
                        end_item_index = Some(item_index);
                    }
                    in_keyword_section = false;
                }
            }
            continue;
        }

        if first_item_index.is_none() && positional_call_arguments {
            positional_prefix_count += 1;
        }
    }

    let Some(first_item_index) = first_item_index else {
        return Ok(None);
    };
    let end_item_index = end_item_index.unwrap_or(parameter_form.children.len());
    Ok(Some(KeywordParameterInsertion {
        first_item_index,
        end_item_index,
        positional_prefix_count,
        keyword: default_keyword_for_parameter(new_parameter.as_str()),
    }))
}

pub(crate) fn optional_parameter_insertion(
    dialect: Dialect,
    parameter_form: &ExpressionView,
    protected_prefix_count: usize,
) -> Result<Option<OptionalParameterInsertion>> {
    if !dialect.supports_common_lisp_lambda_list_refactor_model() {
        return Ok(None);
    }

    let mut positional_prefix_count = 0usize;
    let mut optional_parameter_count = 0usize;
    let mut in_optional_section = false;
    let mut first_item_index = None;
    let mut end_item_index = None;

    for (item_index, child) in parameter_form
        .children
        .iter()
        .enumerate()
        .skip(protected_prefix_count)
    {
        if is_dotted_list_separator(child) {
            break;
        }
        if let Some(marker) = atom_text(child).filter(|name| name.starts_with('&')) {
            if marker == "&optional" {
                if first_item_index.is_some() {
                    anyhow::bail!("add-function-parameter found duplicate &optional marker");
                }
                in_optional_section = true;
                first_item_index = Some(item_index + 1);
            } else {
                if in_optional_section && end_item_index.is_none() {
                    end_item_index = Some(item_index);
                }
                in_optional_section = false;
            }
            continue;
        }

        if first_item_index.is_none() {
            positional_prefix_count += 1;
        } else if in_optional_section {
            optional_parameter_count += 1;
        }
    }

    let Some(first_item_index) = first_item_index else {
        return Ok(None);
    };
    let end_item_index = end_item_index.unwrap_or(parameter_form.children.len());
    Ok(Some(OptionalParameterInsertion {
        first_item_index,
        end_item_index,
        positional_prefix_count,
        optional_parameter_count,
    }))
}

pub(crate) fn positional_parameter_insertion(
    dialect: Dialect,
    parameter_form: &ExpressionView,
    protected_prefix_count: usize,
) -> Result<Option<PositionalParameterInsertion>> {
    if !dialect.supports_common_lisp_lambda_list_refactor_model() {
        return Ok(None);
    }

    let mut positional_prefix_count = 0usize;
    let mut insertion_item_index = None;

    for (item_index, child) in parameter_form
        .children
        .iter()
        .enumerate()
        .skip(protected_prefix_count)
    {
        if is_dotted_list_separator(child) {
            insertion_item_index = Some(item_index);
            break;
        }
        if atom_text(child).is_some_and(|name| name.starts_with('&')) {
            insertion_item_index = Some(item_index);
            break;
        }
        positional_prefix_count += 1;
    }

    Ok(
        insertion_item_index.map(|item_index| PositionalParameterInsertion {
            item_index,
            call_argument_index: positional_prefix_count,
        }),
    )
}
