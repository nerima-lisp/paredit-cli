#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineFunctionCall {
    pub raw_args: Vec<String>,
    pub whole_call: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineArgumentBindings {
    pub body_bindings: Vec<(String, String)>,
    pub argument_bindings: Vec<(String, String)>,
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
