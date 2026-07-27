use anyhow::Result;

use crate::domain::sexpr::{ExpressionView, SymbolName};

use super::super::calls::{
    add_keyword_function_parameter_call_edit, add_optional_function_parameter_call_edit,
    add_positional_function_parameter_call_edit,
};
use super::super::definition::{
    FunctionParameterTarget, KeywordParameterInsertion, OptionalParameterInsertion,
    PositionalParameterInsertion,
};
use super::super::list_edit::{SpanEdit, atom_text, insertion_edit_for_list_item};
use super::super::types::{
    AddFunctionParameterRequest, FunctionParameterInsert, FunctionParameterSection,
};

pub(super) enum DefinitionInsertionPlan<'a> {
    ExistingPositional(&'a PositionalParameterInsertion),
    ExistingOptional(&'a OptionalParameterInsertion),
    ExistingKeyword(&'a KeywordParameterInsertion),
    CreateOptional {
        item_index: usize,
        positional_prefix_count: usize,
        parameter_text: String,
    },
    CreateKeyword {
        item_index: usize,
        positional_prefix_count: usize,
        keyword: String,
        parameter_text: String,
    },
}

impl DefinitionInsertionPlan<'_> {
    pub(super) const fn resolved_section(&self) -> FunctionParameterSection {
        match self {
            Self::ExistingPositional(_) => FunctionParameterSection::Positional,
            Self::ExistingOptional(_) | Self::CreateOptional { .. } => {
                FunctionParameterSection::Optional
            }
            Self::ExistingKeyword(_) | Self::CreateKeyword { .. } => {
                FunctionParameterSection::Keyword
            }
        }
    }

    pub(super) fn definition_edit(
        &self,
        target: &FunctionParameterTarget,
        request: &AddFunctionParameterRequest<'_>,
    ) -> Result<SpanEdit> {
        match self {
            Self::ExistingKeyword(keyword_insertion) => insertion_edit_for_list_item(
                &target.parameter_container,
                keyword_insertion.item_index(request.insert),
                request.name.as_str(),
                FunctionParameterInsert::Start,
            ),
            Self::ExistingPositional(positional_insertion) => insertion_edit_for_list_item(
                &target.parameter_container,
                positional_insertion.item_index,
                request.name.as_str(),
                FunctionParameterInsert::Start,
            ),
            Self::ExistingOptional(optional_insertion) => insertion_edit_for_list_item(
                &target.parameter_container,
                optional_insertion.item_index(request.insert),
                request.name.as_str(),
                FunctionParameterInsert::Start,
            ),
            Self::CreateOptional {
                item_index,
                parameter_text,
                ..
            }
            | Self::CreateKeyword {
                item_index,
                parameter_text,
                ..
            } => insertion_edit_for_list_item(
                &target.parameter_container,
                *item_index,
                parameter_text,
                FunctionParameterInsert::Start,
            ),
        }
    }

    pub(super) fn call_edit(
        &self,
        call_view: &ExpressionView,
        function_name: &SymbolName,
        call_argument_offset: usize,
        argument: &str,
        insert: FunctionParameterInsert,
    ) -> Result<SpanEdit> {
        match self {
            Self::ExistingKeyword(keyword_insertion) => add_keyword_function_parameter_call_edit(
                call_view,
                function_name,
                call_argument_offset,
                &keyword_insertion.keyword,
                argument,
                keyword_insertion.positional_prefix_count,
                insert,
            ),
            Self::ExistingPositional(positional_insertion) => {
                add_positional_function_parameter_call_edit(
                    call_view,
                    function_name,
                    call_argument_offset,
                    argument,
                    positional_insertion.call_argument_index,
                )
            }
            Self::ExistingOptional(optional_insertion) => {
                add_optional_function_parameter_call_edit(
                    call_view,
                    function_name,
                    call_argument_offset,
                    argument,
                    optional_insertion.positional_prefix_count,
                    optional_insertion.call_argument_index(insert),
                )
            }
            Self::CreateOptional {
                positional_prefix_count,
                ..
            } => add_optional_function_parameter_call_edit(
                call_view,
                function_name,
                call_argument_offset,
                argument,
                *positional_prefix_count,
                *positional_prefix_count,
            ),
            Self::CreateKeyword {
                keyword,
                positional_prefix_count,
                ..
            } => add_keyword_function_parameter_call_edit(
                call_view,
                function_name,
                call_argument_offset,
                keyword,
                argument,
                *positional_prefix_count,
                insert,
            ),
        }
    }
}

pub(super) fn resolve_definition_insertion_plan<'a>(
    target: &'a FunctionParameterTarget,
    request: &AddFunctionParameterRequest<'_>,
) -> Option<DefinitionInsertionPlan<'a>> {
    match request.section {
        FunctionParameterSection::Auto => target
            .keyword_parameter_insertion
            .as_ref()
            .map(DefinitionInsertionPlan::ExistingKeyword)
            .or_else(|| {
                target
                    .optional_parameter_insertion
                    .as_ref()
                    .map(DefinitionInsertionPlan::ExistingOptional)
            })
            .or_else(|| {
                target
                    .positional_parameter_insertion
                    .as_ref()
                    .map(DefinitionInsertionPlan::ExistingPositional)
            }),
        FunctionParameterSection::Positional => target
            .positional_parameter_insertion
            .as_ref()
            .map(DefinitionInsertionPlan::ExistingPositional),
        FunctionParameterSection::Optional => target
            .optional_parameter_insertion
            .as_ref()
            .map(DefinitionInsertionPlan::ExistingOptional)
            .or_else(|| create_optional_insertion_plan(target, request.name.as_str())),
        FunctionParameterSection::Keyword => target
            .keyword_parameter_insertion
            .as_ref()
            .map(DefinitionInsertionPlan::ExistingKeyword)
            .or_else(|| create_keyword_insertion_plan(target, request.name.as_str())),
    }
}

