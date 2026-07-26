use super::super::{
    CommonLispBindingRefactorForm, CommonLispHandlerBindingForm, CommonLispLetBindingForm,
    CommonLispLocalCallableForm, CommonLispResourceBindingForm, CommonLispSlotBindingForm,
    CommonLispValueScopeForm, CommonLispVariableBindingForm,
};
use super::{CommonLispOperator, classify};

impl CommonLispOperator {
    #[must_use]
    pub const fn is_parallel_let_binding(self) -> bool {
        matches!(self, Self::Let | Self::SymbolMacrolet)
    }

    #[must_use]
    pub fn is_sequential_let_binding(self) -> bool {
        self == Self::LetStar
    }

    #[must_use]
    pub fn is_let_binding(self) -> bool {
        self.is_parallel_let_binding() || self.is_sequential_let_binding()
    }

    #[must_use]
    pub const fn let_binding_form(self) -> Option<CommonLispLetBindingForm> {
        match self {
            Self::Let => Some(CommonLispLetBindingForm::Parallel),
            Self::LetStar => Some(CommonLispLetBindingForm::Sequential),
            Self::SymbolMacrolet => Some(CommonLispLetBindingForm::SymbolMacro),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_value_binding(self) -> bool {
        matches!(self, Self::DestructuringBind | Self::MultipleValueBind)
    }

    #[must_use]
    pub const fn is_clause_binding(self) -> bool {
        matches!(self, Self::HandlerCase | Self::RestartCase)
    }

    #[must_use]
    pub const fn is_handler_bind_binding(self) -> bool {
        matches!(self, Self::HandlerBind | Self::RestartBind)
    }

    #[must_use]
    pub fn includes_restart_bind_options(self) -> bool {
        self == Self::RestartBind
    }

    #[must_use]
    pub const fn handler_binding_form(self) -> Option<CommonLispHandlerBindingForm> {
        match self {
            Self::HandlerBind => Some(CommonLispHandlerBindingForm::Handler),
            Self::RestartBind => Some(CommonLispHandlerBindingForm::Restart),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_iteration_binding(self) -> bool {
        matches!(self, Self::Dolist | Self::Dotimes)
    }

    #[must_use]
    pub const fn is_do_binding(self) -> bool {
        matches!(self, Self::Do | Self::DoStar)
    }

    #[must_use]
    pub const fn is_prog_binding(self) -> bool {
        matches!(self, Self::Prog | Self::ProgStar)
    }

    #[must_use]
    pub const fn is_sequential_variable_binding(self) -> bool {
        matches!(self, Self::DoStar | Self::ProgStar)
    }

    #[must_use]
    pub const fn variable_binding_form(self) -> Option<CommonLispVariableBindingForm> {
        match self {
            Self::Do | Self::Prog => Some(CommonLispVariableBindingForm::Parallel),
            Self::DoStar | Self::ProgStar => Some(CommonLispVariableBindingForm::Sequential),
            _ => None,
        }
    }

    #[must_use]
    pub const fn has_variable_step_forms(self) -> bool {
        self.is_do_binding()
    }

    #[must_use]
    pub const fn value_scope_form(self) -> Option<CommonLispValueScopeForm> {
        if let Some(form) = self.let_binding_form() {
            return Some(CommonLispValueScopeForm::Let(form));
        }
        if let Some(form) = self.variable_binding_form() {
            return Some(CommonLispValueScopeForm::Variable(form));
        }
        if let Some(form) = self.local_callable_form() {
            return Some(CommonLispValueScopeForm::LocalCallable(form));
        }
        if let Some(form) = self.handler_binding_form() {
            return Some(CommonLispValueScopeForm::Handler(form));
        }
        if let Some(form) = self.resource_binding_form() {
            return Some(CommonLispValueScopeForm::Resource(form));
        }

        match self {
            Self::Lambda => Some(CommonLispValueScopeForm::Lambda),
            Self::Fn => Some(CommonLispValueScopeForm::FunctionLiteral),
            operator if operator.is_defun_like() => Some(CommonLispValueScopeForm::Definition),
            operator if operator.is_value_binding() => Some(CommonLispValueScopeForm::Value),
            operator if operator.is_clause_binding() => Some(CommonLispValueScopeForm::Clause),
            operator if operator.is_iteration_binding() => {
                Some(CommonLispValueScopeForm::Iteration)
            }
            operator if operator.is_slot_binding() => Some(CommonLispValueScopeForm::Slot),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_slot_binding(self) -> bool {
        matches!(self, Self::WithSlots | Self::WithAccessors)
    }

    #[must_use]
    pub const fn resource_binding_form(self) -> Option<CommonLispResourceBindingForm> {
        match self {
            Self::WithOpenFile => Some(CommonLispResourceBindingForm::OpenFile),
            Self::WithOpenStream => Some(CommonLispResourceBindingForm::OpenStream),
            Self::WithInputFromString => Some(CommonLispResourceBindingForm::InputFromString),
            Self::WithOutputToString => Some(CommonLispResourceBindingForm::OutputToString),
            _ => None,
        }
    }

    #[must_use]
    pub const fn slot_binding_form(self) -> Option<CommonLispSlotBindingForm> {
        match self {
            Self::WithSlots => Some(CommonLispSlotBindingForm::WithSlots),
            Self::WithAccessors => Some(CommonLispSlotBindingForm::WithAccessors),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_local_callable_binding(self) -> bool {
        matches!(
            self,
            Self::Flet | Self::Labels | Self::Macrolet | Self::CompilerMacrolet
        )
    }

    #[must_use]
    pub const fn local_callable_form(self) -> Option<CommonLispLocalCallableForm> {
        classify::local_callable_form(self)
    }

    #[must_use]
    pub fn binding_refactor_form(self) -> Option<CommonLispBindingRefactorForm> {
        if let Some(form) = self.let_binding_form() {
            return Some(CommonLispBindingRefactorForm::Let(form));
        }
        if let Some(form) = self.local_callable_form() {
            return Some(CommonLispBindingRefactorForm::LocalCallable(form));
        }
        if let Some(form) = self.handler_binding_form() {
            return Some(CommonLispBindingRefactorForm::Handler(form));
        }
        if let Some(form) = self.slot_binding_form() {
            return Some(CommonLispBindingRefactorForm::Slot(form));
        }

        match self {
            Self::DestructuringBind | Self::MultipleValueBind => {
                Some(CommonLispBindingRefactorForm::Value)
            }
            operator if operator.is_lambda_like() => {
                Some(CommonLispBindingRefactorForm::LambdaLike)
            }
            operator if operator.is_method_definition() => {
                Some(CommonLispBindingRefactorForm::MethodDefinition)
            }
            operator if operator.is_defun_like() => {
                Some(CommonLispBindingRefactorForm::FunctionDefinition)
            }
            Self::HandlerCase | Self::RestartCase => Some(CommonLispBindingRefactorForm::Clause),
            Self::Dolist | Self::Dotimes => Some(CommonLispBindingRefactorForm::Iteration),
            Self::Loop => Some(CommonLispBindingRefactorForm::Loop),
            Self::Do | Self::DoStar => Some(CommonLispBindingRefactorForm::Do(
                self.variable_binding_form()?,
            )),
            Self::Prog | Self::ProgStar => Some(CommonLispBindingRefactorForm::Prog(
                self.variable_binding_form()?,
            )),
            _ => None,
        }
    }
}
