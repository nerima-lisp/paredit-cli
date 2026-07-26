use crate::definition::DefinitionCategory;

use super::super::{
    CommonLispLambdaListShape, CommonLispPackageDeclarationForm, CommonLispRuntimeDependencyForm,
};
use super::{CommonLispOperator, classify};

impl CommonLispOperator {
    #[must_use]
    pub const fn is_lambda_like(self) -> bool {
        matches!(self, Self::Lambda | Self::Fn)
    }

    #[must_use]
    pub const fn is_defun_like(self) -> bool {
        matches!(
            self,
            Self::Defun
                | Self::Defmacro
                | Self::DefineMethodCombination
                | Self::DefineSetfExpander
                | Self::DefineCompilerMacro
                | Self::DefineModifyMacro
                | Self::Defsetf
        )
    }

    #[must_use]
    pub const fn is_inline_function_definition(self) -> bool {
        matches!(
            self,
            Self::Defun | Self::Defmacro | Self::DefineCompilerMacro
        )
    }

    #[must_use]
    pub const fn is_method_definition(self) -> bool {
        matches!(self, Self::Defmethod | Self::ClDefmethod)
    }

    #[must_use]
    pub const fn supports_function_parameter_refactor(self) -> bool {
        matches!(
            self,
            Self::Defun
                | Self::Defmacro
                | Self::DefineMethodCombination
                | Self::DefineSetfExpander
                | Self::DefineCompilerMacro
                | Self::DefineModifyMacro
                | Self::Defsetf
                | Self::Defmethod
                | Self::ClDefmethod
                | Self::Defgeneric
        )
    }

    #[must_use]
    pub const fn definition_category(self) -> Option<DefinitionCategory> {
        classify::definition_category(self)
    }

    #[must_use]
    pub const fn definition_lambda_list_shape(self) -> Option<CommonLispLambdaListShape> {
        classify::definition_lambda_list_shape(self)
    }

    #[must_use]
    pub const fn runtime_dependency_form(self) -> Option<CommonLispRuntimeDependencyForm> {
        classify::runtime_dependency_form(self)
    }

    #[must_use]
    pub const fn package_declaration_form(self) -> Option<CommonLispPackageDeclarationForm> {
        classify::package_declaration_form(self)
    }

    #[must_use]
    pub fn is_asdf_system_definition(self) -> bool {
        self == Self::Defsystem
    }
}
