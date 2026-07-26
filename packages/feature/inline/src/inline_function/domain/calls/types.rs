#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::inline_function::domain) struct InlineFunctionCall {
    pub(in crate::inline_function::domain) raw_args: Vec<String>,
    pub(in crate::inline_function::domain) whole_call: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::inline_function::domain) struct InlineArgumentBindings {
    pub(in crate::inline_function::domain) body_bindings: Vec<(String, String)>,
    pub(in crate::inline_function::domain) argument_bindings: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallSideAllowOtherKeys {
    AbsentOrFalse,
    True,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterBinding {
    pub body_entries: Vec<(String, String)>,
    pub argument_entries: Vec<(String, String)>,
    pub default_scope_entries: Vec<(String, String)>,
}
