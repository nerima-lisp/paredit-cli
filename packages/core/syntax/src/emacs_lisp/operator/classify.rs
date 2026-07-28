use crate::definition::DefinitionCategory;

use super::super::forms::{
    EmacsLispBinderShape, EmacsLispBindingScope, EmacsLispBindingVisibility,
    EmacsLispCallableShape, EmacsLispDefinitionCell, EmacsLispDependencyForm,
    EmacsLispLocalCallableForm,
};
use super::EmacsLispOperator;

use EmacsLispBindingVisibility::{Parallel, Recursive, Sequential};

/// Where the form's binders sit, for every operator that opens a lexical
/// scope.
///
/// `cl-loop` is absent on purpose: its `for`/`with`/`into` clauses are a
/// grammar rather than a layout, and guessing a child index for them would
/// invent bindings. `cl-letf` is absent for the opposite reason — it binds
/// *places*, so `(cl-letf (((symbol-function 'f) #'g)) …)` introduces no
/// lexical name at all.
pub(super) const fn binder_shape(operator: EmacsLispOperator) -> Option<EmacsLispBinderShape> {
    Some(match operator {
        EmacsLispOperator::Let => EmacsLispBinderShape::PairList {
            container: 1,
            body_start: 2,
            body_end: None,
            visibility: Parallel,
        },
        EmacsLispOperator::LetStar | EmacsLispOperator::Dlet => EmacsLispBinderShape::PairList {
            container: 1,
            body_start: 2,
            body_end: None,
            visibility: Sequential,
        },
        EmacsLispOperator::Letrec => EmacsLispBinderShape::PairList {
            container: 1,
            body_start: 2,
            body_end: None,
            visibility: Recursive,
        },
        EmacsLispOperator::ClSymbolMacrolet => EmacsLispBinderShape::PairList {
            container: 1,
            body_start: 2,
            body_end: None,
            visibility: Parallel,
        },
        // `(if-let* (BINDINGS) THEN ELSE…)`. The ELSE forms are siblings of
        // THEN but run only when a binding's value was nil, so none of the
        // bindings is in scope there.
        EmacsLispOperator::IfLet | EmacsLispOperator::IfLetStar => EmacsLispBinderShape::PairList {
            container: 1,
            body_start: 2,
            body_end: Some(3),
            visibility: Sequential,
        },
        EmacsLispOperator::WhenLet
        | EmacsLispOperator::WhenLetStar
        | EmacsLispOperator::AndLetStar
        | EmacsLispOperator::WhileLet => EmacsLispBinderShape::PairList {
            container: 1,
            body_start: 2,
            body_end: None,
            visibility: Sequential,
        },
        EmacsLispOperator::PcaseLet => EmacsLispBinderShape::PatternPairList {
            container: 1,
            body_start: 2,
            visibility: Parallel,
        },
        EmacsLispOperator::PcaseLetStar => EmacsLispBinderShape::PatternPairList {
            container: 1,
            body_start: 2,
            visibility: Sequential,
        },
        EmacsLispOperator::SeqLet => EmacsLispBinderShape::SinglePattern {
            pattern: 1,
            value: 2,
            body_start: 3,
        },
        EmacsLispOperator::ClDestructuringBind | EmacsLispOperator::ClMultipleValueBind => {
            EmacsLispBinderShape::ArgumentList {
                arglist: 1,
                value: 2,
                body_start: 3,
            }
        }
        EmacsLispOperator::Dolist
        | EmacsLispOperator::ClDolist
        | EmacsLispOperator::Dotimes
        | EmacsLispOperator::ClDotimes => EmacsLispBinderShape::Iteration {
            spec: 1,
            body_start: 2,
            has_result_form: true,
        },
        EmacsLispOperator::PcaseDolist => EmacsLispBinderShape::PatternIteration {
            spec: 1,
            body_start: 2,
        },
        EmacsLispOperator::ClDo => EmacsLispBinderShape::VariableSpecs {
            container: 1,
            body_start: 3,
            visibility: Parallel,
        },
        EmacsLispOperator::ClDoStar => EmacsLispBinderShape::VariableSpecs {
            container: 1,
            body_start: 3,
            visibility: Sequential,
        },
        EmacsLispOperator::NamedLet => EmacsLispBinderShape::NamedLet {
            name: 1,
            container: 2,
            body_start: 3,
        },
        EmacsLispOperator::ClFlet
        | EmacsLispOperator::ClFletStar
        | EmacsLispOperator::ClLabels
        | EmacsLispOperator::ClMacrolet
        | EmacsLispOperator::Flet
        | EmacsLispOperator::Labels => EmacsLispBinderShape::LocalCallables {
            container: 1,
            body_start: 2,
            form: match local_callable_form(operator) {
                Some(form) => form,
                // Unreachable: every arm above has a local-callable form.
                None => return None,
            },
        },
        EmacsLispOperator::ConditionCase | EmacsLispOperator::ConditionCaseUnlessDebug => {
            EmacsLispBinderShape::ConditionCase {
                variable: 1,
                protected: 2,
                first_handler: 3,
            }
        }
        EmacsLispOperator::WithSlots => EmacsLispBinderShape::Slots {
            container: 1,
            object: 2,
            body_start: 3,
        },
        EmacsLispOperator::Lambda => EmacsLispBinderShape::Parameters {
            arglist: 1,
            body_start: 2,
        },
        // `(closure ENV ARGS BODY…)`: the byte-compiler's own spelling of a
        // lexical closure, which appears in macro-expanded sources.
        EmacsLispOperator::Closure => EmacsLispBinderShape::Parameters {
            arglist: 2,
            body_start: 3,
        },
        _ => return None,
    })
}

