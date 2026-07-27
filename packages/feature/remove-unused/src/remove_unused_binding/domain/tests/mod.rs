use proptest::{prelude::*, test_runner::TestCaseError};

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionView, Path, SymbolName, SyntaxTree};

use super::{RemoveUnusedBindingPlan, RemoveUnusedBindingRequest, plan_remove_unused_binding};

mod clojure;
mod common_lisp;
mod pbt;

fn symbol(name: &str) -> SymbolName {
    SymbolName::new(name).expect("symbol")
}

fn target(input: &str) -> ExpressionView {
    target_at(input, "0")
}

fn target_at(input: &str, path: &str) -> ExpressionView {
    let tree = SyntaxTree::parse(input).expect("parse");
    tree.select_path(&path.parse::<Path>().expect("path"))
        .expect("select")
        .view()
}

fn target_at_with_dialect(input: &str, path: &str, dialect: Dialect) -> ExpressionView {
    let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
    tree.select_path(&path.parse::<Path>().expect("path"))
        .expect("select")
        .view()
}

fn plan_remove_unused_binding_for(
    input: &str,
    dialect: Dialect,
    path: Option<&str>,
    name: Option<&str>,
    all_bindings: bool,
    allow_drop_value: bool,
) -> RemoveUnusedBindingPlan {
    let parsed_path = path.map(|path| path.parse().expect("path"));
    let symbol = name.map(symbol);
    plan_remove_unused_binding(RemoveUnusedBindingRequest {
        input,
        dialect,
        path: parsed_path,
        target: target_at_with_dialect(input, path.unwrap_or("0"), dialect),
        name: symbol.as_ref(),
        all_bindings,
        allow_drop_value,
    })
    .expect("plan")
}

fn remove_unused_binding_error(
    input: &str,
    dialect: Dialect,
    name: Option<&str>,
    all_bindings: bool,
    allow_drop_value: bool,
) -> String {
    let symbol = name.map(symbol);
    plan_remove_unused_binding(RemoveUnusedBindingRequest {
        input,
        dialect,
        path: None,
        target: target_at_with_dialect(input, "0", dialect),
        name: symbol.as_ref(),
        all_bindings,
        allow_drop_value,
    })
    .expect_err("expected remove-unused-binding to fail")
    .to_string()
}

fn remove_unused_binding_error_for(
    input: &str,
    dialect: Dialect,
    path: Option<&str>,
    name: Option<&str>,
    all_bindings: bool,
    allow_drop_value: bool,
) -> String {
    let parsed_path = path.map(|path| path.parse().expect("path"));
    let symbol = name.map(symbol);
    plan_remove_unused_binding(RemoveUnusedBindingRequest {
        input,
        dialect,
        path: parsed_path,
        target: target_at_with_dialect(input, path.unwrap_or("0"), dialect),
        name: symbol.as_ref(),
        all_bindings,
        allow_drop_value,
    })
    .expect_err("expected remove-unused-binding to fail")
    .to_string()
}

#[test]
fn rejects_target_that_does_not_match_input() {
    let source = "(let ((x 1)) x)";
    let symbol = symbol("x");

    let error = plan_remove_unused_binding(RemoveUnusedBindingRequest {
        input: "(é ((x 1)) x)",
        dialect: Dialect::CommonLisp,
        path: None,
        target: target(source),
        name: Some(&symbol),
        all_bindings: false,
        allow_drop_value: true,
    })
    .unwrap_err();

    assert!(error.to_string().contains("does not match the input"));
}

