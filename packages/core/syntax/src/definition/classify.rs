use crate::clojure::ClojureOperator;
use crate::common_lisp::{
    CommonLispOperator, common_lisp_operator_head_eq, normalize_common_lisp_operator_head,
};
use crate::dialect::Dialect;
use crate::emacs_lisp::EmacsLispOperator;
use crate::scheme::SchemeOperator;

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

    if dialect == Dialect::EmacsLisp {
        // Ahead of the shared path, and not through it: everything below
        // runs `head` through the Common Lisp normalizer, which folds case
        // and strips a `cl:` prefix. Emacs Lisp does neither.
        return EmacsLispOperator::from_head(head).and_then(EmacsLispOperator::definition_category);
    }

    // Clojure is case-sensitive and qualifies core forms with `clojure.core/`
    // rather than a Common Lisp package marker, so it must not go through the
    // shared lowercase/`package:symbol` normalization below.
    if dialect == Dialect::Clojure {
        return ClojureOperator::from_head(head).and_then(ClojureOperator::definition_category);
    }

    if dialect != Dialect::Unknown && !dialect.is_definition_head(head) {
        return None;
    }

    let normalized = normalize_common_lisp_operator_head(head);
    let normalized_lower = normalized.to_ascii_lowercase();
    let category = match dialect {
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
        // Scheme is case-sensitive (R7RS 2.1), so the case-folded `normalized_
        // lower` used by the other arms would accept `DEFINE`. The raw head is
        // what the operator table matches on.
        Dialect::Scheme | Dialect::Racket => match SchemeOperator::from_head(head) {
            // `(define x 1)` and `(define (f) 1)` share a head. The category
            // depends on the shape of child 1, which `definition_shape` cannot
            // see from here, so `Function` stays the reported answer for
            // continuity; `dialect::semantic::definition_shape` distinguishes
            // them where a caller needs the difference.
            Some(SchemeOperator::Define | SchemeOperator::Lambda) => DefinitionCategory::Function,
            Some(operator) => match operator.definition_category() {
                Some(category) => category,
                // A binding form is not a definition, but the existing
                // contract reports `let`/`let*` as `Other`, and the same is
                // true of every other scope-opening head.
                None if operator.is_binder() => DefinitionCategory::Other,
                None => return None,
            },
            None => return None,
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
        Dialect::CommonLisp | Dialect::EmacsLisp | Dialect::Clojure => {
            unreachable!("Common Lisp, Emacs Lisp and Clojure are handled before dialect dispatch")
        }
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
            EmacsLispOperator::from_head(head),
            Some(
                EmacsLispOperator::Defmacro
                    | EmacsLispOperator::ClDefmacro
                    | EmacsLispOperator::ClDefineCompilerMacro
                    | EmacsLispOperator::DefineInline
            )
        ),
        Dialect::Clojure => ClojureOperator::from_head(head)
            .is_some_and(ClojureOperator::is_macro_expander_definition),
        // Every one of these spells its macro-expander form `defmacro`
        // (Janet, Hy, Carp) or has both a Lisp-2-style `defmacro` and its own
        // `defsyntax` (LFE). None of them has an operator table of its own in
        // this crate, so the head is compared directly rather than routed
        // through one.
        Dialect::Janet | Dialect::Hy | Dialect::Carp => {
            common_lisp_operator_head_eq(head, "defmacro")
        }
        Dialect::Lfe => {
            common_lisp_operator_head_eq(head, "defmacro")
                || common_lisp_operator_head_eq(head, "defsyntax")
        }
        // Fennel's macro-expander form is `macro`, not `defmacro`.
        Dialect::Fennel => common_lisp_operator_head_eq(head, "macro"),
        // Scheme and Racket stay out deliberately. A `defmacro` body is
        // *evaluated* to produce code, which is what this predicate marks; a
        // `syntax-rules` template is substituted, never run, and the
        // reference query skips `define-syntax` bodies wholesale in
        // `lexical_scope::traversal::binding_forms::scheme` instead.
        Dialect::Scheme | Dialect::Racket => false,
    }
}
