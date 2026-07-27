use super::*;

#[test]
fn plans_emacs_lisp_cl_defmethod_optional_parameter_rename_without_touching_default_form() {
    assert_binding_rename! {
        input: "(cl-defmethod render ((node widget) &optional (stream (default-stream node) stream-p)) (list node stream stream-p))",
        dialect: Dialect::EmacsLisp,
        from: "stream",
        to: "out",
        form: "cl-defmethod",
        references: 1,
        rewritten: "(cl-defmethod render ((node widget) &optional (out (default-stream node) stream-p)) (list node out stream-p))",
    }
}

#[test]
fn plans_defmacro_optional_parameter_rename_without_touching_default_form() {
    assert_binding_rename! {
        input: "(defmacro wrap (&optional (value (default value) supplied)) (list value supplied))",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "form",
        form: "defmacro",
        references: 1,
        rewritten: "(defmacro wrap (&optional (form (default value) supplied)) (list form supplied))",
    }
}

#[test]
fn plans_defmacro_body_parameter_rename_without_touching_call_site_forms() {
    assert_binding_rename! {
        input: "(defmacro collect-forms (&body body) (list body (length body)))",
        dialect: Dialect::CommonLisp,
        from: "body",
        to: "forms",
        form: "defmacro",
        references: 2,
        rewritten: "(defmacro collect-forms (&body forms) (list forms (length forms)))",
    }
}

#[test]
fn plans_defmacro_environment_parameter_rename_without_touching_body() {
    assert_binding_rename! {
        input: "(defmacro inspect (&environment env value) (list env value))",
        dialect: Dialect::CommonLisp,
        from: "env",
        to: "macro-env",
        form: "defmacro",
        references: 1,
        rewritten: "(defmacro inspect (&environment macro-env value) (list macro-env value))",
    }
}

#[test]
fn plans_defmacro_whole_and_environment_parameter_rename_without_touching_body() {
    assert_binding_rename! {
        input: "(defmacro inspect (&whole whole &environment env value) (list whole env value))",
        dialect: Dialect::CommonLisp,
        from: "env",
        to: "macro-env",
        form: "defmacro",
        references: 1,
        rewritten: "(defmacro inspect (&whole whole &environment macro-env value) (list whole macro-env value))",
    }
}

#[test]
fn plans_defmacro_aux_parameter_rename_without_touching_aux_initializer() {
    assert_binding_rename! {
        input: "(defmacro inspect (&whole form value &aux (tag form)) (list form value tag))",
        dialect: Dialect::CommonLisp,
        from: "form",
        to: "macro-form",
        form: "defmacro",
        references: 2,
        rewritten: "(defmacro inspect (&whole macro-form value &aux (tag macro-form)) (list macro-form value tag))",
    }
}

#[test]
fn plans_macrolet_aux_parameter_rename_without_touching_aux_initializer() {
    assert_binding_rename! {
        input: "(macrolet ((inspect (&whole form value &aux (tag form)) (list form value tag))) (inspect value) form)",
        dialect: Dialect::CommonLisp,
        from: "form",
        to: "macro-form",
        form: "macrolet",
        references: 2,
        rewritten: "(macrolet ((inspect (&whole macro-form value &aux (tag macro-form)) (list macro-form value tag))) (inspect value) form)",
    }
}

#[test]
fn plans_define_setf_expander_environment_parameter_rename() {
    assert_binding_rename! {
        input: "(define-setf-expander slot (&whole whole &environment env target) (list whole env target))",
        dialect: Dialect::CommonLisp,
        from: "env",
        to: "macro-env",
        form: "define-setf-expander",
        references: 1,
        rewritten: "(define-setf-expander slot (&whole whole &environment macro-env target) (list whole macro-env target))",
    }
}

#[test]
fn plans_define_compiler_macro_environment_parameter_rename() {
    assert_binding_rename! {
        input: "(define-compiler-macro render (&whole whole &environment env target) (list whole env target))",
        dialect: Dialect::CommonLisp,
        from: "env",
        to: "macro-env",
        form: "define-compiler-macro",
        references: 1,
        rewritten: "(define-compiler-macro render (&whole whole &environment macro-env target) (list whole macro-env target))",
    }
}

#[test]
fn plans_macrolet_whole_parameter_rename_without_touching_call_site_form() {
    assert_binding_rename! {
        input: "(macrolet ((wrap (&whole whole value) (list whole value))) (wrap value))",
        dialect: Dialect::CommonLisp,
        from: "whole",
        to: "form",
        form: "macrolet",
        references: 1,
        rewritten: "(macrolet ((wrap (&whole form value) (list form value))) (wrap value))",
    }
}

#[test]
fn plans_compiler_macrolet_environment_parameter_rename_without_touching_body() {
    assert_binding_rename! {
        input: "(compiler-macrolet ((expand (&whole whole &environment env value) (list whole env value))) (expand value) env)",
        dialect: Dialect::CommonLisp,
        from: "env",
        to: "macro-env",
        form: "compiler-macrolet",
        references: 1,
        rewritten: "(compiler-macrolet ((expand (&whole whole &environment macro-env value) (list whole macro-env value))) (expand value) env)",
    }
}

#[test]
fn plans_compiler_macrolet_aux_parameter_rename_without_touching_aux_initializer() {
    assert_binding_rename! {
        input: "(compiler-macrolet ((inspect (&whole form value &aux (tag form)) (list form value tag))) (inspect value) form)",
        dialect: Dialect::CommonLisp,
        from: "form",
        to: "macro-form",
        form: "compiler-macrolet",
        references: 2,
        rewritten: "(compiler-macrolet ((inspect (&whole macro-form value &aux (tag macro-form)) (list macro-form value tag))) (inspect value) form)",
    }
}

#[test]
fn plans_macrolet_key_parameter_rename_without_touching_key_designator_or_call_site() {
    assert_binding_rename! {
        input: "(macrolet ((inspect (&key ((:value value) (default value) value-supplied)) (list value value-supplied))) (inspect :value value) value)",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "form",
        form: "macrolet",
        references: 1,
        rewritten: "(macrolet ((inspect (&key ((:value form) (default value) value-supplied)) (list form value-supplied))) (inspect :value value) value)",
    }
}

#[test]
fn plans_compiler_macrolet_key_parameter_rename_without_touching_key_designator_or_call_site() {
    assert_binding_rename! {
        input: "(compiler-macrolet ((inspect (&key ((:value value) (default value) value-supplied)) (list value value-supplied))) (inspect :value value) value)",
        dialect: Dialect::CommonLisp,
        from: "value",
        to: "form",
        form: "compiler-macrolet",
        references: 1,
        rewritten: "(compiler-macrolet ((inspect (&key ((:value form) (default value) value-supplied)) (list form value-supplied))) (inspect :value value) value)",
    }
}
