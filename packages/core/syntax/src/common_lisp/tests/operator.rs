use super::*;

#[test]
fn classifies_package_qualified_common_lisp_heads() {
    assert_eq!(
        CommonLispOperator::from_head("cl:let*"),
        Some(CommonLispOperator::LetStar)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl:load-library"),
        Some(CommonLispOperator::LoadLibrary)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-user:restart-bind"),
        Some(CommonLispOperator::RestartBind)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-user:handler-case"),
        Some(CommonLispOperator::HandlerCase)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-user:restart-case"),
        Some(CommonLispOperator::RestartCase)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-user:handler-bind"),
        Some(CommonLispOperator::HandlerBind)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-user:macrolet"),
        Some(CommonLispOperator::Macrolet)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-user:compiler-macrolet"),
        Some(CommonLispOperator::CompilerMacrolet)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-user:symbol-macrolet"),
        Some(CommonLispOperator::SymbolMacrolet)
    );
    assert_eq!(
        CommonLispOperator::from_head("common-lisp:let*"),
        Some(CommonLispOperator::LetStar)
    );
    assert_eq!(
        CommonLispOperator::from_head("common-lisp-user:restart-bind"),
        Some(CommonLispOperator::RestartBind)
    );
    assert_eq!(
        CommonLispOperator::from_head("CL:LET*"),
        Some(CommonLispOperator::LetStar)
    );
    assert_eq!(
        CommonLispOperator::from_head("COMMON-LISP:LOAD-LIBRARY"),
        Some(CommonLispOperator::LoadLibrary)
    );
    assert_eq!(
        CommonLispOperator::from_head("CL-USER:RESTART-BIND"),
        Some(CommonLispOperator::RestartBind)
    );
    assert_eq!(
        CommonLispOperator::from_head("define-method-combination"),
        Some(CommonLispOperator::DefineMethodCombination)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl:define-method-combination"),
        Some(CommonLispOperator::DefineMethodCombination)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-user:locally"),
        Some(CommonLispOperator::Locally)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-flet"),
        Some(CommonLispOperator::Flet)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-labels"),
        Some(CommonLispOperator::Labels)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-macrolet"),
        Some(CommonLispOperator::Macrolet)
    );
    assert_eq!(
        CommonLispOperator::from_head("cl-symbol-macrolet"),
        Some(CommonLispOperator::SymbolMacrolet)
    );
    assert_eq!(CommonLispOperator::from_head("elisp:let"), None);
}

#[test]
fn normalizes_common_lisp_package_qualified_heads() {
    assert_eq!(
        normalize_common_lisp_operator_head("common-lisp:let"),
        "let"
    );
    assert_eq!(
        normalize_common_lisp_operator_head("common-lisp-user:macrolet"),
        "macrolet"
    );
    assert_eq!(
        normalize_common_lisp_operator_head("common-lisp-user:symbol-macrolet"),
        "symbol-macrolet"
    );
    assert_eq!(normalize_common_lisp_operator_head("CL:SETF"), "SETF");
    assert_eq!(
        normalize_common_lisp_operator_head("COMMON-LISP-USER:DECLARE"),
        "DECLARE"
    );
}

#[test]
fn compares_common_lisp_heads_case_insensitively_after_prefix_normalization() {
    assert!(common_lisp_operator_head_eq("CL:SETF", "setf"));
    assert!(common_lisp_operator_head_eq(
        "COMMON-LISP:DECLARE",
        "declare"
    ));
    assert!(common_lisp_operator_head_eq(
        "common-lisp-user:PROCLAIM",
        "proclaim"
    ));
    assert!(common_lisp_symbol_name_eq("cl:SETF", "common-lisp:setf"));
    assert!(common_lisp_symbol_name_eq(
        "COMMON-LISP-USER:DECLARE",
        "cl-user:declare"
    ));
    assert!(is_common_lisp_declaration_form("COMMON-LISP:DECLARE"));
    assert!(is_common_lisp_declaration_form("cl-user:DECLAIM"));
    assert!(is_common_lisp_declaration_form("common-lisp-user:PROCLAIM"));
    assert!(common_lisp_operator_head_eq("|QUOTE|", "quote"));
    assert!(!common_lisp_operator_head_eq("|quote|", "quote"));
}

