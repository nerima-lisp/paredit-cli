#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispLocalCallableForm {
    Flet,
    Labels,
    Macrolet,
    CompilerMacrolet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispLetBindingForm {
    Parallel,
    Sequential,
    SymbolMacro,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispVariableBindingForm {
    Parallel,
    Sequential,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispHandlerBindingForm {
    Handler,
    Restart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispRuntimeDependencyForm {
    Require,
    Provide,
    Load,
    LoadFile,
    LoadLibrary,
    UsePackage,
    Import,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispPackageDeclarationForm {
    Defpackage,
    InPackage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispValueScopeForm {
    Let(CommonLispLetBindingForm),
    Lambda,
    FunctionLiteral,
    Definition,
    Value,
    Clause,
    Handler(CommonLispHandlerBindingForm),
    Iteration,
    Variable(CommonLispVariableBindingForm),
    Slot,
    Resource(CommonLispResourceBindingForm),
    LocalCallable(CommonLispLocalCallableForm),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispResourceBindingForm {
    OpenFile,
    OpenStream,
    InputFromString,
    OutputToString,
}

impl CommonLispResourceBindingForm {
    #[must_use]
    pub const fn body_start_index(self) -> usize {
        2
    }
}

/// A form body whose leading declarations apply to every following body form.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommonLispDeclarationScope {
    declaration_start_index: usize,
}

impl CommonLispDeclarationScope {
    #[must_use]
    pub const fn new(declaration_start_index: usize) -> Self {
        Self {
            declaration_start_index,
        }
    }

    #[must_use]
    pub const fn declaration_start_index(self) -> usize {
        self.declaration_start_index
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispSlotBindingForm {
    WithSlots,
    WithAccessors,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispBindingRefactorForm {
    Let(CommonLispLetBindingForm),
    Value,
    LambdaLike,
    MethodDefinition,
    FunctionDefinition,
    LocalCallable(CommonLispLocalCallableForm),
    Clause,
    Handler(CommonLispHandlerBindingForm),
    Iteration,
    Loop,
    Do(CommonLispVariableBindingForm),
    Prog(CommonLispVariableBindingForm),
    Slot(CommonLispSlotBindingForm),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispBindingListShape {
    NameValuePairs,
    LocalCallableDefinitions(CommonLispLocalCallableForm),
    VariableSpecs(CommonLispVariableSpecForm),
    SlotBindings(CommonLispSlotBindingForm),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispBindingReferenceScope {
    NameValuePairs(CommonLispLetBindingForm),
    LocalCallableDefinitions(CommonLispLocalCallableForm),
    VariableSpecs(CommonLispVariableSpecForm, CommonLispVariableBindingForm),
    BodyOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispVariableSpecForm {
    Do,
    Prog,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonLispLambdaListShape {
    ChildAt(usize),
    FirstListAtOrAfter(usize),
}

impl CommonLispLocalCallableForm {
    #[must_use]
    pub const fn is_macro(self) -> bool {
        matches!(self, Self::Macrolet | Self::CompilerMacrolet)
    }

    #[must_use]
    pub const fn operator_name(self) -> &'static str {
        match self {
            Self::Flet => "flet",
            Self::Labels => "labels",
            Self::Macrolet => "macrolet",
            Self::CompilerMacrolet => "compiler-macrolet",
        }
    }
}

impl CommonLispLetBindingForm {
    #[must_use]
    pub const fn is_sequential(self) -> bool {
        matches!(self, Self::Sequential)
    }

    #[must_use]
    pub const fn supports_inline_refactor(self) -> bool {
        matches!(self, Self::Parallel | Self::Sequential | Self::SymbolMacro)
    }
}

impl CommonLispVariableBindingForm {
    #[must_use]
    pub const fn is_sequential(self) -> bool {
        matches!(self, Self::Sequential)
    }
}

impl CommonLispHandlerBindingForm {
    #[must_use]
    pub const fn includes_restart_options(self) -> bool {
        matches!(self, Self::Restart)
    }
}

impl CommonLispValueScopeForm {
    /// Returns the first child that may be a body declaration when its index
    /// is fixed by the form syntax. Method definitions are handled by their
    /// parsed lambda-list position instead.
    #[must_use]
    pub const fn declaration_scope(self) -> Option<CommonLispDeclarationScope> {
        match self {
            Self::Let(_) | Self::Lambda | Self::LocalCallable(_) | Self::Resource(_) => {
                Some(CommonLispDeclarationScope::new(2))
            }
            Self::Definition | Self::Value | Self::Slot => Some(CommonLispDeclarationScope::new(3)),
            Self::FunctionLiteral
            | Self::Clause
            | Self::Handler(_)
            | Self::Iteration
            | Self::Variable(_) => None,
        }
    }
}

impl CommonLispBindingRefactorForm {
    /// Whether this form introduces value bindings that can dynamically bind
    /// a variable declared special.
    #[must_use]
    pub const fn supports_dynamic_special_binding(self) -> bool {
        matches!(
            self,
            Self::Let(CommonLispLetBindingForm::Parallel | CommonLispLetBindingForm::Sequential)
                | Self::Do(_)
                | Self::Prog(_)
        )
    }

    #[must_use]
    pub const fn supports_remove_unused_binding(self) -> bool {
        matches!(
            self,
            Self::Let(_) | Self::LocalCallable(_) | Self::Do(_) | Self::Prog(_) | Self::Slot(_)
        )
    }

    #[must_use]
    pub const fn remove_unused_body_start_index(self) -> usize {
        match self {
            Self::Slot(_) | Self::Do(_) => 3,
            _ => 2,
        }
    }

    #[must_use]
    pub const fn preserves_binding_form_when_empty(self) -> bool {
        matches!(self, Self::Do(_) | Self::Prog(_))
    }

    #[must_use]
    pub const fn binding_list_shape(self) -> Option<CommonLispBindingListShape> {
        match self {
            Self::Let(_) => Some(CommonLispBindingListShape::NameValuePairs),
            Self::LocalCallable(form) => {
                Some(CommonLispBindingListShape::LocalCallableDefinitions(form))
            }
            Self::Do(_) => Some(CommonLispBindingListShape::VariableSpecs(
                CommonLispVariableSpecForm::Do,
            )),
            Self::Prog(_) => Some(CommonLispBindingListShape::VariableSpecs(
                CommonLispVariableSpecForm::Prog,
            )),
            Self::Slot(form) => Some(CommonLispBindingListShape::SlotBindings(form)),
            _ => None,
        }
    }

    #[must_use]
    pub const fn reference_scope(self) -> Option<CommonLispBindingReferenceScope> {
        match self {
            Self::Let(form) => Some(CommonLispBindingReferenceScope::NameValuePairs(form)),
            Self::LocalCallable(form) => Some(
                CommonLispBindingReferenceScope::LocalCallableDefinitions(form),
            ),
            Self::Do(form) => Some(CommonLispBindingReferenceScope::VariableSpecs(
                CommonLispVariableSpecForm::Do,
                form,
            )),
            Self::Prog(form) => Some(CommonLispBindingReferenceScope::VariableSpecs(
                CommonLispVariableSpecForm::Prog,
                form,
            )),
            Self::Slot(_) => Some(CommonLispBindingReferenceScope::BodyOnly),
            _ => None,
        }
    }
}

impl CommonLispVariableSpecForm {
    #[must_use]
    pub const fn form_name(self) -> &'static str {
        match self {
            Self::Do => "do",
            Self::Prog => "prog",
        }
    }

    #[must_use]
    pub const fn max_children(self) -> usize {
        match self {
            Self::Do => 3,
            Self::Prog => 2,
        }
    }

    #[must_use]
    pub const fn has_step_forms(self) -> bool {
        matches!(self, Self::Do)
    }

    #[must_use]
    pub const fn end_clause_index(self) -> Option<usize> {
        match self {
            Self::Do => Some(2),
            Self::Prog => None,
        }
    }

    #[must_use]
    pub const fn body_start_index(self) -> usize {
        match self {
            Self::Do => 3,
            Self::Prog => 2,
        }
    }
}