#[test]
fn supports_known_dialects_and_rejects_unknown() {
    let fixtures = [
        (Dialect::CommonLisp, "(let ((unused 1) (used 2)) used)"),
        (Dialect::EmacsLisp, "(let ((unused 1) (used 2)) used)"),
        (Dialect::Scheme, "(let ((unused 1) (used 2)) used)"),
        (Dialect::Clojure, "(let [unused 1 used 2] used)"),
        (Dialect::Janet, "(let [unused 1 used 2] used)"),
        (Dialect::Fennel, "(let [unused 1 used 2] used)"),
    ];

    for (dialect, input) in fixtures {
        let plan =
            plan_remove_unused_binding_for(input, dialect, None, Some("unused"), false, true);

        assert_eq!(plan.binding_name.as_deref(), Some("unused"));
        assert!(!plan.rewritten.contains("unused"));
        SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).expect("reparse rewritten output");
    }

    let error = remove_unused_binding_error(
        "(let ((unused 1) (used 2)) used)",
        Dialect::Unknown,
        Some("unused"),
        false,
        true,
    );
    assert!(error.contains("does not support dialect unknown"));
}

#[test]
fn unknown_dialect_fails_before_parsing_malformed_input() {
    let symbol = symbol("unused");
    let error = plan_remove_unused_binding(RemoveUnusedBindingRequest {
        input: "(",
        dialect: Dialect::Unknown,
        path: None,
        target: target("(let ((unused 1)) 2)"),
        name: Some(&symbol),
        all_bindings: false,
        allow_drop_value: true,
    })
    .expect_err("unknown dialect must fail")
    .to_string();

    assert!(error.contains("does not support dialect unknown"));
}

#[test]
fn non_common_lisp_binding_names_are_case_sensitive() {
    let fixtures = [
        (Dialect::EmacsLisp, "(let ((foo 1) (used 2)) used)"),
        (Dialect::Scheme, "(let ((foo 1) (used 2)) used)"),
        (Dialect::Clojure, "(let [foo 1 used 2] used)"),
        (Dialect::Janet, "(let [foo 1 used 2] used)"),
        (Dialect::Fennel, "(let [foo 1 used 2] used)"),
    ];

    for (dialect, input) in fixtures {
        let error = remove_unused_binding_error(input, dialect, Some("FOO"), false, true);
        assert!(
            error.contains("binding FOO was not found"),
            "{dialect:?}: {error}"
        );
    }

    let common_lisp = common_lisp_plan("(let ((foo 1) (used 2)) used)", Some("FOO"), false, true);
    assert_eq!(common_lisp.binding_name.as_deref(), Some("foo"));
}

#[test]
fn preserves_dialect_reader_forms_during_rewrite() {
    let fixtures = [
        (
            Dialect::CommonLisp,
            "(let ((unused 1) (used (list #\\) #:done #x2a))) used)",
            &["#\\)", "#:done", "#x2a"][..],
        ),
        (
            Dialect::EmacsLisp,
            "(let ((unused 1) (used (list ?\\)))) used)",
            &["?\\)"][..],
        ),
        (
            Dialect::Clojure,
            "(let [unused 1 used (list #foo/bar #:person{:x 1})] used)",
            &["#foo/bar", "#:person{:x 1}"][..],
        ),
    ];

    for (dialect, input, preserved) in fixtures {
        let plan =
            plan_remove_unused_binding_for(input, dialect, None, Some("unused"), false, true);

        for reader_form in preserved {
            assert!(
                plan.rewritten.contains(reader_form),
                "{dialect:?}: {reader_form}"
            );
        }
        SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).expect("reparse rewritten output");
    }
}

fn common_lisp_plan(
    input: &str,
    name: Option<&str>,
    all_bindings: bool,
    allow_drop_value: bool,
) -> RemoveUnusedBindingPlan {
    plan_remove_unused_binding_for(
        input,
        Dialect::CommonLisp,
        None,
        name,
        all_bindings,
        allow_drop_value,
    )
}

fn common_lisp_error(
    input: &str,
    name: Option<&str>,
    all_bindings: bool,
    allow_drop_value: bool,
) -> String {
    remove_unused_binding_error(
        input,
        Dialect::CommonLisp,
        name,
        all_bindings,
        allow_drop_value,
    )
}
