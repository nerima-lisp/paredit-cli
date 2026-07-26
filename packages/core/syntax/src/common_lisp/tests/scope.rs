use super::*;

#[test]
fn recognizes_common_lisp_local_callable_heads_with_package_prefixes() {
    assert_eq!(
        common_lisp_local_callable_form(Dialect::CommonLisp, "cl:macrolet"),
        Some(CommonLispLocalCallableForm::Macrolet)
    );
    assert_eq!(
        common_lisp_local_callable_form(Dialect::Unknown, "cl:compiler-macrolet"),
        Some(CommonLispLocalCallableForm::CompilerMacrolet)
    );
    assert_eq!(
        common_lisp_local_callable_form(Dialect::CommonLisp, "cl:flet"),
        Some(CommonLispLocalCallableForm::Flet)
    );
    assert_eq!(
        common_lisp_local_callable_form(Dialect::CommonLisp, "cl-user:labels"),
        Some(CommonLispLocalCallableForm::Labels)
    );
}

#[test]
fn rejects_qualified_local_callable_heads_outside_supported_dialects() {
    assert_eq!(
        common_lisp_local_callable_form(Dialect::Clojure, "cl:macrolet"),
        None
    );
    assert_eq!(
        common_lisp_local_callable_form(Dialect::Scheme, "cl:flet"),
        None
    );
}

#[test]
fn computes_labels_scope_for_binding_and_body_paths() {
    let tree = SyntaxTree::parse("(labels ((foo (x) (foo x))) (foo 1))").expect("parse labels");

    let binding_scope = local_callable_scope_at_path(
        &tree,
        Dialect::CommonLisp,
        &"0.1.0.2".parse().expect("binding path"),
    )
    .expect("binding scope");
    let body_scope = local_callable_scope_at_path(
        &tree,
        Dialect::CommonLisp,
        &"0.2".parse().expect("body path"),
    )
    .expect("body scope");

    assert!(is_local_callable_bound(&binding_scope, "foo"));
    assert!(is_local_callable_bound(&body_scope, "foo"));
}

#[test]
fn matches_local_callable_names_after_package_prefix_normalization() {
    let scope = vec!["COMMON-LISP-USER:FOO".to_owned()];

    assert!(is_local_callable_bound(&scope, "foo"));
    assert!(is_local_callable_bound(&scope, "cl-user:foo"));
}

#[test]
fn computes_flet_scope_without_binding_body_visibility() {
    let tree = SyntaxTree::parse("(flet ((foo (x) (foo x))) (foo 1))").expect("parse flet");

    let binding_scope = local_callable_scope_at_path(
        &tree,
        Dialect::CommonLisp,
        &"0.1.0.2".parse().expect("binding path"),
    )
    .expect("binding scope");
    let body_scope = local_callable_scope_at_path(
        &tree,
        Dialect::CommonLisp,
        &"0.2".parse().expect("body path"),
    )
    .expect("body scope");

    assert!(!is_local_callable_bound(&binding_scope, "foo"));
    assert!(is_local_callable_bound(&body_scope, "foo"));
}

#[test]
fn computes_compiler_macrolet_scope_without_expander_body_visibility() {
    let tree = SyntaxTree::parse("(compiler-macrolet ((foo (x) (foo x))) (foo 1))")
        .expect("parse compiler-macrolet");

    let expander_scope = local_callable_scope_at_path(
        &tree,
        Dialect::CommonLisp,
        &"0.1.0.2".parse().expect("expander path"),
    )
    .expect("expander scope");
    let body_scope = local_callable_scope_at_path(
        &tree,
        Dialect::CommonLisp,
        &"0.2".parse().expect("body path"),
    )
    .expect("body scope");

    assert!(!is_local_callable_bound(&expander_scope, "foo"));
    assert!(is_local_callable_bound(&body_scope, "foo"));
}

#[test]
fn extends_local_callable_body_scope_with_binding_names() {
    let tree = SyntaxTree::parse("(flet ((foo (x) x) (bar (y) y)) (foo (bar 1)))")
        .expect("parse local callable form");
    let view = tree
        .select_path(&"0".parse().expect("form path"))
        .expect("select form")
        .view();

    let scope = local_callable_body_scope(&["outer".to_owned()], &view);

    assert_eq!(
        scope,
        vec!["outer".to_owned(), "foo".to_owned(), "bar".to_owned()]
    );
}

#[test]
fn extracts_setf_local_callable_names_from_flet_bindings() {
    let tree =
        SyntaxTree::parse("(flet (((cl-user:setf foo) (value object) value) (bar (y) y)) (foo 1))")
            .expect("parse local callable form");
    let view = tree
        .select_path(&"0".parse().expect("form path"))
        .expect("select form")
        .view();

    let names = local_callable_names(&view);

    assert_eq!(names, vec!["foo".to_owned(), "bar".to_owned()]);
    assert!(is_local_callable_bound(&names, "cl-user:foo"));
    assert!(is_local_callable_bound(&names, "bar"));
}

#[test]
fn distinguishes_labels_and_flet_binding_body_scope() {
    let local_scope = vec!["outer".to_owned()];
    let body_scope = vec!["outer".to_owned(), "foo".to_owned()];

    assert_eq!(
        local_callable_binding_body_scope(
            CommonLispLocalCallableForm::Labels,
            &local_scope,
            &body_scope,
        ),
        body_scope.as_slice()
    );
    assert_eq!(
        local_callable_binding_body_scope(
            CommonLispLocalCallableForm::Flet,
            &local_scope,
            &body_scope,
        ),
        local_scope.as_slice()
    );
    assert_eq!(
        local_callable_binding_body_scope(
            CommonLispLocalCallableForm::Macrolet,
            &local_scope,
            &body_scope,
        ),
        local_scope.as_slice()
    );
}

#[test]
fn distinguishes_definition_reference_scope_for_labels_and_flet() {
    let local = vec!["outer".to_owned()];
    let body = vec!["outer".to_owned(), "helper".to_owned()];

    assert_eq!(
        local_callable_definition_reference_scope(
            CommonLispLocalCallableForm::Labels,
            &local,
            &body
        ),
        local.as_slice()
    );
    assert_eq!(
        local_callable_definition_reference_scope(CommonLispLocalCallableForm::Flet, &local, &body),
        body.as_slice()
    );
}
