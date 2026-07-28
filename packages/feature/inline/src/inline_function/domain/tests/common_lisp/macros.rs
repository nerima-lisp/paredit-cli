use super::super::*;

#[test]
fn inlines_common_lisp_defmacro_quasiquote_with_unquote_splicing() {
    let input = "(defmacro collect (&rest values) `(list ,@values))\n(print (collect 1 2 3))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(list 1 2 3)");
    assert_eq!(
        plan.rewritten,
        "(defmacro collect (&rest values) `(list ,@values))\n(print (list 1 2 3))"
    );
}

#[test]
fn inlines_common_lisp_defmacro_with_whole_parameter() {
    let input = "(defmacro identity-form (&whole whole value) value)\n(print (identity-form 42))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "42");
    assert_eq!(
        plan.rewritten,
        "(defmacro identity-form (&whole whole value) value)\n(print 42)"
    );
}

#[test]
fn inlines_common_lisp_defmacro_with_whole_and_destructuring_parameter() {
    let input = "(defmacro inspect (&whole form (left right)) (list form left right))\n(print (inspect (a b)))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(list (inspect (a b)) a b)");
    assert_eq!(
        plan.rewritten,
        "(defmacro inspect (&whole form (left right)) (list form left right))\n(print (list (inspect (a b)) a b))"
    );
}

#[test]
fn inlines_common_lisp_defmacro_with_whole_and_aux_parameter() {
    let input = "(defmacro inspect (&whole form (value &aux (tag :seen))) (list form value tag))\n(print (inspect (a)))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(list (inspect (a)) a :seen)");
    assert_eq!(
        plan.rewritten,
        "(defmacro inspect (&whole form (value &aux (tag :seen))) (list form value tag))\n(print (list (inspect (a)) a :seen))"
    );
}

#[test]
fn inlines_common_lisp_defmacro_with_body_parameter() {
    let input = "(defmacro collect-forms (&body forms) `(progn ,@forms))\n(print (collect-forms (foo) (bar 1)))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(progn (foo) (bar 1))");
    assert_eq!(
        plan.rewritten,
        "(defmacro collect-forms (&body forms) `(progn ,@forms))\n(print (progn (foo) (bar 1)))"
    );
}

#[test]
fn inlines_common_lisp_defmacro_with_aux_dependency_chain() {
    let input = "(defmacro render-one (x &aux (y x) (z y)) (list y z))\n(print (render-one 1))";
    let plan = duplicate_evaluation_plan(input);

    assert_eq!(plan.calls[0].replacement, "(list 1 1)");
    assert_eq!(
        plan.rewritten,
        "(defmacro render-one (x &aux (y x) (z y)) (list y z))\n(print (list 1 1))"
    );
}

#[test]
fn inlines_common_lisp_define_compiler_macro() {
    let input = "(define-compiler-macro area (w h) `(* ,w ,h))\n(print (area 3 4))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(* 3 4)");
    assert_eq!(
        plan.rewritten,
        "(define-compiler-macro area (w h) `(* ,w ,h))\n(print (* 3 4))"
    );
}

#[test]
fn inlines_common_lisp_define_compiler_macro_with_whole_and_destructuring_parameter() {
    let input = "(define-compiler-macro inspect (&whole form (left right)) (list form right left))\n(print (inspect (a b)))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(list (inspect (a b)) b a)");
    assert_eq!(
        plan.rewritten,
        "(define-compiler-macro inspect (&whole form (left right)) (list form right left))\n(print (list (inspect (a b)) b a))"
    );
}

#[test]
fn inlines_common_lisp_define_compiler_macro_top_level_key_destructuring() {
    let input = "(define-compiler-macro wrap (&key ((:style (mode variant)) '(:plain :narrow) style-p)) (list mode variant style-p))\n(print (wrap :style (:bold :wide)))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(list :bold :wide t)");
    assert_eq!(
        plan.rewritten,
        "(define-compiler-macro wrap (&key ((:style (mode variant)) '(:plain :narrow) style-p)) (list mode variant style-p))\n(print (list :bold :wide t))"
    );
}

#[test]
fn inlines_common_lisp_defmacro_with_allow_other_keys_and_rest() {
    let input = "(defmacro render-one (x &rest rest &key (style :plain) &allow-other-keys) `(list ,x ,style ,@rest))\n(print (render-one 1 :style :bold :size 10))";
    let plan = inline_plan(input);

    assert_eq!(
        plan.calls[0].replacement,
        "(list 1 :bold :style :bold :size 10)"
    );
    assert_eq!(
        plan.rewritten,
        "(defmacro render-one (x &rest rest &key (style :plain) &allow-other-keys) `(list ,x ,style ,@rest))\n(print (list 1 :bold :style :bold :size 10))"
    );
}

