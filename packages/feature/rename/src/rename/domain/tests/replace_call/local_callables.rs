use super::*;

#[test]
fn explicit_path_rejects_labels_local_function_calls() {
    assert_shadowed_explicit_path!(
        "(defun render () (labels ((fetch-user (id) (fetch-user id))) (fetch-user user)))"
    );
}

#[test]
fn explicit_path_rejects_cl_user_labels_local_function_calls() {
    assert_shadowed_explicit_path!(
        "(defun render () (cl-user:labels ((fetch-user (id) (fetch-user id))) (fetch-user user)))"
    );
}

#[test]
fn all_calls_skip_labels_local_function_calls() {
    assert_replace_calls!(
        input: "(defun main () (labels ((fetch-user (id) (fetch-user id))) (fetch-user user)))\n(fetch-user root)",
        scope: ReplaceFunctionCallsScope::AllCalls,
        calls: 1,
        rewritten_contains: [
            "(labels ((fetch-user (id) (fetch-user id))) (fetch-user user))",
            "(load-user root)"
        ]
    );
}

#[test]
fn all_calls_skip_cl_user_labels_local_function_calls() {
    assert_replace_calls!(
        input: "(defun main () (cl-user:labels ((fetch-user (id) (fetch-user id))) (fetch-user user)))\n(fetch-user root)",
        scope: ReplaceFunctionCallsScope::AllCalls,
        calls: 1,
        rewritten_contains: [
            "(cl-user:labels ((fetch-user (id) (fetch-user id))) (fetch-user user))",
            "(load-user root)"
        ]
    );
}

#[test]
fn explicit_path_rejects_cl_user_flet_local_function_calls() {
    assert_shadowed_explicit_path!(
        "(defun render () (cl-user:flet ((fetch-user (id) (fetch-user id))) (fetch-user user)))"
    );
}

#[test]
fn all_calls_replaces_outer_calls_inside_flet_binding_bodies_only() {
    assert_replace_calls!(
        input: "(defun main () (flet ((fetch-user (id) (fetch-user id))) (fetch-user user)))\n(fetch-user root)",
        scope: ReplaceFunctionCallsScope::AllCalls,
        calls: 2,
        rewritten_contains: [
            "(flet ((fetch-user (id) (load-user id))) (fetch-user user))",
            "(load-user root)"
        ]
    );
}

#[test]
fn all_calls_replaces_outer_calls_inside_cl_user_flet_binding_bodies_only() {
    assert_replace_calls!(
        input: "(defun main () (cl-user:flet ((fetch-user (id) (fetch-user id))) (fetch-user user)))\n(fetch-user root)",
        scope: ReplaceFunctionCallsScope::AllCalls,
        calls: 2,
        rewritten_contains: [
            "(cl-user:flet ((fetch-user (id) (load-user id))) (fetch-user user))",
            "(load-user root)"
        ]
    );
}
