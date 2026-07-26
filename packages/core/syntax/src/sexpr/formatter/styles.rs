use super::Formatter;
use crate::domain::common_lisp::{CommonLispOperator, normalize_common_lisp_operator_head};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ListStyle {
    Definition,
    SystemDefinition,
    Defmethod,
    DefinitionNameBody,
    Lambda,
    NamedLambda,
    Binding,
    LocalFunctions,
    OneArgumentBody,
    TwoArgumentBody,
    ClauseForm,
    CondClauses,
    CaseClauses,
    Do,
    Prog,
    Declaration,
    PairAssignment,
    Loop,
    HeadBody,
    If,
    General,
}

impl Formatter {
    pub(super) fn style_for_head(&self, head: &str) -> ListStyle {
        if let Some(operator) = CommonLispOperator::from_head(head) {
            match operator {
                operator if operator.is_method_definition() => {
                    return ListStyle::Defmethod;
                }
                CommonLispOperator::Lambda => return ListStyle::Lambda,
                CommonLispOperator::DefineSymbolMacro => return ListStyle::DefinitionNameBody,
                operator if operator.is_asdf_system_definition() => {
                    return ListStyle::SystemDefinition;
                }
                operator if operator.definition_category().is_some() => {
                    return ListStyle::Definition;
                }
                operator if operator.is_let_binding() || operator.is_handler_bind_binding() => {
                    return ListStyle::Binding;
                }
                operator if operator.is_local_callable_binding() => {
                    return ListStyle::LocalFunctions;
                }
                operator if operator.is_iteration_binding() || operator.is_slot_binding() => {
                    return ListStyle::OneArgumentBody;
                }
                operator if operator.is_value_binding() => return ListStyle::TwoArgumentBody,
                operator if operator.is_clause_binding() => return ListStyle::ClauseForm,
                operator if operator.is_do_binding() => return ListStyle::Do,
                operator if operator.is_prog_binding() => return ListStyle::Prog,
                CommonLispOperator::Loop => return ListStyle::Loop,
                _ => {}
            }
        }

        let normalized_head = normalize_common_lisp_operator_head(head);
        match normalized_head.to_ascii_lowercase().as_str() {
            "named-lambda" => ListStyle::NamedLambda,
            "if" => ListStyle::If,
            "when"
            | "unless"
            | "with-open-file"
            | "with-open-stream"
            | "with-input-from-string"
            | "with-output-to-string"
            | "with-hash-table-iterator"
            | "with-package-iterator"
            | "block"
            | "catch"
            | "unwind-protect"
            | "eval-when" => ListStyle::OneArgumentBody,
            "cond" => ListStyle::CondClauses,
            "case" | "ccase" | "ecase" | "typecase" | "ctypecase" | "etypecase" => {
                ListStyle::CaseClauses
            }
            "progn" | "prog1" | "prog2" | "tagbody" | "defpackage" | "locally" => {
                ListStyle::HeadBody
            }
            "declare" | "declaim" | "proclaim" => ListStyle::Declaration,
            "setq" | "psetq" | "setf" | "psetf" => ListStyle::PairAssignment,
            _ => ListStyle::General,
        }
    }
}
