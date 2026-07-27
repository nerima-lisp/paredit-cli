use super::*;

#[test]
fn skips_common_lisp_macrolet_local_macro_calls() {
    let tree = parse("(defun main () (macrolet ((helper (x) (list 'target x))) (helper 1)))");
    let symbol = SymbolName::new("helper").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert!(calls.is_empty());
}

#[test]
fn skips_common_lisp_cl_user_macrolet_local_macro_calls() {
    let tree =
        parse("(defun main () (cl-user:macrolet ((helper (x) (list 'target x))) (helper 1)))");
    let symbol = SymbolName::new("helper").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert!(calls.is_empty());
}

#[test]
fn skips_emacs_lisp_cl_macrolet_local_macro_calls() {
    let tree = parse("(defun main () (cl-macrolet ((helper (x) (list 'target x))) (helper 1)))");
    let symbol = SymbolName::new("helper").unwrap();
    let calls = build_call_report(&tree, Dialect::EmacsLisp, Some(&symbol), false).unwrap();

    assert!(calls.is_empty());
}

#[test]
fn skips_common_lisp_compiler_macrolet_local_macro_calls() {
    let tree =
        parse("(defun main () (compiler-macrolet ((helper (x) (list 'target x))) (helper 1)))");
    let symbol = SymbolName::new("helper").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert!(calls.is_empty());
}

#[test]
fn skips_common_lisp_cl_macrolet_local_macro_calls() {
    let tree = parse("(defun main () (cl:macrolet ((helper (x) (list 'target x))) (helper 1)))");
    let symbol = SymbolName::new("helper").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert!(calls.is_empty());
}

#[test]
fn skips_common_lisp_cl_user_compiler_macrolet_local_macro_calls() {
    let tree = parse(
        "(defun main () (cl-user:compiler-macrolet ((helper (x) (list 'target x))) (helper 1)))",
    );
    let symbol = SymbolName::new("helper").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert!(calls.is_empty());
}

#[test]
fn skips_common_lisp_cl_compiler_macrolet_local_macro_calls() {
    let tree =
        parse("(defun main () (cl:compiler-macrolet ((helper (x) (list 'target x))) (helper 1)))");
    let symbol = SymbolName::new("helper").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert!(calls.is_empty());
}

#[test]
fn reports_common_lisp_symbol_macrolet_body_calls_without_callable_shadowing() {
    let tree = parse("(defun main () (symbol-macrolet ((helper place)) (helper 1) (target 2)))");
    let symbol = SymbolName::new("helper").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].head, "helper");
    assert_eq!(calls[0].argument_count, 1);
    assert_eq!(calls[0].enclosing_definition.as_deref(), Some("main"));
}

#[test]
fn reports_common_lisp_cl_user_symbol_macrolet_body_calls_without_callable_shadowing() {
    let tree =
        parse("(defun main () (cl-user:symbol-macrolet ((helper place)) (helper 1) (target 2)))");
    let symbol = SymbolName::new("helper").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].head, "helper");
    assert_eq!(calls[0].argument_count, 1);
    assert_eq!(calls[0].enclosing_definition.as_deref(), Some("main"));
}

#[test]
fn reports_common_lisp_symbol_macrolet_expansion_calls() {
    let tree = parse("(defun main () (symbol-macrolet ((value (target 1))) value))");
    let symbol = SymbolName::new("target").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].head, "target");
    assert_eq!(calls[0].argument_count, 1);
    assert_eq!(calls[0].enclosing_definition.as_deref(), Some("main"));
}

#[test]
fn skips_common_lisp_defmethod_specialized_lambda_list_calls() {
    let tree = parse("(defmethod render :around ((node widget) stream) (draw node stream))");
    let calls = build_call_report(&tree, Dialect::CommonLisp, None, false).unwrap();

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].head, "draw");
    assert_eq!(calls[0].enclosing_definition.as_deref(), Some("render"));
}

#[test]
fn reports_common_lisp_setf_place_calls_for_setf_callables() {
    let tree = parse(
        "(define-setf-expander accessor (place) (values nil nil '(store) '(writer store) '(reader place)))\n(defun render (item) (setf (accessor item) 1) accessor)",
    );
    let symbol = SymbolName::new("accessor").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].head, "accessor");
    assert_eq!(calls[0].argument_count, 1);
    assert_eq!(calls[0].enclosing_definition.as_deref(), Some("render"));
}

#[test]
fn skips_common_lisp_quoted_call_heads() {
    let tree = parse("(defun main () '(target 1))");
    let symbol = SymbolName::new("target").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert!(calls.is_empty());
}

#[test]
fn reports_common_lisp_unquoted_call_heads_inside_quasiquote() {
    let tree = parse("(defun main () `(value ,(target 1)))");
    let symbol = SymbolName::new("target").unwrap();
    let calls = build_call_report(&tree, Dialect::CommonLisp, Some(&symbol), false).unwrap();

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].head, "target");
    assert_eq!(calls[0].argument_count, 1);
    assert_eq!(calls[0].enclosing_definition.as_deref(), Some("main"));
}
