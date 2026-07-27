use super::*;

#[test]
fn renames_quoted_setf_function_names_in_fdefinition() {
    assert_function_rename! {
        input: "(defun accessor (x) x)\n(fdefinition '(setf accessor))",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 1,
        changed: true,
        rewritten_contains: [
            "(defun slot-accessor (x) x)",
            "(fdefinition '(setf slot-accessor))"
        ]
    };
}

#[test]
fn renames_unquoted_setf_function_designators_inside_quasiquote() {
    assert_function_rename! {
        input: "(defun accessor (x) x)\n(defun caller () `(list ,#'(setf accessor) ,(function (setf accessor)) ,(fdefinition '(setf accessor))))",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(defun slot-accessor (x) x)",
            "(defun caller () `(list ,#'(setf slot-accessor) ,(function (setf slot-accessor)) ,(fdefinition '(setf slot-accessor))))"
        ]
    };
}

#[test]
fn renames_setf_function_designators_inside_reader_quoted_lambda_bodies() {
    assert_function_rename! {
        input: "(defun accessor (x) x)\n(defun caller () #'(lambda () #'(setf accessor) (function (setf accessor)) (fdefinition '(setf accessor)) (setf (accessor thing) 1)))",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 4,
        changed: true,
        rewritten_contains: [
            "(defun slot-accessor (x) x)",
            "#'(lambda () #'(setf slot-accessor) (function (setf slot-accessor)) (fdefinition '(setf slot-accessor)) (setf (slot-accessor thing) 1))"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_define_setf_expander_designators_inside_reader_quoted_lambda_bodies()
 {
    assert_function_rename! {
        input: "(cl:define-setf-expander accessor (place) (values nil nil '(store) '(writer store) '(reader place)))\n(defun caller () #'(lambda () #'accessor (function accessor) (setf (accessor thing) 1)))",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl:define-setf-expander slot-accessor (place)",
            "#'(lambda () #'slot-accessor (function slot-accessor) (setf (slot-accessor thing) 1))"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_define_setf_expander_designators_inside_reader_quoted_lambda_bodies()
 {
    assert_function_rename! {
        input: "(cl-user:define-setf-expander accessor (place) (values nil nil '(store) '(writer store) '(reader place)))\n(defun caller () #'(lambda () #'accessor (function accessor) (setf (accessor thing) 1)))",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl-user:define-setf-expander slot-accessor (place)",
            "#'(lambda () #'slot-accessor (function slot-accessor) (setf (slot-accessor thing) 1))"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_defsetf_designators_inside_reader_quoted_lambda_bodies() {
    assert_function_rename! {
        input: "(cl:defsetf accessor set-accessor)\n(defun caller () #'(lambda () #'accessor (function accessor) (setf (accessor thing) 1)))",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl:defsetf slot-accessor set-accessor)",
            "#'(lambda () #'slot-accessor (function slot-accessor) (setf (slot-accessor thing) 1))"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_defsetf_designators_inside_reader_quoted_lambda_bodies() {
    assert_function_rename! {
        input: "(cl-user:defsetf accessor set-accessor)\n(defun caller () #'(lambda () #'accessor (function accessor) (setf (accessor thing) 1)))",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl-user:defsetf slot-accessor set-accessor)",
            "#'(lambda () #'slot-accessor (function slot-accessor) (setf (slot-accessor thing) 1))"
        ]
    };
}
