use super::*;

#[test]
fn classifies_common_lisp_setf_place_calls_against_setf_expander_signature() {
    let reports = build_signature_reports(
        vec![source(
            "(define-setf-expander accessor (place) (values nil nil '(store) '(writer store) '(reader place)))\n(defun render (item) (setf (accessor item) 1) accessor)",
        )],
        Some(&crate::domain::sexpr::SymbolName::new("accessor").unwrap()),
    )
    .unwrap();

    assert_eq!(reports[0].definitions.len(), 1);
    assert_eq!(reports[0].definitions[0].parameter_count, Some(1));
    assert_eq!(reports[0].calls.len(), 1);
    assert_eq!(reports[0].calls[0].call.head, "accessor");
    assert_eq!(
        reports[0].calls[0].expected_parameter_arity,
        Some((1, Some(1)))
    );
    assert_eq!(reports[0].calls[0].status, SignatureCallStatus::Exact);
}

#[test]
fn classifies_common_lisp_setf_place_calls_against_defsetf_long_form_signature() {
    let reports = build_signature_reports(
        vec![source(
            "(defsetf accessor (item) (value) `(writer ,item ,value))\n(defun render (item) (setf (accessor item) 1) accessor)",
        )],
        Some(&crate::domain::sexpr::SymbolName::new("accessor").unwrap()),
    )
    .unwrap();

    assert_eq!(reports[0].definitions.len(), 1);
    assert_eq!(reports[0].definitions[0].parameter_count, Some(1));
    assert_eq!(reports[0].calls.len(), 1);
    assert_eq!(reports[0].calls[0].call.head, "accessor");
    assert_eq!(
        reports[0].calls[0].expected_parameter_arity,
        Some((1, Some(1)))
    );
    assert_eq!(reports[0].calls[0].status, SignatureCallStatus::Exact);
}

#[test]
fn classifies_common_lisp_macro_definitions_with_signature_arity() {
    let reports = build_signature_reports(
        vec![source(
            "(defmacro with-pane (pane theme) `(render ,pane ,theme))\n(defun use () (with-pane pane theme))",
        )],
        Some(&crate::domain::sexpr::SymbolName::new("with-pane").unwrap()),
    )
    .unwrap();

    assert_eq!(reports[0].definitions.len(), 1);
    assert_eq!(
        reports[0].definitions[0].category,
        crate::domain::definition::DefinitionCategory::Macro
    );
    assert_eq!(reports[0].definitions[0].parameter_count, Some(2));
    assert_eq!(reports[0].calls.len(), 1);
    assert_eq!(reports[0].calls[0].call.head, "with-pane");
    assert_eq!(
        reports[0].calls[0].expected_parameter_arity,
        Some((2, Some(2)))
    );
    assert_eq!(reports[0].calls[0].status, SignatureCallStatus::Exact);
}

#[test]
fn classifies_common_lisp_define_method_combination_with_signature_arity() {
    let reports = build_signature_reports(
        vec![source(
            "(define-method-combination render-combination (pane theme) ((primary *)) (list pane theme primary))",
        )],
        Some(&crate::domain::sexpr::SymbolName::new("render-combination").unwrap()),
    )
    .unwrap();

    assert_eq!(reports[0].definitions.len(), 1);
    assert_eq!(
        reports[0].definitions[0].category,
        crate::domain::definition::DefinitionCategory::Macro
    );
    assert_eq!(reports[0].definitions[0].parameter_count, Some(2));
    assert!(reports[0].calls.is_empty());
}

#[test]
fn classifies_common_lisp_compiler_macro_definitions_with_signature_arity() {
    let reports = build_signature_reports(
        vec![source(
            "(define-compiler-macro optimize-render (pane theme) `(render ,pane ,theme))\n(defun use () (optimize-render pane theme))",
        )],
        Some(&crate::domain::sexpr::SymbolName::new("optimize-render").unwrap()),
    )
    .unwrap();

    assert_eq!(reports[0].definitions.len(), 1);
    assert_eq!(
        reports[0].definitions[0].category,
        crate::domain::definition::DefinitionCategory::Macro
    );
    assert_eq!(reports[0].definitions[0].parameter_count, Some(2));
    assert_eq!(reports[0].calls.len(), 1);
    assert_eq!(reports[0].calls[0].call.head, "optimize-render");
    assert_eq!(
        reports[0].calls[0].expected_parameter_arity,
        Some((2, Some(2)))
    );
    assert_eq!(reports[0].calls[0].status, SignatureCallStatus::Exact);
}

#[test]
fn classifies_common_lisp_modify_macro_definitions_with_signature_arity() {
    let reports = build_signature_reports(
        vec![source("(define-modify-macro updatef (place) incf)")],
        Some(&crate::domain::sexpr::SymbolName::new("updatef").unwrap()),
    )
    .unwrap();

    assert_eq!(reports[0].definitions.len(), 1);
    assert_eq!(
        reports[0].definitions[0].category,
        crate::domain::definition::DefinitionCategory::Macro
    );
    assert_eq!(reports[0].definitions[0].parameter_count, Some(1));
    assert!(reports[0].calls.is_empty());
}

#[test]
fn leaves_common_lisp_short_defsetf_without_signature_definition() {
    let reports = build_signature_reports(
        vec![source(
            "(defsetf accessor set-accessor)\n(defun render (item) (setf (accessor item) 1) accessor)",
        )],
        Some(&crate::domain::sexpr::SymbolName::new("accessor").unwrap()),
    )
    .unwrap();

    assert!(reports[0].definitions.is_empty());
    assert_eq!(reports[0].calls.len(), 1);
    assert_eq!(reports[0].calls[0].call.head, "accessor");
    assert_eq!(reports[0].calls[0].expected_parameter_arity, None);
    assert_eq!(
        reports[0].calls[0].status,
        SignatureCallStatus::UnknownDefinition
    );
}

#[test]
fn includes_common_lisp_symbol_macro_without_arity_signature() {
    let reports = build_signature_reports(
        vec![source(
            "(define-symbol-macro current-user (slot-value *session* 'user))\n(list current-user)",
        )],
        Some(&crate::domain::sexpr::SymbolName::new("current-user").unwrap()),
    )
    .unwrap();

    assert_eq!(reports[0].definitions.len(), 1);
    assert_eq!(
        reports[0].definitions[0].category,
        crate::domain::definition::DefinitionCategory::Variable
    );
    assert_eq!(reports[0].definitions[0].parameter_count, None);
    assert!(reports[0].calls.is_empty());
}