pub(super) const fn local_callable_form(
    operator: EmacsLispOperator,
) -> Option<EmacsLispLocalCallableForm> {
    Some(match operator {
        EmacsLispOperator::ClFlet => EmacsLispLocalCallableForm::Flet,
        EmacsLispOperator::ClFletStar => EmacsLispLocalCallableForm::FletStar,
        EmacsLispOperator::ClLabels => EmacsLispLocalCallableForm::Labels,
        EmacsLispOperator::ClMacrolet => EmacsLispLocalCallableForm::Macrolet,
        EmacsLispOperator::Flet => EmacsLispLocalCallableForm::ObsoleteDynamicFlet,
        EmacsLispOperator::Labels => EmacsLispLocalCallableForm::ObsoleteDynamicLabels,
        _ => return None,
    })
}

/// Whether the names a `let`-shaped form binds are lexical or dynamic.
pub(super) const fn binding_scope(operator: EmacsLispOperator) -> EmacsLispBindingScope {
    match operator {
        // `dlet` exists precisely to bind dynamically in a file that is
        // otherwise lexically bound, and `flet`/`labels` from the old `cl.el`
        // rebound the function cell for the body's dynamic extent.
        EmacsLispOperator::Dlet
        | EmacsLispOperator::Flet
        | EmacsLispOperator::Labels
        | EmacsLispOperator::ClLetf
        | EmacsLispOperator::ClLetfStar => EmacsLispBindingScope::AlwaysDynamic,
        // `cl-lib`'s local callables are lexical by construction: they expand
        // to `letrec`-style closures rather than touching a function cell.
        EmacsLispOperator::ClFlet
        | EmacsLispOperator::ClFletStar
        | EmacsLispOperator::ClLabels
        | EmacsLispOperator::ClMacrolet => EmacsLispBindingScope::AlwaysLexical,
        // Everything else follows the file's `lexical-binding` header, which
        // is the whole reason that header is worth reporting on.
        _ => EmacsLispBindingScope::FileDefault,
    }
}

/// Where a definition's lambda list sits, for the forms that have one at a
/// fixed child index.
///
/// `cl-defmethod` is absent: an optional qualifier (`:around`, `:before`) may
/// precede its arglist, so the position is found by scanning rather than
/// known. See [`EmacsLispOperator::definition_arglist_is_first_list_at_or_after`].
pub(super) const fn callable_shape(operator: EmacsLispOperator) -> Option<EmacsLispCallableShape> {
    Some(match operator {
        // `(defun NAME ARGLIST [DOC] [(declare …)] [(interactive …)] BODY…)`
        EmacsLispOperator::Defun
        | EmacsLispOperator::Defsubst
        | EmacsLispOperator::ClDefun
        | EmacsLispOperator::ClDefsubst => EmacsLispCallableShape::new(2, true, true, true),
        // A macro cannot be a command, so a leading `(interactive …)` in one
        // is an ordinary call rather than a command specification.
        EmacsLispOperator::Defmacro
        | EmacsLispOperator::ClDefmacro
        | EmacsLispOperator::ClDefineCompilerMacro
        | EmacsLispOperator::DefineInline => EmacsLispCallableShape::new(2, true, true, false),
        EmacsLispOperator::ClDefgeneric => EmacsLispCallableShape::new(2, true, true, false),
        // `(ert-deftest NAME () [DOC] [:tags …] BODY…)`: the arglist is
        // required to be empty, but it occupies the same slot.
        EmacsLispOperator::ErtDeftest => EmacsLispCallableShape::new(2, true, true, false),
        EmacsLispOperator::Lambda => EmacsLispCallableShape::new(1, true, true, true),
        _ => return None,
    })
}

