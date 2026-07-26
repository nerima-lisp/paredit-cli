use crate::domain::common_lisp::{CommonLispOperator, normalize_common_lisp_operator_head};
use crate::domain::dialect::Dialect;

use super::DefinitionCategory;

pub(super) fn classify_definition_head(dialect: Dialect, head: &str) -> Option<DefinitionCategory> {
    if dialect == Dialect::CommonLisp {
        return CommonLispOperator::from_head(head)
            .and_then(CommonLispOperator::definition_category)
            .or_else(|| {
                let normalized_lower =
                    normalize_common_lisp_operator_head(head).to_ascii_lowercase();
                match normalized_lower.as_str() {
                    "deftest" => Some(DefinitionCategory::Test),
                    _ if normalized_lower.starts_with("define-") => {
                        Some(DefinitionCategory::UnknownMacro)
                    }
                    _ => None,
                }
            });
    }

    if dialect != Dialect::Unknown && !dialect.is_definition_head(head) {
        return None;
    }

    let normalized = normalize_common_lisp_operator_head(head);
    let normalized_lower = normalized.to_ascii_lowercase();
    let category = match dialect {
        Dialect::EmacsLisp => match normalized_lower.as_str() {
            "defun" | "defsubst" | "cl-defun" => DefinitionCategory::Function,
            "defmacro" | "cl-defmacro" => DefinitionCategory::Macro,
            "cl-defgeneric" => DefinitionCategory::GenericFunction,
            "cl-defmethod" => DefinitionCategory::Method,
            "defvar" => DefinitionCategory::Variable,
            "defconst" => DefinitionCategory::Constant,
            "defcustom" | "defgroup" => DefinitionCategory::Customization,
            "define-minor-mode" | "define-derived-mode" => DefinitionCategory::Mode,
            "provide" | "require" => DefinitionCategory::Package,
            _ => return None,
        },
        Dialect::Lfe => match normalized_lower.as_str() {
            "defun" => DefinitionCategory::Function,
            "defmacro" => DefinitionCategory::Macro,
            "defrecord" => DefinitionCategory::Struct,
            "defmodule" => DefinitionCategory::Package,
            "defsyntax" => DefinitionCategory::Macro,
            _ => return None,
        },
        Dialect::Hy => match normalized_lower.as_str() {
            "defn" => DefinitionCategory::Function,
            "defmacro" => DefinitionCategory::Macro,
            "defclass" => DefinitionCategory::Class,
            "setv" => DefinitionCategory::Variable,
            _ => return None,
        },
        Dialect::Carp => match normalized_lower.as_str() {
            "defn" => DefinitionCategory::Function,
            "def" => DefinitionCategory::Variable,
            "deftype" => DefinitionCategory::Struct,
            "definterface" => DefinitionCategory::Class,
            "defmodule" => DefinitionCategory::Package,
            _ => return None,
        },
        Dialect::Scheme | Dialect::Racket => match normalized_lower.as_str() {
            "define" | "lambda" => DefinitionCategory::Function,
            "define-syntax" => DefinitionCategory::Macro,
            "define-library" => DefinitionCategory::Package,
            "let" | "let*" => DefinitionCategory::Other,
            _ => return None,
        },
        Dialect::Clojure => match normalized_lower.as_str() {
            "ns" => DefinitionCategory::Package,
            "def" => DefinitionCategory::Variable,
            "defn" => DefinitionCategory::Function,
            "defmacro" => DefinitionCategory::Macro,
            "defrecord" => DefinitionCategory::Struct,
            "deftype" | "defprotocol" => DefinitionCategory::Class,
            "defmulti" => DefinitionCategory::GenericFunction,
            "defmethod" => DefinitionCategory::Method,
            _ => return None,
        },
        Dialect::Janet => match normalized_lower.as_str() {
            "def" | "def-" => DefinitionCategory::Variable,
            "defn" | "defn-" => DefinitionCategory::Function,
            "defmacro" => DefinitionCategory::Macro,
            _ => return None,
        },
        Dialect::Fennel => match normalized_lower.as_str() {
            "fn" | "lambda" => DefinitionCategory::Function,
            "macro" => DefinitionCategory::Macro,
            "local" | "global" => DefinitionCategory::Variable,
            _ => return None,
        },
        Dialect::Unknown => {
            if let Some(category) = CommonLispOperator::from_head(head)
                .and_then(CommonLispOperator::definition_category)
            {
                return Some(category);
            }

            match normalized_lower.as_str() {
                "cl-defun" | "defsubst" | "definline" | "defn" | "defn-" => {
                    DefinitionCategory::Function
                }
                "cl-defmacro" => DefinitionCategory::Macro,
                "cl-defgeneric" => DefinitionCategory::GenericFunction,
                "cl-defmethod" => DefinitionCategory::Method,
                "cl-defclass" => DefinitionCategory::Class,
                "cl-defstruct" | "defrecord" => DefinitionCategory::Struct,
                "def" | "setq-default" => DefinitionCategory::Variable,
                "defconst" => DefinitionCategory::Constant,
                "defparameter" => DefinitionCategory::Parameter,
                "defcustom" => DefinitionCategory::Customization,
                "deftest" | "define-test" | "ert-deftest" | "define-ert-test" => {
                    DefinitionCategory::Test
                }
                "provide" | "require" => DefinitionCategory::Package,
                "defgroup" | "defface" => DefinitionCategory::Customization,
                "define-minor-mode" | "define-derived-mode" | "define-globalized-minor-mode" => {
                    DefinitionCategory::Mode
                }
                _ if dialect.is_definition_head(head) => DefinitionCategory::Other,
                _ if normalized_lower.starts_with("define-") => DefinitionCategory::UnknownMacro,
                _ => return None,
            }
        }
        Dialect::CommonLisp => unreachable!("Common Lisp is handled before dialect dispatch"),
    };
    Some(category)
}

pub(super) fn is_macro_expander_definition(dialect: Dialect, head: &str) -> bool {
    match dialect {
        Dialect::CommonLisp | Dialect::Unknown => matches!(
            CommonLispOperator::from_head(head),
            Some(
                CommonLispOperator::Defmacro
                    | CommonLispOperator::DefineCompilerMacro
                    | CommonLispOperator::DefineSetfExpander
                    | CommonLispOperator::Defsetf
            )
        ),
        Dialect::EmacsLisp => matches!(
            normalize_common_lisp_operator_head(head)
                .to_ascii_lowercase()
                .as_str(),
            "defmacro" | "cl-defmacro"
        ),
        Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Clojure
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Janet
        | Dialect::Fennel => false,
    }
}
