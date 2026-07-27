use super::*;

#[test]
fn renames_define_symbol_macro_definitions_and_value_references() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo *session*) (list foo (foo 1) (setf foo 2))\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: ["(define-symbol-macro bar *session*)", "(list bar (foo 1) (setf bar 2))"]
    };
}

#[test]
fn skips_references_inside_define_symbol_macro_expansion() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo (list foo :tag)) (list foo (setf foo 1))\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: ["(define-symbol-macro bar (list foo :tag))", "(list bar (setf bar 1))"]
    };
}

#[test]
fn renames_define_symbol_macro_inside_progn_without_touching_its_expansion() {
    assert_symbol_macro_rename! {
        input: "(progn (define-symbol-macro foo (list foo :tag)) foo)\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(define-symbol-macro bar (list foo :tag))", "bar)"]
    };
}

#[test]
fn renames_define_symbol_macro_inside_eval_when_without_touching_its_expansion() {
    assert_symbol_macro_rename! {
        input: "(eval-when (:compile-toplevel :load-toplevel :execute) (define-symbol-macro foo (list foo :tag)) foo)\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(define-symbol-macro bar (list foo :tag))", "bar)"]
    };
}

#[test]
fn renames_define_symbol_macro_inside_when_without_touching_its_expansion() {
    assert_symbol_macro_rename! {
        input: "(when ready (define-symbol-macro foo (list foo :tag)) foo)\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 1,
        changed: true,
        rewritten_contains: ["(define-symbol-macro bar (list foo :tag))", "bar)"]
    };
}

#[test]
fn renames_define_symbol_macro_inside_reader_quoted_lambda_body() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) #'(lambda () (define-symbol-macro foo (list foo :tag)) foo) foo\n",
        from: "foo",
        to: "bar",
        definitions: 2,
        references: 2,
        changed: true,
        rewritten_contains: [
            "(define-symbol-macro bar current-user)",
            "#'(lambda () (define-symbol-macro bar (list foo :tag)) bar) bar"
        ]
    };
}

#[test]
fn skips_macrolet_inside_define_symbol_macro_expansion_while_renaming_outer_references() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo (macrolet ((foo (x) (list foo x))) (foo 1) foo)) (list foo (setf foo 2))\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: [
            "(define-symbol-macro bar (macrolet ((foo (x) (list foo x))) (foo 1) foo))",
            "(list bar (setf bar 2))"
        ]
    };
}

#[test]
fn renames_common_lisp_qualified_define_symbol_macro_definition_and_references() {
    assert_symbol_macro_rename! {
        input: "(cl-user:define-symbol-macro foo current-user) (list foo (foo 1) (setf foo 2))\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: ["(cl-user:define-symbol-macro bar current-user)", "(list bar (foo 1) (setf bar 2))"]
    };
}

#[test]
fn renames_cl_qualified_define_symbol_macro_definition_and_references() {
    assert_symbol_macro_rename! {
        input: "(cl:define-symbol-macro foo current-user) (list foo (foo 1) (setf foo 2))\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: ["(cl:define-symbol-macro bar current-user)", "(list bar (foo 1) (setf bar 2))"]
    };
}

#[test]
fn skips_outer_symbol_macro_references_inside_define_setf_expander_bodies() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (list foo (define-setf-expander slot (place) (list foo place)) foo)\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: [
            "(define-symbol-macro bar current-user)",
            "(list bar (define-setf-expander slot (place) (list foo place)) bar)"
        ]
    };
}

#[test]
fn skips_outer_symbol_macro_references_inside_define_compiler_macro_bodies() {
    assert_symbol_macro_rename! {
        input: "(define-symbol-macro foo current-user) (list foo (define-compiler-macro render (place) (list foo place)) foo)\n",
        from: "foo",
        to: "bar",
        definitions: 1,
        references: 2,
        changed: true,
        rewritten_contains: [
            "(define-symbol-macro bar current-user)",
            "(list bar (define-compiler-macro render (place) (list foo place)) bar)"
        ]
    };
}
