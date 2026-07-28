use crate::clojure::{ClojureOperator, is_clojure_definition_head};
use crate::common_lisp::{
    CommonLispBindingRefactorForm, CommonLispLetBindingForm, CommonLispLocalCallableForm,
    CommonLispOperator, CommonLispPackageDeclarationForm, CommonLispRuntimeDependencyForm,
    CommonLispValueScopeForm, CommonLispVariableBindingForm,
};

use crate::scheme::SchemeOperator;

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
            // Mirrors `SchemeOperator`: a head earns a place here when the
            // form it names introduces something a definition report should
            // list. The previous six-entry list omitted `define-values`,
            // `define-record-type` and every `letrec` flavour, so a file using
            // them looked as though it defined nothing.
            Self::Scheme | Self::Racket => {
                SchemeOperator::from_head(head).is_some_and(|operator| {
                    operator.definition_category().is_some() || operator.is_binder()
                })
            }
            Self::Clojure => is_clojure_definition_head(head),
            Self::Hy => matches!(
                head,
                "defn" | "defn/a" | "defmacro" | "defclass" | "setv" | "setx" | "require"
            ),
            Self::Carp => matches!(
                head,
                "defn"
                    | "def"
                    | "defmacro"
                    | "deftype"
                    | "definterface"
                    | "defdynamic"
                    | "defndynamic"
                    | "defmodule"
            ),
            Self::Janet => matches!(
                head,
                "def"
                    | "def-"
                    | "defn"
                    | "defn-"
                    | "defmacro"
                    | "defmacro-"
                    | "var"
                    | "var-"
                    | "varfn"
            ),
            // `λ` is Fennel's own spelling of `lambda`, and `macros` defines a
            // table of macros rather than a single one.
            Self::Fennel => matches!(
                head,
                "fn" | "lambda" | "λ" | "macro" | "macros" | "local" | "global" | "var"
            ),
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
            Self::Lfe => matches!(head, "defun" | "defmacro"),
            Self::Scheme | Self::Racket => matches!(head, "define"),
            // Clojure's private `defn-` takes the same parameter vector as
            // `defn`, and `defmethod` carries one after its dispatch value.
            Self::Clojure => {
                clojure_operator(head).is_some_and(ClojureOperator::is_callable_definition)
            }
            Self::Hy => matches!(head, "defn" | "defn/a" | "defmacro"),
            Self::Carp => matches!(head, "defn" | "defndynamic" | "defmacro"),
            Self::Janet => matches!(head, "defn" | "defn-" | "defmacro" | "defmacro-" | "varfn"),
            Self::Fennel => matches!(head, "fn" | "lambda" | "λ" | "macro"),
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
            Self::Clojure => {
                clojure_operator(head).is_some_and(ClojureOperator::is_inline_function_definition)
            }
            Self::Janet => matches!(head, "defn" | "defn-"),
            Self::Hy => matches!(head, "defn" | "defn/a"),
            Self::Carp => matches!(head, "defn" | "defndynamic"),
            Self::Fennel => matches!(head, "fn" | "lambda" | "λ"),
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

    /// Whether `(f a . rest)` is a legal parameter list in this dialect.
    ///
    /// Deliberately broader than
    /// [`Self::supports_common_lisp_lambda_list_refactor_model`], which asks
    /// the different question of whether `&optional`/`&key`/`&rest` markers
    /// apply. A dotted tail is the *only* way to write a rest parameter in
    /// Scheme (R7RS 4.1.4), so gating it on the Common Lisp model rejected
    /// ordinary Scheme.
    #[must_use]
    pub const fn supports_dotted_rest_parameter(self) -> bool {
        matches!(
            self,
            Self::CommonLisp
                | Self::EmacsLisp
                | Self::Lfe
                | Self::Scheme
                | Self::Racket
                | Self::Unknown
        )
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
            // Clojure `let` is sequential, not parallel: in
            // `(let [x 1 y (inc x)] y)` the initializer of `y` sees `x`.
            Self::Clojure if clojure_operator(head) == Some(ClojureOperator::Let) => Some(
                CommonLispValueScopeForm::Let(CommonLispLetBindingForm::Sequential),
            ),
            Self::Clojure if clojure_operator(head) == Some(ClojureOperator::Fn) => {
                Some(CommonLispValueScopeForm::FunctionLiteral)
            }
            Self::Hy | Self::Carp if head == "let" => Some(CommonLispValueScopeForm::Let(
                CommonLispLetBindingForm::Parallel,
            )),
            Self::Hy | Self::Carp if head == "fn" => {
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
                // LFE's `flet`/`fletrec` bind `(name (args) body)` entries,
                // which is the Common Lisp `flet`/`labels` layout rather than
                // the `(name value)` pairs a let-style binding list holds.
                "flet" => Some(CommonLispBindingRefactorForm::LocalCallable(
                    CommonLispLocalCallableForm::Flet,
                )),
                "fletrec" => Some(CommonLispBindingRefactorForm::LocalCallable(
                    CommonLispLocalCallableForm::Labels,
                )),
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
            // See `common_lisp_value_scope_form_for_head`: Clojure `let` is
            // sequential, and the two layers must agree.
            Self::Clojure if clojure_operator(head) == Some(ClojureOperator::Let) => Some(
                CommonLispBindingRefactorForm::Let(CommonLispLetBindingForm::Sequential),
            ),
            // Bracketed binding forms all take the same `[name value ...]`
            // pairs, so the resource and conditional binders join `let`.
            Self::Hy | Self::Carp | Self::Janet | Self::Fennel
                if bracket_let_binding_head(self, head) =>
            {
                Some(CommonLispBindingRefactorForm::Let(
                    CommonLispLetBindingForm::Parallel,
                ))
            }
            Self::Clojure | Self::Hy | Self::Carp | Self::Janet if head == "fn" => {
                Some(CommonLispBindingRefactorForm::LambdaLike)
            }
            Self::Fennel if matches!(head, "fn" | "lambda" | "λ") => {
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

    /// Reports whether this dialect resolves the head of a call in a separate
    /// namespace from ordinary values, as Common Lisp does.
    ///
    /// In such a dialect `(f 2)` calls the function `f` no matter what a
    /// surrounding `let` bound `f` to, so renaming that lexical binding must
    /// leave the call head alone. LFE inherits the split from Erlang, where a
    /// function is identified by name and arity rather than by a value.
    ///
    /// A dialect that returns `false` is a Lisp-1: the head is an ordinary
    /// value reference and renaming the binding has to rewrite it.
    ///
    /// # Examples
    ///
    /// ```
    /// use paredit_core_syntax::dialect::Dialect;
    ///
    /// assert!(Dialect::CommonLisp.separates_function_namespace());
    /// assert!(Dialect::Lfe.separates_function_namespace());
    /// assert!(!Dialect::Scheme.separates_function_namespace());
    /// assert!(!Dialect::Fennel.separates_function_namespace());
    /// ```
    #[must_use]
    pub const fn separates_function_namespace(self) -> bool {
        matches!(self, Self::CommonLisp | Self::EmacsLisp | Self::Lfe)
    }

    /// Returns the byte length of the leading segment of `text` that names a
    /// binding, when the atom reaches into that binding rather than being it.
    ///
    /// Fennel's `handle:read` and `tbl.field` and Hy's `obj.attr` access a
    /// member of whatever value the leading segment is bound to, so renaming
    /// that binding has to rewrite the segment: rewriting the whole atom would
    /// destroy the access path, and rewriting nothing would leave the reference
    /// pointing at a name that no longer exists.
    ///
    /// Janet's `file/open` and Carp's `Array.length` look similar but are not:
    /// their separators namespace a *module*, resolved from the environment
    /// rather than from any lexical binding, so a local named `file` must leave
    /// `file/open` alone. Those dialects therefore return `None`.
    ///
    /// Also returns `None` when the whole atom is the name, when the atom
    /// merely starts with a separator (`:keyword`, Fennel's `.` accessor form),
    /// and for numeric literals such as `1.5`.
    ///
    /// # Examples
    ///
    /// ```
    /// use paredit_core_syntax::dialect::Dialect;
    ///
    /// assert_eq!(Dialect::Fennel.binding_reference_root_len("handle:read"), Some(6));
    /// assert_eq!(Dialect::Fennel.binding_reference_root_len("tbl.field"), Some(3));
    /// assert_eq!(Dialect::Hy.binding_reference_root_len("obj.attr"), Some(3));
    /// assert_eq!(Dialect::Fennel.binding_reference_root_len("handle"), None);
    /// assert_eq!(Dialect::Fennel.binding_reference_root_len(":keyword"), None);
    /// assert_eq!(Dialect::Fennel.binding_reference_root_len("1.5"), None);
    /// // Module namespacing, not member access on a binding.
    /// assert_eq!(Dialect::Janet.binding_reference_root_len("file/open"), None);
    /// assert_eq!(Dialect::Carp.binding_reference_root_len("Array.length"), None);
    /// assert_eq!(Dialect::CommonLisp.binding_reference_root_len("a:b"), None);
    /// ```
    #[must_use]
    pub fn binding_reference_root_len(self, text: &str) -> Option<usize> {
        let separators: &[char] = match self {
            Self::Fennel => &['.', ':'],
            Self::Hy => &['.'],
            Self::CommonLisp
            | Self::EmacsLisp
            | Self::Lfe
            | Self::Scheme
            | Self::Racket
            | Self::Clojure
            | Self::Carp
            | Self::Janet
            | Self::Unknown => return None,
        };

        let root_len = text.find(separators)?;
        let root = text.get(..root_len)?;
        (!root.is_empty() && !root.bytes().all(|byte| byte.is_ascii_digit())).then_some(root_len)
    }

    pub fn supports_inline_let_refactor_head(self, head: &str) -> bool {
        match self {
            Self::Clojure => clojure_operator(head) == Some(ClojureOperator::Let),
            Self::Hy | Self::Carp | Self::Janet | Self::Fennel => {
                bracket_let_binding_head(self, head)
            }
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

/// Reports whether `head` opens a bracketed `[name value ...]` binding list.
///
/// Beyond `let`, each of these dialects spells resource acquisition and
/// conditional binding with the same pair list, so a binding refactor that
/// understands one understands all of them. Janet's `if-let` and `if-with` are
/// excluded: their else branch is outside the binding's scope.
fn bracket_let_binding_head(dialect: Dialect, head: &str) -> bool {
    match dialect {
        Dialect::Clojure => head == "let",
        Dialect::Hy => matches!(head, "let" | "with" | "for"),
        Dialect::Carp => matches!(head, "let" | "let-do"),
        Dialect::Janet => matches!(
            head,
            "let" | "with" | "with-vars" | "when-let" | "when-with"
        ),
        Dialect::Fennel => matches!(head, "let" | "with-open"),
        Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Unknown => false,
    }
}

fn clojure_operator(head: &str) -> Option<ClojureOperator> {
    ClojureOperator::from_head(head)
}
