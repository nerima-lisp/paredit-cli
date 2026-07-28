use super::*;

#[test]
fn all_calls_skip_cl_user_macrolet_local_function_calls() {
    assert_wrap_calls!(
        input: "(defun render () (cl-user:macrolet ((fetch-user (id) `(fetch-user ,id))) (fetch-user user)) (fetch-user root))",
        scope: WrapFunctionCallsScope::AllCalls,
        calls: 2,
        rewritten_contains: [
            "(cl-user:macrolet",
            "(with-cache (fetch-user root))"
        ]
    );
}

#[test]
fn explicit_path_rejects_compiler_macrolet_shadowed_local_function_calls() {
    assert_shadowed_wrap_explicit_path!(
        "(defun render () (compiler-macrolet ((fetch-user (id) `(fetch-user ,id))) (fetch-user user)))"
    );
}

#[test]
fn explicit_path_rejects_cl_user_compiler_macrolet_shadowed_local_function_calls() {
    assert_shadowed_wrap_explicit_path!(
        "(defun render () (cl-user:compiler-macrolet ((fetch-user (id) `(fetch-user ,id))) (fetch-user user)))"
    );
}

#[test]
fn explicit_path_rejects_cl_user_macrolet_shadowed_local_function_calls() {
    assert_shadowed_wrap_explicit_path!(
        "(defun render () (cl-user:macrolet ((fetch-user (id) `(fetch-user ,id))) (fetch-user user)))"
    );
}

#[test]
fn all_calls_wraps_calls_inside_global_macro_expander_templates() {
    assert_wrap_calls!(
        input: "(defmacro build (id) `(fetch-user ,id)) (fetch-user root)",
        function: "fetch-user",
        wrapper: "with-cache",
        wrapper_template: None,
        scope: WrapFunctionCallsScope::AllCalls,
        calls: 2,
        rewritten: "(defmacro build (id) `(with-cache (fetch-user ,id))) (with-cache (fetch-user root))"
    );
}

#[test]
fn all_calls_wraps_calls_inside_define_setf_expander_templates() {
    assert_wrap_calls!(
        input: "(define-setf-expander slot (place) (values nil nil nil `(fetch-user store) `(fetch-user ,place))) (fetch-user root)",
        function: "fetch-user",
        wrapper: "with-cache",
        wrapper_template: None,
        scope: WrapFunctionCallsScope::AllCalls,
        calls: 3,
        rewritten: "(define-setf-expander slot (place) (values nil nil nil `(with-cache (fetch-user store)) `(with-cache (fetch-user ,place)))) (with-cache (fetch-user root))"
    );
}

#[test]
fn all_calls_wraps_calls_inside_long_defsetf_templates() {
    assert_wrap_calls!(
        input: "(defsetf slot (place) (store) `(fetch-user ,place ,store)) (fetch-user root)",
        function: "fetch-user",
        wrapper: "with-cache",
        wrapper_template: None,
        scope: WrapFunctionCallsScope::AllCalls,
        calls: 2,
        rewritten: "(defsetf slot (place) (store) `(with-cache (fetch-user ,place ,store))) (with-cache (fetch-user root))"
    );
}
