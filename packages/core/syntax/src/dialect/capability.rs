use crate::common_lisp::{
    CommonLispBindingRefactorForm, CommonLispLetBindingForm, CommonLispLocalCallableForm,
    CommonLispOperator, CommonLispPackageDeclarationForm, CommonLispRuntimeDependencyForm,
    CommonLispValueScopeForm, CommonLispVariableBindingForm,
};

use super::Dialect;

impl Dialect {
    #[must_use]
    pub fn is_definition_head(self, head: &str) -> bool {
        match self {
            Self::CommonLisp => common_lisp_operator(head)
                .is_some_and(|operator| operator.definition_category().is_some()),
            Self::EmacsLisp => matches!(
                head,
                "defun"
                    | "defmacro"
                    | "defsubst"
                    | "cl-defun"
                    | "cl-defmacro"
                    | "cl-defgeneric"
                    | "cl-defmethod"
                    | "defvar"
                    | "defconst"
                    | "defcustom"
                    | "defgroup"
                    | "define-minor-mode"
                    | "define-derived-mode"
                    | "provide"
                    | "require"
            ),
            Self::Lfe => matches!(
                head,
                "defun" | "defmacro" | "defrecord" | "defmodule" | "defsyntax"
            ),
            Self::Scheme | Self::Racket => matches!(
                head,
                "define" | "define-syntax" | "define-library" | "lambda" | "let" | "let*"
            ),
            Self::Clojure => matches!(
                head,
                "ns" | "def"
                    | "defn"
                    | "defmacro"
                    | "defrecord"
                    | "deftype"
                    | "defprotocol"
                    | "defmulti"
                    | "defmethod"
            ),
            Self::Hy => matches!(head, "defn" | "defmacro" | "defclass" | "setv" | "require"),
            Self::Carp => matches!(
                head,
                "defn" | "def" | "deftype" | "definterface" | "defdynamic" | "defmodule"
            ),
            Self::Janet => matches!(head, "def" | "defn" | "defmacro" | "def-" | "defn-"),
            Self::Fennel => matches!(head, "fn" | "lambda" | "macro" | "local" | "global"),
            Self::Unknown => {
                head.starts_with("def")
                    || head.starts_with("cl-def")
                    || matches!(head, "define" | "ns")
            }
        }
    }

    pub fn supports_function_parameter_refactor_head(self, head: &str) -> bool {
        match self {
            Self::CommonLisp => common_lisp_operator(head)
                .is_some_and(CommonLispOperator::supports_function_parameter_refactor),
            Self::EmacsLisp => matches!(
                head,
                "defun"
                    | "defmacro"
                    | "defsubst"
                    | "cl-defun"
                    | "cl-defmacro"
                    | "cl-defgeneric"
                    | "cl-defmethod"
            ),
            Self::Lfe => matches!(head, "defun"),
            Self::Scheme | Self::Racket => matches!(head, "define"),
            Self::Clojure | Self::Hy | Self::Carp => matches!(head, "defn" | "defmacro"),
            Self::Janet => matches!(head, "defn" | "defmacro"),
            Self::Fennel => matches!(head, "fn" | "lambda"),
            Self::Unknown => {
                Self::CommonLisp.supports_function_parameter_refactor_head(head)
                    || matches!(
                        head,
                        "defsubst"
                            | "cl-defun"
                            | "cl-defmacro"
                            | "cl-defgeneric"
                            | "cl-defmethod"
                            | "define"
                            | "defn"
                            | "fn"
                            | "lambda"
                    )
            }
        }
    }

    pub fn supports_inline_function_refactor_head(self, head: &str) -> bool {
        match self {
            Self::CommonLisp => common_lisp_operator(head)
                .is_some_and(CommonLispOperator::is_inline_function_definition),
            Self::EmacsLisp => matches!(head, "defun" | "cl-defun" | "defsubst"),
            Self::Lfe => head == "defun",
            Self::Scheme | Self::Racket => head == "define",
            Self::Clojure | Self::Hy | Self::Carp | Self::Janet => matches!(head, "defn" | "defn-"),
            Self::Fennel => head == "fn",
            Self::Unknown => {
                Self::CommonLisp.supports_inline_function_refactor_head(head)
                    || matches!(
                        head,
                        "defun"
                            | "cl-defun"
                            | "defsubst"
                            | "definline"
                            | "defn"
                            | "defn-"
                            | "define"
                            | "fn"
                    )
            }
        }
    }