#[test]
fn matches_symbol_references_across_arbitrary_package_qualifiers() {
    // Unlike `common_lisp_symbol_name_eq`, which only recognizes the four
    // standard CL home-package aliases (see
    // `compares_common_lisp_heads_case_insensitively_after_prefix_normalization`
    // and the `elisp:let` guard in `classifies_package_qualified_common_lisp_heads`),
    // occurrence matching must tolerate a reference qualified by *any*
    // user-defined package, since `nshell.application:execute-command-line`
    // and bare `execute-command-line` name the same symbol.
    assert!(common_lisp_symbol_reference_eq(
        "nshell.application:execute-command-line",
        "execute-command-line"
    ));
    assert!(common_lisp_symbol_reference_eq(
        "execute-command-line",
        "nshell.application:execute-command-line"
    ));
    assert!(common_lisp_symbol_reference_eq(
        "nshell.domain.parsing::%internal-helper",
        "%internal-helper"
    ));
    assert!(common_lisp_symbol_reference_eq(
        "NSHELL.APPLICATION:FOO",
        "foo"
    ));
    // `#:` uninterned-symbol syntax, as seen in `defpackage` `:export` lists.
    assert!(common_lisp_symbol_reference_eq(
        "#:execute-command-line",
        "execute-command-line"
    ));
    // A leading colon with nothing before it is a keyword, not a qualifier,
    // and must not be conflated with the same-named plain symbol.
    assert!(!common_lisp_symbol_reference_eq(":foo", "foo"));
    assert!(common_lisp_symbol_reference_eq(":foo", ":foo"));
    // Unrelated symbols remain unrelated even once qualifiers are stripped.
    assert!(!common_lisp_symbol_reference_eq(
        "nshell.application:foo",
        "bar"
    ));
    assert!(!common_lisp_symbol_reference_eq("foo\\:bar", "bar"));
    assert!(common_lisp_symbol_reference_eq("foo\\:bar", "|FOO:BAR|"));
    assert!(!common_lisp_symbol_reference_eq("|foo|", "foo"));
    assert_eq!(common_lisp_symbol_reference_needle("foo\\:bar"), "FOO:BAR");
    assert_eq!(
        common_lisp_symbol_reference_needle("pkg:|FOO:BAR|"),
        "FOO:BAR"
    );
    assert_ne!(
        common_lisp_symbol_reference_needle("foo"),
        common_lisp_symbol_reference_needle("|foo|")
    );
    assert!(has_common_lisp_package_qualifier("pkg:foo"));
    assert!(has_common_lisp_package_qualifier("pkg::foo"));
    assert!(!has_common_lisp_package_qualifier("foo\\:bar"));
    assert!(!has_common_lisp_package_qualifier("|foo:bar|"));
}

