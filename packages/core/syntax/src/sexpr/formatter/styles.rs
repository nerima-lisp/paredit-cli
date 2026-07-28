use super::Formatter;
use crate::clojure::{ClojureIndentStyle, clojure_indent_style_for_head};
use crate::common_lisp::{CommonLispOperator, normalize_common_lisp_operator_head};
use crate::dialect::Dialect;

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
    /// `(defn name [params]` with the body indented below, falling back to only
    /// the name on the head line when a docstring, attribute map, or multi-arity
    /// clause list follows it.
    ClojureDefinition,
    /// `(fn [params]` or `(fn name [params]` with the body indented below.
    ClojureFunction,
    /// The head plus this many children share the head line and every remaining
    /// child is indented one per line.
    ///
    /// `ClojurePrefixBody(0)` is the head-alone layout, so Clojure does not need
    /// a separate [`ListStyle::HeadBody`] mapping.
    ClojurePrefixBody(usize),
    /// `(-> value` with one threading step per line, aligned under the value.
    /// The payload is how many children share the head line.
    ClojureThreading(usize),
    /// One test/result pair per line. The payload is how many children precede
    /// the clauses and stay on the head line.
    ClojurePairClauses(usize),
    General,
}

impl Formatter {
    pub(super) fn style_for_head(&self, head: &str) -> ListStyle {
        match self.dialect {
            Dialect::Clojure => Self::clojure_style_for_head(head),
            _ => Self::common_lisp_style_for_head(head),
        }
    }

    /// Maps [`ClojureIndentStyle`] onto the formatter's rendering styles.
    ///
    /// `Call` must map to [`ListStyle::General`], because [`Formatter::inline_list`]
    /// and [`Formatter::compact_node`] refuse to compact any list whose head
    /// carries a style. Giving ordinary calls a style of their own would stop
    /// every Clojure form from ever fitting on one line.
    fn clojure_style_for_head(head: &str) -> ListStyle {
        match clojure_indent_style_for_head(head) {
            ClojureIndentStyle::Definition => ListStyle::ClojureDefinition,
            ClojureIndentStyle::Function => ListStyle::ClojureFunction,
            // A Clojure binding vector lays out exactly like a Common Lisp
            // binding list: the vector stays on the head line with one
            // name/value pair per line, and the body is indented below it.
            ClojureIndentStyle::BindingVector => ListStyle::Binding,
            ClojureIndentStyle::Threading(prefix) => ListStyle::ClojureThreading(prefix),
            ClojureIndentStyle::PairClauses(prefix) => ListStyle::ClojurePairClauses(prefix),
            ClojureIndentStyle::Body(prefix) => ListStyle::ClojurePrefixBody(prefix),
            ClojureIndentStyle::HeadBody => ListStyle::ClojurePrefixBody(0),
            ClojureIndentStyle::Call => ListStyle::General,
        }
    }

    fn common_lisp_style_for_head(head: &str) -> ListStyle {
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