#[test]
fn inlines_common_lisp_define_compiler_macro_with_allow_other_keys_and_rest() {
    let input = "(define-compiler-macro render-one (x &rest rest &key (style :plain) &allow-other-keys) `(list ,x ,style ,@rest))\n(print (render-one 1 :style :bold :size 10))";
    let plan = inline_plan(input);

    assert_eq!(
        plan.calls[0].replacement,
        "(list 1 :bold :style :bold :size 10)"
    );
    assert_eq!(
        plan.rewritten,
        "(define-compiler-macro render-one (x &rest rest &key (style :plain) &allow-other-keys) `(list ,x ,style ,@rest))\n(print (list 1 :bold :style :bold :size 10))"
    );
}

#[test]
fn inlines_common_lisp_defmacro_with_unused_environment_parameter() {
    let input = "(defmacro wrap (&environment env head &body body) `(list ,head ,@body))\n(print (wrap :x 1 2 3))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(list :x 1 2 3)");
    assert_eq!(
        plan.rewritten,
        "(defmacro wrap (&environment env head &body body) `(list ,head ,@body))\n(print (list :x 1 2 3))"
    );
}

#[test]
fn inlines_common_lisp_define_compiler_macro_with_unused_environment_parameter() {
    let input = "(define-compiler-macro wrap (&environment env head &body body) `(list ,head ,@body))\n(print (wrap :x 1 2 3))";
    let plan = inline_plan(input);

    assert_eq!(plan.calls[0].replacement, "(list :x 1 2 3)");
    assert_eq!(
        plan.rewritten,
        "(define-compiler-macro wrap (&environment env head &body body) `(list ,head ,@body))\n(print (list :x 1 2 3))"
    );
}

#[test]
fn rejects_common_lisp_define_setf_expander() {
    let input = "(define-setf-expander slot (place) (values nil nil '(setf slot) (list place)))\n(print (slot foo))";
    let error = inline_error(input, "setf expander inline must fail");

    assert!(error.to_string().contains("define-setf-expander"));
    assert!(
        error
            .to_string()
            .contains("setf expanders rewrite places, not ordinary call expressions")
    );
}

#[test]
fn rejects_common_lisp_defmacro_that_references_environment_parameter() {
    let input = "(defmacro inspect (&environment env value) env)\n(print (inspect 42))";
    let error = inline_error(input, "environment-sensitive macro must fail");

    assert!(
        error
            .to_string()
            .contains("cannot inline macros that reference &environment parameter 'env'")
    );
}

#[test]
fn rejects_common_lisp_defmacro_that_references_environment_parameter_in_aux() {
    let input =
        "(defmacro inspect (&environment env value &aux (tag env)) tag)\n(print (inspect 42))";
    let error = inline_error(
        input,
        "environment-sensitive macro aux initializer must fail",
    );

    assert!(error.to_string().contains(
        "cannot inline macros that reference &environment parameter 'env' in the &aux initializer"
    ));
}

#[test]
fn rejects_common_lisp_defmacro_that_references_environment_parameter_in_nested_optional_default() {
    let input = "(defmacro inspect (&environment env ((value &optional (tag env)))) tag)\n(print (inspect (a)))";
    let error = inline_error(
        input,
        "environment-sensitive macro nested optional initializer must fail",
    );

    assert!(error
        .to_string()
        .contains("cannot inline macros that reference &environment parameter 'env' in the nested &optional default value"));
}

#[test]
fn rejects_common_lisp_define_compiler_macro_that_references_environment_parameter() {
    let input =
        "(define-compiler-macro inspect (&environment env value) env)\n(print (inspect 42))";
    let error = inline_error(input, "environment-sensitive compiler macro must fail");

    assert!(
        error
            .to_string()
            .contains("cannot inline macros that reference &environment parameter 'env'")
    );
}

#[test]
fn rejects_common_lisp_defmacro_with_top_level_unquote_splicing() {
    let input = "(defmacro collect (&rest values) `,@values)\n(print (collect 1 2 3))";
    let error = inline_error(input, "top-level unquote-splicing must fail");

    assert!(
        error
            .to_string()
            .contains("unsupported top-level ,@expr in defmacro body")
    );
}