#[test]
fn exposes_semantic_binding_groups() {
    assert!(CommonLispOperator::Let.is_parallel_let_binding());
    assert!(CommonLispOperator::LetStar.is_sequential_let_binding());
    assert_eq!(
        CommonLispOperator::Let.let_binding_form(),
        Some(CommonLispLetBindingForm::Parallel)
    );
    assert_eq!(
        CommonLispOperator::LetStar.let_binding_form(),
        Some(CommonLispLetBindingForm::Sequential)
    );
    assert_eq!(
        CommonLispOperator::SymbolMacrolet.let_binding_form(),
        Some(CommonLispLetBindingForm::SymbolMacro)
    );
    assert!(CommonLispLetBindingForm::Sequential.is_sequential());
    assert!(!CommonLispLetBindingForm::Parallel.is_sequential());
    assert!(CommonLispLetBindingForm::SymbolMacro.supports_inline_refactor());
    assert!(CommonLispOperator::RestartBind.includes_restart_bind_options());
    assert_eq!(
        CommonLispOperator::HandlerBind.value_scope_form(),
        Some(CommonLispValueScopeForm::Handler(
            CommonLispHandlerBindingForm::Handler
        ))
    );
    assert_eq!(
        CommonLispOperator::RestartBind.value_scope_form(),
        Some(CommonLispValueScopeForm::Handler(
            CommonLispHandlerBindingForm::Restart
        ))
    );
    assert!(!CommonLispHandlerBindingForm::Handler.includes_restart_options());
    assert!(CommonLispHandlerBindingForm::Restart.includes_restart_options());
    assert!(CommonLispOperator::DoStar.is_sequential_variable_binding());
    assert!(CommonLispOperator::ProgStar.is_sequential_variable_binding());
    assert_eq!(
        CommonLispOperator::Do.variable_binding_form(),
        Some(CommonLispVariableBindingForm::Parallel)
    );
    assert_eq!(
        CommonLispOperator::ProgStar.variable_binding_form(),
        Some(CommonLispVariableBindingForm::Sequential)
    );
    assert!(CommonLispVariableBindingForm::Sequential.is_sequential());
    assert!(!CommonLispVariableBindingForm::Parallel.is_sequential());
    assert!(CommonLispOperator::Do.has_variable_step_forms());
    assert!(!CommonLispOperator::Prog.has_variable_step_forms());
    assert_eq!(
        CommonLispOperator::Let.value_scope_form(),
        Some(CommonLispValueScopeForm::Let(
            CommonLispLetBindingForm::Parallel
        ))
    );
    assert_eq!(
        CommonLispOperator::DoStar.value_scope_form(),
        Some(CommonLispValueScopeForm::Variable(
            CommonLispVariableBindingForm::Sequential
        ))
    );
    assert_eq!(
        CommonLispOperator::from_head("cl:with-slots")
            .and_then(CommonLispOperator::value_scope_form),
        Some(CommonLispValueScopeForm::Slot)
    );
    assert_eq!(
        CommonLispOperator::WithSlots.binding_refactor_form(),
        Some(CommonLispBindingRefactorForm::Slot(
            CommonLispSlotBindingForm::WithSlots
        ))
    );
    assert!(
        CommonLispBindingRefactorForm::Do(CommonLispVariableBindingForm::Parallel)
            .supports_remove_unused_binding()
    );
    assert!(
        CommonLispBindingRefactorForm::Do(CommonLispVariableBindingForm::Parallel)
            .preserves_binding_form_when_empty()
    );
    assert_eq!(
        CommonLispBindingRefactorForm::Let(CommonLispLetBindingForm::Parallel).binding_list_shape(),
        Some(CommonLispBindingListShape::NameValuePairs)
    );
    assert_eq!(
        CommonLispBindingRefactorForm::LocalCallable(CommonLispLocalCallableForm::Macrolet)
            .binding_list_shape(),
        Some(CommonLispBindingListShape::LocalCallableDefinitions(
            CommonLispLocalCallableForm::Macrolet
        ))
    );
    assert_eq!(
        CommonLispBindingRefactorForm::Do(CommonLispVariableBindingForm::Parallel)
            .binding_list_shape(),
        Some(CommonLispBindingListShape::VariableSpecs(
            CommonLispVariableSpecForm::Do
        ))
    );
    assert_eq!(
        CommonLispBindingRefactorForm::Slot(CommonLispSlotBindingForm::WithAccessors)
            .binding_list_shape(),
        Some(CommonLispBindingListShape::SlotBindings(
            CommonLispSlotBindingForm::WithAccessors
        ))
    );
    assert_eq!(
        CommonLispBindingRefactorForm::FunctionDefinition.binding_list_shape(),
        None
    );
    assert_eq!(
        CommonLispBindingRefactorForm::Let(CommonLispLetBindingForm::Sequential).reference_scope(),
        Some(CommonLispBindingReferenceScope::NameValuePairs(
            CommonLispLetBindingForm::Sequential
        ))
    );
    assert_eq!(
        CommonLispBindingRefactorForm::LocalCallable(CommonLispLocalCallableForm::Labels)
            .reference_scope(),
        Some(CommonLispBindingReferenceScope::LocalCallableDefinitions(
            CommonLispLocalCallableForm::Labels
        ))
    );
    assert_eq!(
        CommonLispBindingRefactorForm::Do(CommonLispVariableBindingForm::Sequential)
            .reference_scope(),
        Some(CommonLispBindingReferenceScope::VariableSpecs(
            CommonLispVariableSpecForm::Do,
            CommonLispVariableBindingForm::Sequential
        ))
    );
    assert_eq!(
        CommonLispBindingRefactorForm::Slot(CommonLispSlotBindingForm::WithSlots).reference_scope(),
        Some(CommonLispBindingReferenceScope::BodyOnly)
    );
    assert_eq!(CommonLispVariableSpecForm::Do.max_children(), 3);
    assert_eq!(CommonLispVariableSpecForm::Prog.max_children(), 2);
    assert!(CommonLispVariableSpecForm::Do.has_step_forms());
    assert!(!CommonLispVariableSpecForm::Prog.has_step_forms());
    assert_eq!(CommonLispVariableSpecForm::Do.end_clause_index(), Some(2));
    assert_eq!(CommonLispVariableSpecForm::Prog.end_clause_index(), None);
    assert_eq!(CommonLispVariableSpecForm::Do.body_start_index(), 3);
    assert_eq!(CommonLispVariableSpecForm::Prog.body_start_index(), 2);
    assert_eq!(
        common_lisp_binding_refactor_form_for_head("cl:restart-bind"),
        Some(CommonLispBindingRefactorForm::Handler(
            CommonLispHandlerBindingForm::Restart
        ))
    );
    assert_eq!(
        common_lisp_binding_refactor_form_for_head("cl-user:handler-case"),
        Some(CommonLispBindingRefactorForm::Clause)
    );
    assert_eq!(
        common_lisp_binding_refactor_form_for_head("cl-user:restart-case"),
        Some(CommonLispBindingRefactorForm::Clause)
    );
}

