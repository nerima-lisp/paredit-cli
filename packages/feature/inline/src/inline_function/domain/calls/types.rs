#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::domain::inline_function) struct InlineFunctionCall {
    pub(in crate::domain::inline_function) raw_args: Vec<String>,
    pub(in crate::domain::inline_function) whole_call: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::domain::inline_function) struct InlineArgumentBindings {
    pub(in crate::domain::inline_function) body_bindings: Vec<(String, String)>,
    pub(in crate::domain::inline_function) argument_bindings: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallSideAllowOtherKeys {
    AbsentOrFalse,
    True,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParameterBinding {
    pub body_entries: Vec<(String, String)>,
    pub argument_entries: Vec<(String, String)>,
    pub default_scope_entries: Vec<(String, String)>,
}
