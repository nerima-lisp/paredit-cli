use super::*;

#[test]
fn explicit_path_rejects_compiler_macrolet_shadowed_local_function_calls() {
    assert_shadowed_explicit_path!(
        "(defun render () (compiler-macrolet ((fetch-user (id) `(fetch-user ,id))) (fetch-user user)))"
    );
}

#[test]
fn explicit_path_rejects_cl_user_compiler_macrolet_shadowed_local_function_calls() {
    assert_shadowed_explicit_path!(
        "(defun render () (cl-user:compiler-macrolet ((fetch-user (id) `(fetch-user ,id))) (fetch-user user)))"
    );
}

#[test]
fn explicit_path_rejects_cl_user_macrolet_shadowed_local_function_calls() {
    assert_shadowed_explicit_path!(
        "(defun render () (cl-user:macrolet ((fetch-user (id) `(fetch-user ,id))) (fetch-user user)))"
    );
}

#[test]
fn all_calls_replace_outer_calls_inside_cl_user_macrolet_expanders_only() {
    assert_replace_calls!(
        input: "(defun render () (cl-user:macrolet ((fetch-user (id) `(fetch-user ,id))) (fetch-user user)) (fetch-user root))",
        scope: ReplaceFunctionCallsScope::AllCalls,
        calls: 2,
        rewritten_contains: [
            "(cl-user:macrolet",
            "`(load-user ,id)",
            "(fetch-user user)",
            "(load-user root)"
        ]
    );
}

#[test]
fn all_calls_replaces_calls_inside_global_macro_expander_templates() {
    assert_replace_calls!(
        input: "(defmacro build (id) `(fetch-user ,id)) (fetch-user root)",
        scope: ReplaceFunctionCallsScope::AllCalls,
        calls: 2,
        rewritten: "(defmacro build (id) `(load-user ,id)) (load-user root)"
    );
}

#[test]
fn all_calls_replaces_calls_inside_define_setf_expander_templates() {
    assert_replace_calls!(
        input: "(define-setf-expander slot (place) (values nil nil nil `(fetch-user store) `(fetch-user ,place))) (fetch-user root)",
        scope: ReplaceFunctionCallsScope::AllCalls,
        calls: 3,
        rewritten: "(define-setf-expander slot (place) (values nil nil nil `(load-user store) `(load-user ,place))) (load-user root)"
    );
}

#[test]
fn all_calls_replaces_calls_inside_long_defsetf_templates() {
    assert_replace_calls!(
        input: "(defsetf slot (place) (store) `(fetch-user ,place ,store)) (fetch-user root)",
        scope: ReplaceFunctionCallsScope::AllCalls,
        calls: 2,
        rewritten: "(defsetf slot (place) (store) `(load-user ,place ,store)) (load-user root)"
    );
}
