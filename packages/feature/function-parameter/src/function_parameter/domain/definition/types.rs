use crate::domain::common_lisp::CommonLispLocalCallableForm;
use crate::domain::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::super::FunctionParameterInsert;

#[derive(Debug)]
pub(crate) struct FunctionParameterTarget {
    pub(crate) function_name: SymbolName,
    pub(crate) parameter_container: ExpressionView,
    pub(crate) call_argument_offset: usize,
    pub(crate) protected_prefix_count: usize,
    pub(crate) definition_span: ByteSpan,
    pub(crate) definition_scope: FunctionParameterDefinitionScope,
    pub(crate) has_lambda_list_marker: bool,
    pub(crate) positional_parameter_insertion: Option<PositionalParameterInsertion>,
    pub(crate) keyword_parameter_insertion: Option<KeywordParameterInsertion>,
    pub(crate) optional_parameter_insertion: Option<OptionalParameterInsertion>,
    pub(crate) parameters: Vec<ParameterLocation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FunctionParameterDefinitionScope {
    TopLevel,
    LocalCallableBinding {
        form: CommonLispLocalCallableForm,
        enclosing_form_span: ByteSpan,
    },
}

#[derive(Debug)]
pub(crate) struct ParameterLocation {
    pub(crate) name: String,
    pub(crate) item_index: usize,
    pub(crate) section: ParameterSection,
    pub(crate) call_index: Option<usize>,
    pub(crate) keyword_argument: Option<KeywordArgumentLocation>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum ParameterSection {
    Required,
    Optional,
    Keyword,
    Other,
}

#[derive(Debug)]
pub(crate) struct KeywordArgumentLocation {
    pub(crate) keyword: String,
    pub(crate) positional_prefix_count: usize,
}

#[derive(Debug)]
pub(crate) struct KeywordParameterInsertion {
    pub(crate) first_item_index: usize,
    pub(crate) end_item_index: usize,
    pub(crate) positional_prefix_count: usize,
    pub(crate) keyword: String,
}

impl KeywordParameterInsertion {
    pub(crate) const fn item_index(&self, insert: FunctionParameterInsert) -> usize {
        match insert {
            FunctionParameterInsert::Start => self.first_item_index,
            FunctionParameterInsert::End => self.end_item_index,
        }
    }
}

#[derive(Debug)]
pub(crate) struct OptionalParameterInsertion {
    pub(crate) first_item_index: usize,
    pub(crate) end_item_index: usize,
    pub(crate) positional_prefix_count: usize,
    pub(crate) optional_parameter_count: usize,
}

impl OptionalParameterInsertion {
    pub(crate) const fn item_index(&self, insert: FunctionParameterInsert) -> usize {
        match insert {
            FunctionParameterInsert::Start => self.first_item_index,
            FunctionParameterInsert::End => self.end_item_index,
        }
    }

    pub(crate) const fn call_argument_index(&self, insert: FunctionParameterInsert) -> usize {
        match insert {
            FunctionParameterInsert::Start => self.positional_prefix_count,
            FunctionParameterInsert::End => {
                self.positional_prefix_count + self.optional_parameter_count
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct PositionalParameterInsertion {
    pub(crate) item_index: usize,
    pub(crate) call_argument_index: usize,
}
