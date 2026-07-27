use super::*;

#[test]
fn skips_quoted_data_and_preserves_unquote_symbol_macro_references() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) '(foo) #'foo `(list ,foo foo)\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(define-symbol-macro bar current-user)", "'(foo)", "#'foo", "`(list ,bar foo)"]
    };
}

#[test]
fn preserves_unquote_prefixes_when_renaming_symbol_macro_references_inside_quasiquote() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) `(list ,foo ,@foo foo)\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: ["(define-symbol-macro bar current-user)", "`(list ,bar ,@bar foo)"]
    };
}

#[test]
fn skips_lexically_shadowed_bindings_and_parameters() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (let ((foo 1)) foo) (lambda (foo) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(let ((foo 1)) foo)", "(lambda (foo) foo)", "bar\n"]
    };
}

#[test]
fn skips_shadowing_symbol_macrolet_bindings() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (symbol-macrolet ((foo other-user)) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(symbol-macrolet ((foo other-user)) foo)", "bar\n"]
    };
}

#[test]
fn renames_symbol_macrolet_binding_rhs_in_outer_scope_while_leaving_body_shadowed() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (symbol-macrolet ((foo foo)) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: ["(symbol-macrolet ((foo bar)) foo)", "bar\n"]
    };
}

#[test]
fn skips_nested_symbol_macrolet_shadowing_inside_body() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (symbol-macrolet ((foo other-user)) (symbol-macrolet ((foo inner-user)) foo) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(symbol-macrolet ((foo inner-user)) foo)", "foo) bar\n"]
    };
}

#[test]
fn skips_shadowing_cl_user_symbol_macrolet_bindings() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (cl-user:symbol-macrolet ((foo other-user)) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(cl-user:symbol-macrolet ((foo other-user)) foo)", "bar\n"]
    };
}

#[test]
fn renames_outer_symbol_macro_references_inside_reader_quoted_lambda_bodies_only() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) #'(lambda () (symbol-macrolet ((foo other-user)) foo) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: [
            "(define-symbol-macro bar current-user)",
            "#'(lambda () (symbol-macrolet ((foo other-user)) foo) bar) bar"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_symbol_macro_references_inside_reader_quoted_lambda_bodies_only() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) #'(lambda () (cl:symbol-macrolet ((foo other-user)) foo) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: [
            "(define-symbol-macro bar current-user)",
            "#'(lambda () (cl:symbol-macrolet ((foo other-user)) foo) bar) bar"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_symbol_macro_references_inside_reader_quoted_lambda_bodies_only()
 {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) #'(lambda () (cl-user:symbol-macrolet ((foo other-user)) foo) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: [
            "(define-symbol-macro bar current-user)",
            "#'(lambda () (cl-user:symbol-macrolet ((foo other-user)) foo) bar) bar"
        ]
    };
}

#[test]
fn skips_shadowing_package_qualified_symbol_macrolet_binding_names() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (symbol-macrolet ((cl-user:foo other-user)) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(symbol-macrolet ((cl-user:foo other-user)) foo)", "bar\n"]
    };
}

#[test]
fn skips_shadowing_cl_symbol_macrolet_bindings() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (cl:symbol-macrolet ((foo other-user)) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(cl:symbol-macrolet ((foo other-user)) foo)", "bar\n"]
    };
}

#[test]
fn skips_common_lisp_shadowing_binding_forms() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (with-slots (foo) object foo) (with-accessors ((foo get-foo)) object foo) (do ((foo seed (1+ foo))) ((done) foo) foo) (prog ((foo seed)) (return foo)) (loop for foo in values collect foo finally (return foo)) (handler-bind ((error (lambda (foo) foo))) foo) (restart-bind ((retry (lambda () foo) :report (lambda (foo) foo))) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 4,
        changed: true,
        rewritten_contains: [
            "(with-slots (foo) object foo)",
            "(with-accessors ((foo get-foo)) object foo)",
            "(handler-bind ((error (lambda (foo) foo))) bar)",
            "(restart-bind ((retry (lambda () bar) :report (lambda (foo) foo))) bar) bar"
        ]
    };
}

#[test]
fn skips_shadowing_cl_user_common_lisp_binding_forms() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (cl-user:with-slots (foo) object foo) (cl-user:with-accessors ((foo get-foo)) object foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(cl-user:with-slots (foo) object foo)", "(cl-user:with-accessors ((foo get-foo)) object foo)", "bar\n"]
    };
}

#[test]
fn skips_shadowing_cl_user_variable_binding_forms() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (cl-user:do ((foo seed (1+ foo))) ((done) foo) foo) (cl-user:do* ((foo seed (1+ foo))) ((done) foo) foo) (cl-user:prog ((foo seed)) (return foo)) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(cl-user:do ((foo seed (1+ foo))) ((done) foo) foo)", "(cl-user:prog ((foo seed)) (return foo))", "bar\n"]
    };
}

#[test]
fn renames_symbol_macro_references_in_locally_bodies_without_counting_declarations() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (locally (declare (special foo)) foo) (locally (declaim (special foo)) (proclaim (special foo)) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 3,
        changed: true,
        rewritten_contains: [
            "(define-symbol-macro bar current-user)",
            "(locally (declare (special foo)) bar)",
            "(locally (declaim (special foo)) (proclaim (special foo)) bar) bar"
        ]
    };
}

#[test]
fn renames_outer_symbol_macro_references_inside_defun_bodies() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (defun caller () foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: [
            "(define-symbol-macro bar current-user)",
            "(defun caller () bar) bar"
        ]
    };
}