    #[must_use]
    pub const fn inline_function_sequence_head(self) -> &'static str {
        match self {
            Self::CommonLisp | Self::EmacsLisp | Self::Lfe | Self::Unknown => "progn",
            Self::Scheme | Self::Racket => "begin",
            Self::Clojure | Self::Hy | Self::Carp | Self::Janet | Self::Fennel => "do",
        }
    }

    #[must_use]
    pub const fn supports_common_lisp_lambda_list_refactor_model(self) -> bool {
        matches!(self, Self::CommonLisp | Self::EmacsLisp | Self::Unknown)
    }

    #[must_use]
    pub fn common_lisp_local_callable_form_for_head(
        self,
        head: &str,
    ) -> Option<CommonLispLocalCallableForm> {
        if !matches!(self, Self::CommonLisp | Self::EmacsLisp | Self::Unknown) {
            return None;
        }
        common_lisp_operator(head)?.local_callable_form()
    }

    #[must_use]
    pub fn let_binding_form_for_head(self, head: &str) -> Option<CommonLispLetBindingForm> {
        if !matches!(
            self,
            Self::CommonLisp
                | Self::EmacsLisp
                | Self::Lfe
                | Self::Scheme
                | Self::Racket
                | Self::Unknown
        ) {
            return None;
        }
        common_lisp_operator(head)?.let_binding_form()
    }

    #[must_use]
    pub fn variable_binding_form_for_head(
        self,
        head: &str,
    ) -> Option<CommonLispVariableBindingForm> {
        if !matches!(self, Self::CommonLisp | Self::Unknown) {
            return None;
        }
        common_lisp_operator(head)?.variable_binding_form()
    }

    #[must_use]
    pub fn common_lisp_value_scope_form_for_head(
        self,
        head: &str,
    ) -> Option<CommonLispValueScopeForm> {
        if matches!(self, Self::CommonLisp | Self::EmacsLisp | Self::Unknown) {
            return common_lisp_operator(head)?.value_scope_form();
        }

        match self {
            Self::Clojure | Self::Hy | Self::Carp if head == "let" => Some(
                CommonLispValueScopeForm::Let(CommonLispLetBindingForm::Parallel),
            ),
            Self::Clojure | Self::Hy | Self::Carp if head == "fn" => {
                Some(CommonLispValueScopeForm::FunctionLiteral)
            }
            Self::Lfe if head == "let" => Some(CommonLispValueScopeForm::Let(
                CommonLispLetBindingForm::Parallel,
            )),
            _ => None,
        }
    }

    #[must_use]
    pub fn common_lisp_binding_refactor_form_for_head(
        self,
        head: &str,
    ) -> Option<CommonLispBindingRefactorForm> {
        if matches!(self, Self::CommonLisp | Self::EmacsLisp | Self::Unknown) {
            return common_lisp_operator(head)?.binding_refactor_form();
        }

        match self {
            Self::Lfe => match head {
                "let" => Some(CommonLispBindingRefactorForm::Let(
                    CommonLispLetBindingForm::Parallel,
                )),
                "let*" => Some(CommonLispBindingRefactorForm::Let(
                    CommonLispLetBindingForm::Sequential,
                )),
                "lambda" | "match-lambda" => Some(CommonLispBindingRefactorForm::LambdaLike),
                _ => None,
            },
            Self::Scheme | Self::Racket => match head {
                "let" => Some(CommonLispBindingRefactorForm::Let(
                    CommonLispLetBindingForm::Parallel,
                )),
                "let*" => Some(CommonLispBindingRefactorForm::Let(
                    CommonLispLetBindingForm::Sequential,
                )),
                "lambda" => Some(CommonLispBindingRefactorForm::LambdaLike),
                _ => None,
            },
            Self::Clojure | Self::Hy | Self::Carp | Self::Janet | Self::Fennel if head == "let" => {
                Some(CommonLispBindingRefactorForm::Let(
                    CommonLispLetBindingForm::Parallel,
                ))
            }
            Self::Clojure | Self::Hy | Self::Carp | Self::Fennel if head == "fn" => {
                Some(CommonLispBindingRefactorForm::LambdaLike)
            }
            _ => None,
        }
    }

    pub fn common_lisp_variable_binding_has_step_forms_for_head(self, head: &str) -> bool {
        matches!(self, Self::CommonLisp | Self::Unknown)
            && common_lisp_operator(head).is_some_and(CommonLispOperator::has_variable_step_forms)
    }

    #[must_use]
    pub fn common_lisp_runtime_dependency_form_for_head(
        self,
        head: &str,
    ) -> Option<CommonLispRuntimeDependencyForm> {
        let form = if matches!(self, Self::CommonLisp | Self::Unknown) {
            common_lisp_operator(head)?.runtime_dependency_form()?
        } else if self == Self::EmacsLisp {
            // `require`/`provide`/`load`/`load-file`/`load-library` are the
            // same functions with the same load-order semantics in Emacs
            // Lisp, so `dependency-report` should see them there too.
            // `use-package`/`import` are excluded: Emacs Lisp's `use-package`
            // macro (declarative package *configuration*, not the Common
            // Lisp package-system form of the same name) and `import` (not a
            // standard Emacs Lisp form at all) would misclassify an
            // unrelated construct as a dependency if allowed through here.
            match common_lisp_operator(head)?.runtime_dependency_form()? {
                form @ (CommonLispRuntimeDependencyForm::Require
                | CommonLispRuntimeDependencyForm::Provide
                | CommonLispRuntimeDependencyForm::Load
                | CommonLispRuntimeDependencyForm::LoadFile
                | CommonLispRuntimeDependencyForm::LoadLibrary) => form,
                CommonLispRuntimeDependencyForm::UsePackage
                | CommonLispRuntimeDependencyForm::Import => return None,
            }
        } else {
            return None;
        };
        Some(form)
    }

    #[must_use]
    pub fn common_lisp_package_declaration_form_for_head(
        self,
        head: &str,
    ) -> Option<CommonLispPackageDeclarationForm> {
        if !matches!(self, Self::CommonLisp | Self::Unknown) {
            return None;
        }
        common_lisp_operator(head)?.package_declaration_form()
    }

    pub fn is_common_lisp_asdf_system_definition_head(self, head: &str) -> bool {
        matches!(self, Self::CommonLisp | Self::Unknown)
            && common_lisp_operator(head).is_some_and(CommonLispOperator::is_asdf_system_definition)
    }

    pub fn supports_inline_let_refactor_head(self, head: &str) -> bool {
        match self {
            Self::Clojure | Self::Hy | Self::Carp | Self::Janet | Self::Fennel => head == "let",
            Self::CommonLisp
            | Self::EmacsLisp
            | Self::Lfe
            | Self::Scheme
            | Self::Racket
            | Self::Unknown => self
                .let_binding_form_for_head(head)
                .is_some_and(CommonLispLetBindingForm::supports_inline_refactor),
        }
    }
}

fn common_lisp_operator(head: &str) -> Option<CommonLispOperator> {
    CommonLispOperator::from_head(head)
}
