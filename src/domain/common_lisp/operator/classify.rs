use crate::domain::definition::DefinitionCategory;

use super::super::{
    CommonLispLambdaListShape, CommonLispLocalCallableForm, CommonLispPackageDeclarationForm,
    CommonLispRuntimeDependencyForm,
};
use super::CommonLispOperator;

pub(super) const fn definition_category(operator: CommonLispOperator) -> Option<DefinitionCategory> {
    Some(match operator {
        CommonLispOperator::Defun => DefinitionCategory::Function,
        CommonLispOperator::Defmacro
        | CommonLispOperator::DefineMethodCombination
        | CommonLispOperator::DefineCompilerMacro
        | CommonLispOperator::DefineModifyMacro
        | CommonLispOperator::DefineSetfExpander
        | CommonLispOperator::Defsetf => DefinitionCategory::Macro,
        CommonLispOperator::Defgeneric => DefinitionCategory::GenericFunction,
        CommonLispOperator::Defmethod | CommonLispOperator::ClDefmethod => {
            DefinitionCategory::Method
        }
        CommonLispOperator::Defclass => DefinitionCategory::Class,
        CommonLispOperator::Defstruct | CommonLispOperator::Deftype => DefinitionCategory::Struct,
        CommonLispOperator::DefineCondition => DefinitionCategory::Condition,
        CommonLispOperator::Defvar
        | CommonLispOperator::Defglobal
        | CommonLispOperator::DefineSymbolMacro => DefinitionCategory::Variable,
        CommonLispOperator::Defconstant => DefinitionCategory::Constant,
        CommonLispOperator::Defparameter => DefinitionCategory::Parameter,
        CommonLispOperator::Defpackage
        | CommonLispOperator::InPackage
        | CommonLispOperator::Provide
        | CommonLispOperator::Require
        | CommonLispOperator::UsePackage
        | CommonLispOperator::Import => DefinitionCategory::Package,
        CommonLispOperator::Defsystem => DefinitionCategory::System,
        _ => return None,
    })
}

pub(super) const fn definition_lambda_list_shape(
    operator: CommonLispOperator,
) -> Option<CommonLispLambdaListShape> {
    Some(match operator {
        CommonLispOperator::Defun
        | CommonLispOperator::Defmacro
        | CommonLispOperator::DefineMethodCombination
        | CommonLispOperator::DefineCompilerMacro
        | CommonLispOperator::DefineModifyMacro
        | CommonLispOperator::DefineSetfExpander
        | CommonLispOperator::Defsetf
        | CommonLispOperator::Defgeneric => CommonLispLambdaListShape::ChildAt(2),
        CommonLispOperator::Defmethod | CommonLispOperator::ClDefmethod => {
            CommonLispLambdaListShape::FirstListAtOrAfter(2)
        }
        _ => return None,
    })
}

pub(super) const fn runtime_dependency_form(
    operator: CommonLispOperator,
) -> Option<CommonLispRuntimeDependencyForm> {
    match operator {
        CommonLispOperator::Require => Some(CommonLispRuntimeDependencyForm::Require),
        CommonLispOperator::Provide => Some(CommonLispRuntimeDependencyForm::Provide),
        CommonLispOperator::Load => Some(CommonLispRuntimeDependencyForm::Load),
        CommonLispOperator::LoadFile => Some(CommonLispRuntimeDependencyForm::LoadFile),
        CommonLispOperator::LoadLibrary => Some(CommonLispRuntimeDependencyForm::LoadLibrary),
        CommonLispOperator::UsePackage => Some(CommonLispRuntimeDependencyForm::UsePackage),
        CommonLispOperator::Import => Some(CommonLispRuntimeDependencyForm::Import),
        _ => None,
    }
}

pub(super) const fn package_declaration_form(
    operator: CommonLispOperator,
) -> Option<CommonLispPackageDeclarationForm> {
    match operator {
        CommonLispOperator::Defpackage => Some(CommonLispPackageDeclarationForm::Defpackage),
        CommonLispOperator::InPackage => Some(CommonLispPackageDeclarationForm::InPackage),
        _ => None,
    }
}

pub(super) const fn local_callable_form(
    operator: CommonLispOperator,
) -> Option<CommonLispLocalCallableForm> {
    match operator {
        CommonLispOperator::Flet => Some(CommonLispLocalCallableForm::Flet),
        CommonLispOperator::Labels => Some(CommonLispLocalCallableForm::Labels),
        CommonLispOperator::Macrolet => Some(CommonLispLocalCallableForm::Macrolet),
        CommonLispOperator::CompilerMacrolet => Some(CommonLispLocalCallableForm::CompilerMacrolet),
        _ => None,
    }
}
