use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Path, SymbolName};

#[derive(Debug, Clone)]
pub struct AddFunctionParameterRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub definition_path: Path,
    pub name: SymbolName,
    pub argument: String,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
    pub insert: FunctionParameterInsert,
    pub section: FunctionParameterSection,
}

#[derive(Debug, Clone)]
pub struct MoveFunctionParameterRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub definition_path: Path,
    pub name: SymbolName,
    pub to_index: usize,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
}

#[derive(Debug, Clone)]
pub struct SwapFunctionParametersRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub definition_path: Path,
    pub left_name: SymbolName,
    pub right_name: SymbolName,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
}

#[derive(Debug, Clone)]
pub struct ReorderFunctionParametersRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub definition_path: Path,
    pub parameter_order: Vec<SymbolName>,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
}

#[derive(Debug, Clone)]
pub struct RemoveFunctionParameterRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub definition_path: Path,
    pub name: SymbolName,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
    pub missing_argument_policy: MissingArgumentPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingArgumentPolicy {
    Reject,
    Ignore,
}

impl MissingArgumentPolicy {
    #[must_use]
    pub const fn allows_missing_argument(self) -> bool {
        matches!(self, Self::Ignore)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionParameterInsert {
    Start,
    End,
}

impl FunctionParameterInsert {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionParameterSection {
    Auto,
    Positional,
    Optional,
    Keyword,
}

impl FunctionParameterSection {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Positional => "required",
            Self::Optional => "optional",
            Self::Keyword => "keyword",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AddFunctionParameterPlan {
    pub dialect: Dialect,
    pub definition_path: Path,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
    pub definition_span: ByteSpan,
    pub parameter_list_span: ByteSpan,
    pub call_spans: Vec<ByteSpan>,
    pub function_name: SymbolName,
    pub parameter_name: SymbolName,
    pub argument: String,
    pub insert: FunctionParameterInsert,
    pub section: FunctionParameterSection,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct MoveFunctionParameterPlan {
    pub dialect: Dialect,
    pub definition_path: Path,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
    pub definition_span: ByteSpan,
    pub parameter_list_span: ByteSpan,
    pub call_spans: Vec<ByteSpan>,
    pub function_name: SymbolName,
    pub parameter_name: SymbolName,
    pub from_index: usize,
    pub to_index: usize,
    pub moved_arguments: Vec<String>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct SwapFunctionParametersPlan {
    pub dialect: Dialect,
    pub definition_path: Path,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
    pub definition_span: ByteSpan,
    pub parameter_list_span: ByteSpan,
    pub call_spans: Vec<ByteSpan>,
    pub function_name: SymbolName,
    pub left_name: SymbolName,
    pub right_name: SymbolName,
    pub left_index: usize,
    pub right_index: usize,
    pub swapped_arguments: Vec<(String, String)>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct ReorderFunctionParametersPlan {
    pub dialect: Dialect,
    pub definition_path: Path,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
    pub definition_span: ByteSpan,
    pub parameter_list_span: ByteSpan,
    pub call_spans: Vec<ByteSpan>,
    pub function_name: SymbolName,
    pub old_parameter_order: Vec<SymbolName>,
    pub new_parameter_order: Vec<SymbolName>,
    pub reordered_arguments: Vec<Vec<String>>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct RemoveFunctionParameterPlan {
    pub dialect: Dialect,
    pub definition_path: Path,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
    pub definition_span: ByteSpan,
    pub parameter_list_span: ByteSpan,
    pub call_spans: Vec<ByteSpan>,
    pub function_name: SymbolName,
    pub parameter_name: SymbolName,
    pub parameter_index: usize,
    pub parameter_keyword: Option<String>,
    pub removed_arguments: Vec<Option<String>>,
    pub rewritten: String,
    pub changed: bool,
}
