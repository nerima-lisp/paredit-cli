use crate::definition::DefinitionCategory;

use super::super::forms::{
    SchemeBindingForm, SchemeBindingNamespace, SchemeDefinitionForm, SchemeLetKind,
    SchemeLibraryDeclaration,
};
use super::SchemeOperator;

pub(super) const fn binding_form(operator: SchemeOperator) -> Option<SchemeBindingForm> {
    Some(match operator {
        SchemeOperator::Let => SchemeBindingForm::Let {
            kind: SchemeLetKind::Parallel,
            namespace: SchemeBindingNamespace::Value,
        },
        SchemeOperator::LetStar => SchemeBindingForm::Let {
            kind: SchemeLetKind::Sequential,
            namespace: SchemeBindingNamespace::Value,
        },
        // R7RS 4.2.2 separates `letrec` from `letrec*` by whether the
        // initializers are evaluated in an unspecified order or left to right.
        // Both make the whole group visible to every initializer, and
        // visibility is the only thing a scope table records, so they share a
        // kind here.
        SchemeOperator::Letrec | SchemeOperator::LetrecStar => SchemeBindingForm::Let {
            kind: SchemeLetKind::Recursive,
            namespace: SchemeBindingNamespace::Value,
        },
        SchemeOperator::LetSyntax => SchemeBindingForm::Let {
            kind: SchemeLetKind::Parallel,
            namespace: SchemeBindingNamespace::Syntax,
        },
        SchemeOperator::LetrecSyntax => SchemeBindingForm::Let {
            kind: SchemeLetKind::Recursive,
            namespace: SchemeBindingNamespace::Syntax,
        },
        SchemeOperator::LetValues => SchemeBindingForm::LetValues(SchemeLetKind::Parallel),
        SchemeOperator::LetStarValues => SchemeBindingForm::LetValues(SchemeLetKind::Sequential),
        SchemeOperator::LetrecValues => SchemeBindingForm::LetValues(SchemeLetKind::Recursive),
        SchemeOperator::Do => SchemeBindingForm::Do,
        SchemeOperator::Lambda => SchemeBindingForm::Lambda,
        SchemeOperator::CaseLambda => SchemeBindingForm::CaseLambda,
        SchemeOperator::Guard => SchemeBindingForm::Guard,
        SchemeOperator::Parameterize | SchemeOperator::FluidLet => {
            SchemeBindingForm::DynamicBinding
        }
        _ => return None,
    })
}

pub(super) const fn definition_form(operator: SchemeOperator) -> Option<SchemeDefinitionForm> {
    Some(match operator {
        SchemeOperator::Define => SchemeDefinitionForm::Define,
        SchemeOperator::DefineValues => SchemeDefinitionForm::DefineValues,
        SchemeOperator::DefineRecordType => SchemeDefinitionForm::DefineRecordType,
        SchemeOperator::DefineSyntax => SchemeDefinitionForm::DefineSyntax,
        SchemeOperator::DefineSyntaxRule => SchemeDefinitionForm::DefineSyntaxRule,
        SchemeOperator::DefineLibrary => SchemeDefinitionForm::DefineLibrary,
        SchemeOperator::Struct => SchemeDefinitionForm::Struct,
        SchemeOperator::DefineStruct => SchemeDefinitionForm::DefineStruct,
        SchemeOperator::DefineContract => SchemeDefinitionForm::DefineContract,
        _ => return None,
    })
}

pub(super) const fn definition_category(operator: SchemeOperator) -> Option<DefinitionCategory> {
    Some(match operator {
        // `(define x 1)` and `(define (f) 1)` share a head, so the category a
        // `define` really has depends on the form's own shape. Variable is the
        // conservative reading; `dialect::semantic` refines it by inspecting
        // child 1.
        SchemeOperator::Define | SchemeOperator::DefineValues | SchemeOperator::DefineContract => {
            DefinitionCategory::Variable
        }
        SchemeOperator::DefineSyntax
        | SchemeOperator::DefineSyntaxRule
        | SchemeOperator::LetSyntax
        | SchemeOperator::LetrecSyntax => DefinitionCategory::Macro,
        SchemeOperator::DefineRecordType
        | SchemeOperator::Struct
        | SchemeOperator::DefineStruct => DefinitionCategory::Struct,
        SchemeOperator::DefineLibrary => DefinitionCategory::Package,
        _ => return None,
    })
}

pub(super) const fn library_declaration(
    operator: SchemeOperator,
) -> Option<SchemeLibraryDeclaration> {
    Some(match operator {
        SchemeOperator::Export => SchemeLibraryDeclaration::Export,
        SchemeOperator::Import => SchemeLibraryDeclaration::Import,
        SchemeOperator::Begin => SchemeLibraryDeclaration::Begin,
        SchemeOperator::Include => SchemeLibraryDeclaration::Include,
        SchemeOperator::IncludeCi => SchemeLibraryDeclaration::IncludeCi,
        SchemeOperator::IncludeLibraryDeclarations => {
            SchemeLibraryDeclaration::IncludeLibraryDeclarations
        }
        SchemeOperator::CondExpand => SchemeLibraryDeclaration::CondExpand,
        _ => return None,
    })
}

/// Whether the operator's body is ordinary evaluated code the walk can descend
/// into without special handling.
///
/// The point of this predicate is the *negative* case. A form that binds, or
/// that quotes its arguments, must not be walked as a plain call, and an
/// unrecognised head must be treated as opaque because a macro may do either.
pub(super) const fn has_transparent_body(operator: SchemeOperator) -> bool {
    matches!(
        operator,
        SchemeOperator::Begin
            | SchemeOperator::When
            | SchemeOperator::Unless
            | SchemeOperator::Cond
            | SchemeOperator::Case
            | SchemeOperator::If
            | SchemeOperator::And
            | SchemeOperator::Or
            | SchemeOperator::Delay
            | SchemeOperator::DelayForce
            | SchemeOperator::MakePromise
            | SchemeOperator::Set
    )
}