#[test]
fn maps_local_callable_operators_to_domain_form_types() {
    assert_eq!(
        CommonLispOperator::Flet.local_callable_form(),
        Some(CommonLispLocalCallableForm::Flet)
    );
    assert_eq!(
        CommonLispOperator::Labels.local_callable_form(),
        Some(CommonLispLocalCallableForm::Labels)
    );
    assert!(CommonLispLocalCallableForm::Macrolet.is_macro());
    assert!(CommonLispLocalCallableForm::CompilerMacrolet.is_macro());
    assert!(!CommonLispLocalCallableForm::Flet.is_macro());
    assert_eq!(CommonLispLocalCallableForm::Flet.operator_name(), "flet");
    assert_eq!(
        CommonLispLocalCallableForm::CompilerMacrolet.operator_name(),
        "compiler-macrolet"
    );
    assert_eq!(CommonLispOperator::Let.local_callable_form(), None);
}

#[test]
fn classifies_inline_function_definitions() {
    assert!(CommonLispOperator::Defun.is_inline_function_definition());
    assert!(CommonLispOperator::Defmacro.is_inline_function_definition());
    assert!(CommonLispOperator::DefineCompilerMacro.is_inline_function_definition());
    assert!(!CommonLispOperator::DefineSetfExpander.is_inline_function_definition());
}

#[test]
fn classifies_refactor_capabilities() {
    assert!(CommonLispOperator::Defun.supports_function_parameter_refactor());
    assert!(CommonLispOperator::DefineCompilerMacro.supports_function_parameter_refactor());
    assert!(CommonLispOperator::Defmethod.supports_function_parameter_refactor());
    assert!(CommonLispOperator::ClDefmethod.supports_function_parameter_refactor());
    assert!(CommonLispOperator::Defgeneric.supports_function_parameter_refactor());
    assert!(CommonLispOperator::DefineModifyMacro.supports_function_parameter_refactor());
    assert!(
        CommonLispOperator::Let
            .let_binding_form()
            .is_some_and(CommonLispLetBindingForm::supports_inline_refactor)
    );
    assert!(
        CommonLispOperator::LetStar
            .let_binding_form()
            .is_some_and(CommonLispLetBindingForm::supports_inline_refactor)
    );
    assert!(
        CommonLispOperator::SymbolMacrolet
            .let_binding_form()
            .is_some_and(CommonLispLetBindingForm::supports_inline_refactor)
    );
    assert_eq!(
        CommonLispOperator::Flet.binding_refactor_form(),
        Some(CommonLispBindingRefactorForm::LocalCallable(
            CommonLispLocalCallableForm::Flet
        ))
    );
    assert_eq!(
        CommonLispOperator::Labels.binding_refactor_form(),
        Some(CommonLispBindingRefactorForm::LocalCallable(
            CommonLispLocalCallableForm::Labels
        ))
    );
    assert!(
        CommonLispOperator::Macrolet
            .binding_refactor_form()
            .is_some_and(|form| matches!(
                form,
                CommonLispBindingRefactorForm::LocalCallable(local_form)
                    if local_form.is_macro()
            ))
    );
}
