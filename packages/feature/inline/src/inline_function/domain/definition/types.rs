use crate::domain::sexpr::{ExpressionView, SymbolName};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InlineDefinitionKind {
    Function,
    Macro,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InlineDefinition {
    pub name: SymbolName,
    pub params: Vec<InlineParameter>,
    pub body: ExpressionView,
    pub kind: InlineDefinitionKind,
    pub accepts_other_keys: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InlineParameterKind {
    Positional { optional: bool },
    Keyword { keyword: String },
    Rest,
    Whole,
    Environment,
    Aux,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InlineParameter {
    pub binding: InlineParameterBinding,
    pub kind: InlineParameterKind,
    pub default_value: Option<String>,
    pub supplied_p: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InlineParameterBinding {
    Name(String),
    Destructure(InlineDestructurePattern),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InlineDestructurePattern {
    Name(String),
    List(InlineDestructureListPattern),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InlineDestructureListPattern {
    pub whole: Option<String>,
    pub required: Vec<InlineDestructurePattern>,
    pub optional: Vec<InlineDestructureOptionalPattern>,
    pub rest: Option<Box<InlineDestructurePattern>>,
    pub keys: Vec<InlineDestructureKeyPattern>,
    pub aux: Vec<InlineParameter>,
    pub allow_other_keys: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InlineDestructureOptionalPattern {
    pub binding: InlineDestructurePattern,
    pub default_value: Option<String>,
    pub supplied_p: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InlineDestructureKeyPattern {
    pub binding: InlineDestructurePattern,
    pub keyword: String,
    pub default_value: Option<String>,
    pub supplied_p: Option<String>,
}

impl InlineParameter {
    pub fn primary_name(&self) -> Option<&str> {
        match &self.binding {
            InlineParameterBinding::Name(name) => Some(name),
            InlineParameterBinding::Destructure(_) => None,
        }
    }

    pub fn binding_names(&self) -> Vec<String> {
        match &self.binding {
            InlineParameterBinding::Name(name) => vec![name.clone()],
            InlineParameterBinding::Destructure(pattern) => pattern.binding_names(),
        }
    }
}

impl InlineDestructurePattern {
    pub fn binding_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        self.collect_binding_names(&mut names);
        names
    }

    fn collect_binding_names(&self, output: &mut Vec<String>) {
        match self {
            Self::Name(name) => output.push(name.clone()),
            Self::List(items) => {
                if let Some(name) = &items.whole {
                    output.push(name.clone());
                }
                for item in &items.required {
                    item.collect_binding_names(output);
                }
                for item in &items.optional {
                    item.binding.collect_binding_names(output);
                    if let Some(name) = &item.supplied_p {
                        output.push(name.clone());
                    }
                }
                if let Some(item) = &items.rest {
                    item.collect_binding_names(output);
                }
                for item in &items.keys {
                    item.binding.collect_binding_names(output);
                    if let Some(name) = &item.supplied_p {
                        output.push(name.clone());
                    }
                }
                for item in &items.aux {
                    output.extend(item.binding_names());
                }
            }
        }
    }
}
