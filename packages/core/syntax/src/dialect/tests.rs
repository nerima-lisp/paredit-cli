use crate::common_lisp::{
    CommonLispLetBindingForm, CommonLispLocalCallableForm, CommonLispPackageDeclarationForm,
    CommonLispRuntimeDependencyForm, CommonLispValueScopeForm, CommonLispVariableBindingForm,
};

use super::Dialect;

#[test]
fn detects_common_lisp_extensions() {
    assert_eq!(Dialect::from_extension("lisp"), Dialect::CommonLisp);
    assert_eq!(Dialect::from_extension("asd"), Dialect::CommonLisp);
}

#[test]
fn detects_emacs_lisp_extension() {
    assert_eq!(Dialect::from_extension("el"), Dialect::EmacsLisp);
}

#[test]
fn detects_lfe_and_hy_extensions_and_capabilities() {
    assert_eq!(Dialect::from_extension("lfe"), Dialect::Lfe);
    assert_eq!(Dialect::from_extension("hy"), Dialect::Hy);
    assert_eq!(Dialect::Lfe.label(), "lfe");
    assert_eq!(Dialect::Hy.label(), "hy");
    assert_eq!("lfe".parse::<Dialect>().unwrap(), Dialect::Lfe);
    assert_eq!("hy".parse::<Dialect>().unwrap(), Dialect::Hy);

    // LFE is paren/`defun`-based; Hy is bracket/`defn`-based.
    assert!(Dialect::Lfe.is_definition_head("defun"));
    assert_eq!(Dialect::Lfe.inline_function_sequence_head(), "progn");
    assert!(Dialect::Hy.is_definition_head("defn"));
    assert!(Dialect::Hy.is_definition_head("setv"));
    assert_eq!(Dialect::Hy.inline_function_sequence_head(), "do");

    // Both support inline-let via their respective let models.
    assert!(Dialect::Lfe.supports_inline_let_refactor_head("let"));
    assert!(Dialect::Hy.supports_inline_let_refactor_head("let"));
}

#[test]
fn detects_carp_extension_and_capabilities() {
    assert_eq!(Dialect::from_extension("carp"), Dialect::Carp);
    assert_eq!(Dialect::Carp.label(), "carp");
    assert_eq!("carp".parse::<Dialect>().unwrap(), Dialect::Carp);

    // Carp is bracket/`defn`-based, like Hy and Clojure.
    assert!(Dialect::Carp.is_definition_head("defn"));
    assert!(Dialect::Carp.is_definition_head("deftype"));
    assert_eq!(Dialect::Carp.inline_function_sequence_head(), "do");
    assert!(Dialect::Carp.supports_inline_let_refactor_head("let"));
    assert!(!Dialect::Carp.supports_inline_let_refactor_head("let*"));
}

#[test]
fn detects_racket_extensions_and_labels() {
    assert_eq!(Dialect::from_extension("rkt"), Dialect::Racket);
    assert_eq!(Dialect::from_extension("rktl"), Dialect::Racket);
    assert_eq!(Dialect::from_extension("rktd"), Dialect::Racket);
    assert_eq!(Dialect::Racket.label(), "racket");
    assert_eq!("racket".parse::<Dialect>().unwrap(), Dialect::Racket);
    // Racket mirrors Scheme's structural capability profile.
    assert!(Dialect::Racket.supports_inline_let_refactor_head("let"));
    assert!(Dialect::Racket.is_definition_head("define"));
    assert_eq!(Dialect::Racket.inline_function_sequence_head(), "begin");
}

#[test]
fn detects_common_lisp_definition_heads_from_operator_semantics() {
    assert!(Dialect::CommonLisp.is_definition_head("defun"));
    assert!(Dialect::CommonLisp.is_definition_head("cl:defun"));
    assert!(Dialect::CommonLisp.is_definition_head("asdf:defsystem"));
    assert!(!Dialect::CommonLisp.is_definition_head("load"));
}

#[test]
fn detects_emacs_lisp_definition_heads_from_cl_forms() {
    assert!(Dialect::EmacsLisp.is_definition_head("cl-defun"));
    assert!(Dialect::EmacsLisp.is_definition_head("cl-defmacro"));
    assert!(Dialect::EmacsLisp.is_definition_head("cl-defgeneric"));
    assert!(Dialect::EmacsLisp.is_definition_head("cl-defmethod"));
    assert!(Dialect::Unknown.is_definition_head("cl-defgeneric"));
}