pub(super) const fn definition_cell(
    operator: EmacsLispOperator,
) -> Option<EmacsLispDefinitionCell> {
    Some(match operator {
        EmacsLispOperator::Defun
        | EmacsLispOperator::Defsubst
        | EmacsLispOperator::ClDefun
        | EmacsLispOperator::ClDefsubst
        | EmacsLispOperator::DefineInline
        | EmacsLispOperator::ClDefgeneric
        | EmacsLispOperator::ClDefmethod
        | EmacsLispOperator::Defalias
        | EmacsLispOperator::DefineObsoleteFunctionAlias
        | EmacsLispOperator::Defadvice
        | EmacsLispOperator::DefineAdvice
        | EmacsLispOperator::ErtDeftest => EmacsLispDefinitionCell::Function,
        EmacsLispOperator::Defmacro
        | EmacsLispOperator::ClDefmacro
        | EmacsLispOperator::ClDefineCompilerMacro => EmacsLispDefinitionCell::Macro,
        EmacsLispOperator::Defvar
        | EmacsLispOperator::DefvarLocal
        | EmacsLispOperator::DefvarKeymap
        | EmacsLispOperator::Defconst
        | EmacsLispOperator::Defvaralias
        | EmacsLispOperator::DefineObsoleteVariableAlias => EmacsLispDefinitionCell::Variable,
        EmacsLispOperator::Defcustom
        | EmacsLispOperator::Defface
        | EmacsLispOperator::Defgroup
        | EmacsLispOperator::Deftheme
        | EmacsLispOperator::DefineWidget => EmacsLispDefinitionCell::Customization,
        EmacsLispOperator::ClDefstruct
        | EmacsLispOperator::ClDeftype
        | EmacsLispOperator::Defclass => EmacsLispDefinitionCell::Type,
        EmacsLispOperator::DefineError => EmacsLispDefinitionCell::Condition,
        EmacsLispOperator::DefineMinorMode
        | EmacsLispOperator::DefineDerivedMode
        | EmacsLispOperator::DefineGlobalizedMinorMode
        | EmacsLispOperator::DefineGlobalMinorMode
        | EmacsLispOperator::DefineGenericMode
        | EmacsLispOperator::DefineCompilationMode => EmacsLispDefinitionCell::Mode,
        _ => return None,
    })
}

pub(super) const fn definition_category(operator: EmacsLispOperator) -> Option<DefinitionCategory> {
    Some(match operator {
        EmacsLispOperator::Defun
        | EmacsLispOperator::Defsubst
        | EmacsLispOperator::ClDefun
        | EmacsLispOperator::ClDefsubst
        | EmacsLispOperator::DefineInline
        | EmacsLispOperator::Defalias
        | EmacsLispOperator::DefineObsoleteFunctionAlias
        | EmacsLispOperator::Defadvice
        | EmacsLispOperator::DefineAdvice => DefinitionCategory::Function,
        EmacsLispOperator::Defmacro
        | EmacsLispOperator::ClDefmacro
        | EmacsLispOperator::ClDefineCompilerMacro => DefinitionCategory::Macro,
        EmacsLispOperator::ClDefgeneric => DefinitionCategory::GenericFunction,
        EmacsLispOperator::ClDefmethod => DefinitionCategory::Method,
        EmacsLispOperator::Defclass => DefinitionCategory::Class,
        EmacsLispOperator::ClDefstruct | EmacsLispOperator::ClDeftype => DefinitionCategory::Struct,
        EmacsLispOperator::DefineError => DefinitionCategory::Condition,
        EmacsLispOperator::Defvar
        | EmacsLispOperator::DefvarLocal
        | EmacsLispOperator::DefvarKeymap
        | EmacsLispOperator::Defvaralias
        | EmacsLispOperator::DefineObsoleteVariableAlias => DefinitionCategory::Variable,
        EmacsLispOperator::Defconst => DefinitionCategory::Constant,
        EmacsLispOperator::Defcustom
        | EmacsLispOperator::Defface
        | EmacsLispOperator::Defgroup
        | EmacsLispOperator::Deftheme
        | EmacsLispOperator::DefineWidget => DefinitionCategory::Customization,
        EmacsLispOperator::DefineMinorMode
        | EmacsLispOperator::DefineDerivedMode
        | EmacsLispOperator::DefineGlobalizedMinorMode
        | EmacsLispOperator::DefineGlobalMinorMode
        | EmacsLispOperator::DefineGenericMode
        | EmacsLispOperator::DefineCompilationMode => DefinitionCategory::Mode,
        EmacsLispOperator::ErtDeftest => DefinitionCategory::Test,
        EmacsLispOperator::Require
        | EmacsLispOperator::Provide
        | EmacsLispOperator::DefinePackage => DefinitionCategory::Package,
        _ => return None,
    })
}

pub(super) const fn dependency_form(
    operator: EmacsLispOperator,
) -> Option<EmacsLispDependencyForm> {
    Some(match operator {
        EmacsLispOperator::Require => EmacsLispDependencyForm::Require,
        EmacsLispOperator::Provide => EmacsLispDependencyForm::Provide,
        EmacsLispOperator::Load => EmacsLispDependencyForm::Load,
        EmacsLispOperator::LoadFile => EmacsLispDependencyForm::LoadFile,
        EmacsLispOperator::LoadLibrary => EmacsLispDependencyForm::LoadLibrary,
        EmacsLispOperator::Autoload => EmacsLispDependencyForm::Autoload,
        EmacsLispOperator::DeclareFunction => EmacsLispDependencyForm::DeclareFunction,
        EmacsLispOperator::DefinePackage => EmacsLispDependencyForm::DefinePackage,
        _ => return None,
    })
}