fn create_optional_insertion_plan<'a>(
    target: &'a FunctionParameterTarget,
    parameter_name: &str,
) -> Option<DefinitionInsertionPlan<'a>> {
    if let Some(keyword_insertion) = target.keyword_parameter_insertion.as_ref() {
        return Some(DefinitionInsertionPlan::CreateOptional {
            item_index: keyword_insertion.first_item_index.saturating_sub(1),
            positional_prefix_count: keyword_insertion.positional_prefix_count,
            parameter_text: format!("&optional {parameter_name}"),
        });
    }

    if let Some(positional_insertion) = target.positional_parameter_insertion.as_ref() {
        return Some(DefinitionInsertionPlan::CreateOptional {
            item_index: positional_insertion.item_index,
            positional_prefix_count: positional_insertion.call_argument_index,
            parameter_text: format!("&optional {parameter_name}"),
        });
    }

    (!target.has_lambda_list_marker).then(|| {
        let positional_prefix_count = target
            .parameter_container
            .children
            .len()
            .saturating_sub(target.protected_prefix_count);
        DefinitionInsertionPlan::CreateOptional {
            item_index: target.parameter_container.children.len(),
            positional_prefix_count,
            parameter_text: format!("&optional {parameter_name}"),
        }
    })
}

fn create_keyword_insertion_plan<'a>(
    target: &'a FunctionParameterTarget,
    parameter_name: &str,
) -> Option<DefinitionInsertionPlan<'a>> {
    if let Some(optional_insertion) = target.optional_parameter_insertion.as_ref() {
        if optional_insertion.end_item_index != target.parameter_container.children.len() {
            return None;
        }
        let positional_prefix_count = optional_insertion.positional_prefix_count
            + optional_insertion.optional_parameter_count;
        return Some(DefinitionInsertionPlan::CreateKeyword {
            item_index: optional_insertion.end_item_index,
            positional_prefix_count,
            keyword: format!(":{parameter_name}"),
            parameter_text: format!("&key {parameter_name}"),
        });
    }

    if target.has_lambda_list_marker {
        return None;
    }

    Some(DefinitionInsertionPlan::CreateKeyword {
        item_index: target.parameter_container.children.len(),
        positional_prefix_count: target
            .parameter_container
            .children
            .iter()
            .skip(target.protected_prefix_count)
            .filter(|child| atom_text(child).is_none_or(|text| !text.starts_with('&')))
            .count(),
        keyword: format!(":{parameter_name}"),
        parameter_text: format!("&key {parameter_name}"),
    })
}