#[test]
fn exposes_function_parameter_refactor_capability_by_dialect() {
    assert!(
        Dialect::CommonLisp.supports_function_parameter_refactor_head("cl:define-setf-expander")
    );
    assert!(Dialect::CommonLisp.supports_function_parameter_refactor_head("defsetf"));
    assert!(Dialect::CommonLisp.supports_function_parameter_refactor_head("define-compiler-macro"));
    assert!(Dialect::CommonLisp.supports_function_parameter_refactor_head("define-modify-macro"));
    assert!(Dialect::CommonLisp.supports_function_parameter_refactor_head("defmethod"));
    assert!(Dialect::CommonLisp.supports_function_parameter_refactor_head("defgeneric"));
    assert!(Dialect::EmacsLisp.supports_function_parameter_refactor_head("defsubst"));
    assert!(Dialect::EmacsLisp.supports_function_parameter_refactor_head("cl-defun"));
    assert!(Dialect::EmacsLisp.supports_function_parameter_refactor_head("cl-defmacro"));
    assert!(Dialect::EmacsLisp.supports_function_parameter_refactor_head("cl-defgeneric"));
    assert!(Dialect::EmacsLisp.supports_function_parameter_refactor_head("cl-defmethod"));
    assert!(!Dialect::EmacsLisp.supports_function_parameter_refactor_head("defgeneric"));
    assert!(Dialect::Unknown.supports_function_parameter_refactor_head("cl-defgeneric"));
    assert!(Dialect::Unknown.supports_function_parameter_refactor_head("defsubst"));
    assert!(Dialect::Unknown.supports_function_parameter_refactor_head("defn"));
}

#[test]
fn exposes_inline_function_refactor_capability_by_dialect() {
    assert!(Dialect::CommonLisp.supports_inline_function_refactor_head("cl:defun"));
    assert!(Dialect::CommonLisp.supports_inline_function_refactor_head("defmacro"));
    assert!(Dialect::CommonLisp.supports_inline_function_refactor_head("define-compiler-macro"));
    assert!(!Dialect::CommonLisp.supports_inline_function_refactor_head("define-setf-expander"));
    assert!(Dialect::EmacsLisp.supports_inline_function_refactor_head("defsubst"));
    assert!(!Dialect::EmacsLisp.supports_inline_function_refactor_head("defmacro"));
    assert!(Dialect::Unknown.supports_inline_function_refactor_head("definline"));
}

#[test]
fn exposes_inline_function_sequence_head_by_dialect() {
    assert_eq!(Dialect::CommonLisp.inline_function_sequence_head(), "progn");
    assert_eq!(Dialect::EmacsLisp.inline_function_sequence_head(), "progn");
    assert_eq!(Dialect::Unknown.inline_function_sequence_head(), "progn");
    assert_eq!(Dialect::Scheme.inline_function_sequence_head(), "begin");
    assert_eq!(Dialect::Clojure.inline_function_sequence_head(), "do");
    assert_eq!(Dialect::Janet.inline_function_sequence_head(), "do");
    assert_eq!(Dialect::Fennel.inline_function_sequence_head(), "do");
}

#[test]
fn exposes_common_lisp_lambda_list_refactor_model_by_dialect() {
    assert!(Dialect::CommonLisp.supports_common_lisp_lambda_list_refactor_model());
    assert!(Dialect::EmacsLisp.supports_common_lisp_lambda_list_refactor_model());
    assert!(Dialect::Unknown.supports_common_lisp_lambda_list_refactor_model());
    assert!(!Dialect::Scheme.supports_common_lisp_lambda_list_refactor_model());
    assert!(!Dialect::Clojure.supports_common_lisp_lambda_list_refactor_model());
    assert!(!Dialect::Janet.supports_common_lisp_lambda_list_refactor_model());
    assert!(!Dialect::Fennel.supports_common_lisp_lambda_list_refactor_model());
}

#[test]
fn exposes_common_lisp_local_callable_resolution_by_dialect() {
    assert_eq!(
        Dialect::CommonLisp.common_lisp_local_callable_form_for_head("cl:flet"),
        Some(CommonLispLocalCallableForm::Flet)
    );
    assert_eq!(
        Dialect::CommonLisp.common_lisp_local_callable_form_for_head("defun"),
        None
    );
    assert_eq!(
        Dialect::Unknown.common_lisp_local_callable_form_for_head("cl:macrolet"),
        Some(CommonLispLocalCallableForm::Macrolet)
    );
    assert_eq!(
        Dialect::EmacsLisp.common_lisp_local_callable_form_for_head("cl-flet"),
        Some(CommonLispLocalCallableForm::Flet)
    );
    assert_eq!(
        Dialect::EmacsLisp.common_lisp_local_callable_form_for_head("cl-labels"),
        Some(CommonLispLocalCallableForm::Labels)
    );
}

