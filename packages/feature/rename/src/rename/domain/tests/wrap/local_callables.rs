use super::*;

#[test]
fn all_calls_skip_labels_local_function_calls() {
    assert_wrap_calls!(
        input: "(defun render () (labels ((fetch-user (id) (fetch-user id))) (fetch-user user)) (fetch-user root))",
        scope: WrapFunctionCallsScope::AllCalls,
        calls: 1,
        rewritten_contains: [
            "(labels ((fetch-user (id) (fetch-user id))) (fetch-user user))",
            "(with-cache (fetch-user root))"
        ]
    );
}

#[test]
fn all_calls_skip_cl_user_labels_local_function_calls() {
    assert_wrap_calls!(
        input: "(defun render () (cl-user:labels ((fetch-user (id) (fetch-user id))) (fetch-user user)) (fetch-user root))",
        scope: WrapFunctionCallsScope::AllCalls,
        calls: 1,
        rewritten_contains: [
            "(cl-user:labels ((fetch-user (id) (fetch-user id))) (fetch-user user))",
            "(with-cache (fetch-user root))"
        ]
    );
}

#[test]
fn all_calls_skip_cl_user_flet_local_function_calls() {
    assert_wrap_calls!(
        input: "(defun render () (cl-user:flet ((fetch-user (id) (fetch-user id))) (fetch-user user)) (fetch-user root))",
        scope: WrapFunctionCallsScope::AllCalls,
        calls: 2,
        rewritten_contains: [
            "(cl-user:flet ((fetch-user (id) (with-cache (fetch-user id)))) (fetch-user user))",
            "(with-cache (fetch-user root))"
        ]
    );
}

#[test]
fn explicit_path_rejects_shadowed_local_function_calls() {
    assert_shadowed_wrap_explicit_path!(
        "(defun render () (labels ((fetch-user (id) (fetch-user id))) (fetch-user user)))"
    );
}

#[test]
fn explicit_path_rejects_cl_user_labels_shadowed_local_function_calls() {
    assert_shadowed_wrap_explicit_path!(
        "(defun render () (cl-user:labels ((fetch-user (id) (fetch-user id))) (fetch-user user)))"
    );
}

#[test]
fn explicit_path_rejects_cl_user_flet_shadowed_local_function_calls() {
    assert_shadowed_wrap_explicit_path!(
        "(defun render () (cl-user:flet ((fetch-user (id) (fetch-user id))) (fetch-user user)))"
    );
}
