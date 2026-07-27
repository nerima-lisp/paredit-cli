use paredit_core_syntax::common_lisp::CommonLispLocalCallableForm;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::super::FunctionParameterInsert;

#[derive(Debug)]
pub struct FunctionParameterTarget {
    pub function_name: SymbolName,
    pub parameter_container: ExpressionView,
    pub call_argument_offset: usize,
    pub protected_prefix_count: usize,
    pub definition_span: ByteSpan,
    pub definition_scope: FunctionParameterDefinitionScope,
    pub has_lambda_list_marker: bool,
    pub positional_parameter_insertion: Option<PositionalParameterInsertion>,
    pub keyword_parameter_insertion: Option<KeywordParameterInsertion>,
    pub optional_parameter_insertion: Option<OptionalParameterInsertion>,
    pub parameters: Vec<ParameterLocation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FunctionParameterDefinitionScope {
    TopLevel,
    LocalCallableBinding {
        form: CommonLispLocalCallableForm,
        enclosing_form_span: ByteSpan,
    },
}

#[derive(Debug)]
pub struct ParameterLocation {
    pub name: String,
    pub item_index: usize,
    pub section: ParameterSection,
    pub call_index: Option<usize>,
    pub keyword_argument: Option<KeywordArgumentLocation>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ParameterSection {
    Required,
    Optional,
    Keyword,
    Other,
}

#[derive(Debug)]
pub struct KeywordArgumentLocation {
    pub keyword: String,
    pub positional_prefix_count: usize,
}

#[derive(Debug)]
pub struct KeywordParameterInsertion {
    pub first_item_index: usize,
    pub end_item_index: usize,
    pub positional_prefix_count: usize,
    pub keyword: String,
}

impl KeywordParameterInsertion {
    pub const fn item_index(&self, insert: FunctionParameterInsert) -> usize {
        match insert {
            FunctionParameterInsert::Start => self.first_item_index,
            FunctionParameterInsert::End => self.end_item_index,
        }
    }
}

#[derive(Debug)]
pub struct OptionalParameterInsertion {
    pub first_item_index: usize,
    pub end_item_index: usize,
    pub positional_prefix_count: usize,
    pub optional_parameter_count: usize,
}

impl OptionalParameterInsertion {
    pub const fn item_index(&self, insert: FunctionParameterInsert) -> usize {
        match insert {
            FunctionParameterInsert::Start => self.first_item_index,
            FunctionParameterInsert::End => self.end_item_index,
        }
    }

    pub const fn call_argument_index(&self, insert: FunctionParameterInsert) -> usize {
        match insert {
            FunctionParameterInsert::Start => self.positional_prefix_count,
            FunctionParameterInsert::End => {
                self.positional_prefix_count + self.optional_parameter_count
            }
        }
    }
}

#[derive(Debug)]
pub struct PositionalParameterInsertion {
    pub item_index: usize,
    pub call_argument_index: usize,
}