#[test]
fn exposes_let_binding_refactor_capability_by_dialect() {
    assert_eq!(
        Dialect::CommonLisp.let_binding_form_for_head("cl:let"),
        Some(CommonLispLetBindingForm::Parallel)
    );
    assert_eq!(
        Dialect::CommonLisp.let_binding_form_for_head("let*"),
        Some(CommonLispLetBindingForm::Sequential)
    );
    assert!(Dialect::CommonLisp.supports_inline_let_refactor_head("let*"));
    assert!(Dialect::EmacsLisp.supports_inline_let_refactor_head("let"));
    assert!(Dialect::EmacsLisp.supports_inline_let_refactor_head("cl-symbol-macrolet"));
    assert!(Dialect::Scheme.supports_inline_let_refactor_head("let"));
    assert!(Dialect::Clojure.supports_inline_let_refactor_head("let"));
    assert!(Dialect::CommonLisp.supports_inline_let_refactor_head("symbol-macrolet"));
    assert!(Dialect::CommonLisp.supports_inline_let_refactor_head("cl-user:symbol-macrolet"));
    assert_eq!(
        Dialect::EmacsLisp.let_binding_form_for_head("cl-symbol-macrolet"),
        Some(CommonLispLetBindingForm::SymbolMacro)
    );
    assert_eq!(
        Dialect::CommonLisp.let_binding_form_for_head("cl-user:symbol-macrolet"),
        Some(CommonLispLetBindingForm::SymbolMacro)
    );
    assert_eq!(Dialect::Clojure.let_binding_form_for_head("let"), None);
}

#[test]
fn exposes_extract_function_value_scope_capability_by_dialect() {
    assert_eq!(
        Dialect::CommonLisp.common_lisp_value_scope_form_for_head("cl:let"),
        Some(CommonLispValueScopeForm::Let(
            CommonLispLetBindingForm::Parallel
        ))
    );
    assert_eq!(
        Dialect::Clojure.common_lisp_value_scope_form_for_head("let"),
        Some(CommonLispValueScopeForm::Let(
            CommonLispLetBindingForm::Parallel
        ))
    );
    assert_eq!(
        Dialect::Clojure.common_lisp_value_scope_form_for_head("fn"),
        Some(CommonLispValueScopeForm::FunctionLiteral)
    );
    assert_eq!(
        Dialect::Clojure.common_lisp_value_scope_form_for_head("do"),
        None
    );
    assert_eq!(
        Dialect::EmacsLisp.common_lisp_value_scope_form_for_head("let"),
        Some(CommonLispValueScopeForm::Let(
            CommonLispLetBindingForm::Parallel
        ))
    );
    assert_eq!(
        Dialect::EmacsLisp.common_lisp_value_scope_form_for_head("cl-symbol-macrolet"),
        Some(CommonLispValueScopeForm::Let(
            CommonLispLetBindingForm::SymbolMacro
        ))
    );
    assert_eq!(
        Dialect::EmacsLisp.common_lisp_value_scope_form_for_head("cl-flet"),
        Some(CommonLispValueScopeForm::LocalCallable(
            CommonLispLocalCallableForm::Flet
        ))
    );
}

#[test]
fn exposes_common_lisp_variable_binding_form_by_dialect() {
    assert_eq!(
        Dialect::CommonLisp.variable_binding_form_for_head("cl:do"),
        Some(CommonLispVariableBindingForm::Parallel)
    );
    assert_eq!(
        Dialect::CommonLisp.variable_binding_form_for_head("do*"),
        Some(CommonLispVariableBindingForm::Sequential)
    );
    assert_eq!(
        Dialect::CommonLisp.variable_binding_form_for_head("prog"),
        Some(CommonLispVariableBindingForm::Parallel)
    );
    assert_eq!(
        Dialect::Unknown.variable_binding_form_for_head("cl:prog*"),
        Some(CommonLispVariableBindingForm::Sequential)
    );
    assert_eq!(
        Dialect::EmacsLisp.variable_binding_form_for_head("do*"),
        None
    );
}

#[test]
fn exposes_common_lisp_dependency_and_package_capabilities_by_dialect() {
    assert_eq!(
        Dialect::CommonLisp.common_lisp_runtime_dependency_form_for_head("cl:require"),
        Some(CommonLispRuntimeDependencyForm::Require)
    );
    assert_eq!(
        Dialect::Unknown.common_lisp_runtime_dependency_form_for_head("load-file"),
        Some(CommonLispRuntimeDependencyForm::LoadFile)
    );
    assert_eq!(
        Dialect::EmacsLisp.common_lisp_runtime_dependency_form_for_head("require"),
        Some(CommonLispRuntimeDependencyForm::Require)
    );
    assert_eq!(
        Dialect::EmacsLisp.common_lisp_runtime_dependency_form_for_head("use-package"),
        None
    );
    assert_eq!(
        Dialect::EmacsLisp.common_lisp_runtime_dependency_form_for_head("import"),
        None
    );
    assert_eq!(
        Dialect::CommonLisp.common_lisp_package_declaration_form_for_head("in-package"),
        Some(CommonLispPackageDeclarationForm::InPackage)
    );
    assert!(Dialect::CommonLisp.is_common_lisp_asdf_system_definition_head("asdf:defsystem"));
    assert!(!Dialect::EmacsLisp.is_common_lisp_asdf_system_definition_head("defsystem"));
}
