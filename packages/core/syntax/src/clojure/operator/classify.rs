use crate::definition::DefinitionCategory;

use super::{ClojureBindingForm, ClojureOperator, ClojureThreadingForm};

pub(super) const fn definition_category(operator: ClojureOperator) -> Option<DefinitionCategory> {
    Some(match operator {
        ClojureOperator::Ns | ClojureOperator::InNs => DefinitionCategory::Package,
        // `declare` forward-declares vars with no value; it is a definition of
        // the name, which is what reference and rename passes care about.
        ClojureOperator::Def | ClojureOperator::Defonce | ClojureOperator::Declare => {
            DefinitionCategory::Variable
        }
        ClojureOperator::Defn | ClojureOperator::DefnPrivate => DefinitionCategory::Function,
        ClojureOperator::Defmacro => DefinitionCategory::Macro,
        ClojureOperator::Defmulti => DefinitionCategory::GenericFunction,
        ClojureOperator::Defmethod => DefinitionCategory::Method,
        ClojureOperator::Defprotocol | ClojureOperator::Definterface | ClojureOperator::Deftype => {
            DefinitionCategory::Class
        }
        ClojureOperator::Defrecord | ClojureOperator::Defstruct => DefinitionCategory::Struct,
        ClojureOperator::Deftest => DefinitionCategory::Test,
        _ => return None,
    })
}

pub(super) const fn binding_form(operator: ClojureOperator) -> Option<ClojureBindingForm> {
    Some(match operator {
        // Every one of these takes a `[name init ...]` vector and, unlike
        // Common Lisp `let`, evaluates the initializers in order with each
        // name visible to the initializers that follow it.
        ClojureOperator::Let
        | ClojureOperator::Loop
        | ClojureOperator::Binding
        | ClojureOperator::WithLocalVars
        | ClojureOperator::WithOpen
        | ClojureOperator::WithRedefs
        | ClojureOperator::WhenLet
        | ClojureOperator::WhenSome
        | ClojureOperator::WhenFirst
        | ClojureOperator::IfLet
        | ClojureOperator::IfSome => ClojureBindingForm::SequentialPairs,
        ClojureOperator::Letfn => ClojureBindingForm::LocalFunctions,
        ClojureOperator::Doseq | ClojureOperator::Dotimes | ClojureOperator::For => {
            ClojureBindingForm::IterationClauses
        }
        ClojureOperator::Fn => ClojureBindingForm::Parameters,
        _ => return None,
    })
}

pub(super) const fn threading_form(operator: ClojureOperator) -> Option<ClojureThreadingForm> {
    Some(match operator {
        ClojureOperator::Thread => ClojureThreadingForm::First,
        ClojureOperator::ThreadLast => ClojureThreadingForm::Last,
        ClojureOperator::AsThread => ClojureThreadingForm::Named,
        ClojureOperator::SomeThread => ClojureThreadingForm::SomeFirst,
        ClojureOperator::SomeThreadLast => ClojureThreadingForm::SomeLast,
        ClojureOperator::CondThread => ClojureThreadingForm::CondFirst,
        ClojureOperator::CondThreadLast => ClojureThreadingForm::CondLast,
        _ => return None,
    })
}
