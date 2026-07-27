use super::super::*;

#[test]
fn renames_define_modify_macro_definition_and_macro_calls() {
    assert_function_rename! {
        input: "(define-modify-macro bumpf (delta) +)\n(bumpf place 1)\n(list #'bumpf bumpf)",
        dialect: Dialect::CommonLisp,
        from: "bumpf",
        to: "stepf",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: ["(define-modify-macro stepf (delta) +)", "(stepf place 1)", "#'stepf bumpf"]
    };
}

#[test]
fn renames_common_lisp_qualified_define_modify_macro_definition_and_macro_calls() {
    assert_function_rename! {
        input: "(cl:define-modify-macro bumpf (delta) +)\n(bumpf place 1)\n(list #'bumpf bumpf)",
        dialect: Dialect::CommonLisp,
        from: "bumpf",
        to: "stepf",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: ["(cl:define-modify-macro stepf (delta) +)", "(stepf place 1)", "#'stepf bumpf"]
    };
}

#[test]
fn renames_common_lisp_user_qualified_define_modify_macro_definition_and_macro_calls() {
    assert_function_rename! {
        input: "(cl-user:define-modify-macro bumpf (delta) +)\n(bumpf place 1)\n(list #'bumpf bumpf)",
        dialect: Dialect::CommonLisp,
        from: "bumpf",
        to: "stepf",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: ["(cl-user:define-modify-macro stepf (delta) +)", "(stepf place 1)", "#'stepf bumpf"]
    };
}

#[test]
fn renames_define_setf_expander_definition_place_uses_and_designators() {
    assert_function_rename! {
        input: "(define-setf-expander accessor (place) (values nil nil '(store) '(writer store) '(reader place)))\n(setf (accessor item) 1)\n(list #'accessor (function accessor) accessor)",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(define-setf-expander slot-accessor (place)",
            "(setf (slot-accessor item) 1)",
            "#'slot-accessor",
            "(function slot-accessor)",
            " accessor)"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_define_setf_expander_definition_place_uses_and_designators() {
    assert_function_rename! {
        input: "(cl-user:define-setf-expander accessor (place) (values nil nil '(store) '(writer store) '(reader place)))\n(setf (accessor item) 1)\n(list #'accessor (function accessor) accessor)",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl-user:define-setf-expander slot-accessor (place)",
            "(setf (slot-accessor item) 1)",
            "#'slot-accessor",
            "(function slot-accessor)",
            " accessor)"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_define_setf_expander_definition_place_uses_and_designators() {
    assert_function_rename! {
        input: "(cl:define-setf-expander accessor (place) (values nil nil '(store) '(writer store) '(reader place)))\n(setf (accessor item) 1)\n(list #'accessor (function accessor) accessor)",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl:define-setf-expander slot-accessor (place)",
            "(setf (slot-accessor item) 1)",
            "#'slot-accessor",
            "(function slot-accessor)",
            " accessor)"
        ]
    };
}

#[test]
fn renames_defsetf_definition_place_uses_and_designators() {
    assert_function_rename! {
        input: "(defsetf accessor set-accessor)\n(setf (accessor item) 1)\n(list #'accessor (function accessor) accessor)",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(defsetf slot-accessor set-accessor)",
            "(setf (slot-accessor item) 1)",
            "#'slot-accessor",
            "(function slot-accessor)",
            " accessor)"
        ]
    };
}

#[test]
fn renames_function_calls_generated_by_long_defsetf_quasiquote_expanders() {
    assert_function_rename! {
        input: "(defun update-slot (place value) (list place value))\n(defsetf slot (place) (value) `(update-slot ,place ,value))\n(update-slot root value)",
        dialect: Dialect::CommonLisp,
        from: "update-slot",
        to: "store-slot",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(defun store-slot (place value) (list place value))",
            "(defsetf slot (place) (value) `(store-slot ,place ,value))",
            "(store-slot root value)"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_defsetf_definition_place_uses_and_designators() {
    assert_function_rename! {
        input: "(cl-user:defsetf accessor set-accessor)\n(setf (accessor item) 1)\n(list #'accessor (function accessor) accessor)",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl-user:defsetf slot-accessor set-accessor)",
            "(setf (slot-accessor item) 1)",
            "#'slot-accessor",
            "(function slot-accessor)",
            " accessor)"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_defsetf_definition_place_uses_and_designators() {
    assert_function_rename! {
        input: "(cl:defsetf accessor set-accessor)\n(setf (accessor item) 1)\n(list #'accessor (function accessor) accessor)",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 1,
        calls: 3,
        changed: true,
        rewritten_contains: [
            "(cl:defsetf slot-accessor set-accessor)",
            "(setf (slot-accessor item) 1)",
            "#'slot-accessor",
            "(function slot-accessor)",
            " accessor)"
        ]
    };
}
