use super::*;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionView, Path, SymbolName};
use proptest::prelude::*;

fn parsed(input: &str, dialect: Dialect) -> (SyntaxTree, ExpressionView) {
    let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse fixture");
    let target = tree
        .select_path(&"0".parse::<Path>().expect("path"))
        .expect("select fixture")
        .view();
    (tree, target)
}

fn symbol_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,8}".prop_filter("reserved symbol", |name| {
        !matches!(
            name.as_str(),
            "defun" | "false" | "fn" | "lambda" | "let" | "nil" | "t" | "true"
        )
    })
}

#[test]
fn plans_unthread_first_pipeline_into_nested_call() {
    let input = "(-> value (normalize mode) render)";
    let (tree, target) = parsed(input, Dialect::Clojure);
    let plan = plan_unthread_expression(UnthreadExpressionRequest {
        input,
        tree: &tree,
        dialect: Dialect::Clojure,
        path: Some("0".parse().expect("path")),
        target,
        style: None,
        operator: None,
    })
    .expect("plan");

    assert_eq!(plan.style, UnthreadStyle::First);
    assert_eq!(plan.base, "value");
    assert_eq!(plan.replacement, "(render (normalize value mode))");
    assert_eq!(plan.steps.len(), 2);
    assert!(plan.changed);
}

#[test]
fn plans_unthread_last_pipeline_into_nested_call() {
    let input = "(->> rows (filter pred) (map f))";
    let (tree, target) = parsed(input, Dialect::Clojure);
    let plan = plan_unthread_expression(UnthreadExpressionRequest {
        input,
        tree: &tree,
        dialect: Dialect::Clojure,
        path: Some("0".parse().expect("path")),
        target,
        style: None,
        operator: None,
    })
    .expect("plan");

    assert_eq!(plan.style, UnthreadStyle::Last);
    assert_eq!(plan.base, "rows");
    assert_eq!(plan.replacement, "(map f (filter pred rows))");
    assert_eq!(plan.steps.len(), 2);
    assert!(plan.changed);
}

#[test]
fn rejects_style_alone_on_an_unrecognized_operator() {
    let input = "(+ a b)";
    let (tree, target) = parsed(input, Dialect::CommonLisp);
    let err = plan_unthread_expression(UnthreadExpressionRequest {
        input,
        tree: &tree,
        dialect: Dialect::CommonLisp,
        path: Some("0".parse().expect("path")),
        target,
        style: Some(UnthreadStyle::First),
        operator: None,
    })
    .expect_err("bare --style must not accept an unrecognized operator");

    assert!(err.to_string().contains("--operator"));
}

#[test]
fn accepts_style_with_explicit_operator_confirming_a_custom_pipeline() {
    let input = "(my-pipe value (normalize mode) render)";
    let (tree, target) = parsed(input, Dialect::CommonLisp);
    let plan = plan_unthread_expression(UnthreadExpressionRequest {
        input,
        tree: &tree,
        dialect: Dialect::CommonLisp,
        path: Some("0".parse().expect("path")),
        target,
        style: Some(UnthreadStyle::First),
        operator: Some(SymbolName::new("my-pipe").expect("symbol")),
    })
    .expect("plan");

    assert_eq!(plan.replacement, "(render (normalize value mode))");
}

#[test]
fn supports_known_dialects_and_rejects_unknown() {
    let cases = [
        (Dialect::CommonLisp, true),
        (Dialect::EmacsLisp, true),
        (Dialect::Scheme, true),
        (Dialect::Clojure, true),
        (Dialect::Janet, true),
        (Dialect::Fennel, true),
        (Dialect::Unknown, false),
    ];

    for (dialect, supported) in cases {
        let input = "(-> value (normalize mode) render)";
        let (tree, target) = parsed(input, dialect);
        let result = plan_unthread_expression(UnthreadExpressionRequest {
            input,
            tree: &tree,
            dialect,
            path: Some("0".parse().expect("path")),
            target,
            style: None,
            operator: None,
        });

        if supported {
            let plan = result
                .unwrap_or_else(|error| panic!("{} should be supported: {error}", dialect.label()));
            assert!(
                SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).is_ok(),
                "{} output should parse in the same dialect",
                dialect.label()
            );
        } else {
            let error = result.expect_err("unknown dialect should be rejected");
            assert!(
                error
                    .to_string()
                    .contains("does not support dialect unknown")
            );
        }
    }
}

#[test]
fn rejects_unknown_before_reader_and_target_validation() {
    let (tree, target) = parsed("atom", Dialect::CommonLisp);
    let error = plan_unthread_expression(UnthreadExpressionRequest {
        input: ")",
        tree: &tree,
        dialect: Dialect::Unknown,
        path: Some("0".parse().expect("path")),
        target,
        style: None,
        operator: None,
    })
    .expect_err("unknown dialect should fail before malformed input and target checks");

    assert!(
        error
            .to_string()
            .contains("does not support dialect unknown")
    );
}

#[test]
fn preserves_dialect_reader_atoms_and_forms() {
    let cases = [
        (
            Dialect::CommonLisp,
            "(-> #\\) (normalize mode) render)",
            "#\\)",
        ),
        (
            Dialect::EmacsLisp,
            "(-> ?\\) (normalize mode) render)",
            "?\\)",
        ),
        (
            Dialect::Clojure,
            "(-> #foo/bar {:x 1} (normalize mode) render)",
            "#foo/bar {:x 1}",
        ),
    ];

    for (dialect, input, preserved) in cases {
        let (tree, target) = parsed(input, dialect);
        let plan = plan_unthread_expression(UnthreadExpressionRequest {
            input,
            tree: &tree,
            dialect,
            path: Some("0".parse().expect("path")),
            target,
            style: None,
            operator: None,
        })
        .unwrap_or_else(|error| panic!("{} reader case failed: {error}", dialect.label()));

        assert!(plan.rewritten.contains(preserved));
        SyntaxTree::parse_with_dialect(&plan.rewritten, dialect)
            .unwrap_or_else(|error| panic!("{} output did not reparse: {error}", dialect.label()));
    }
}

#[test]
fn rejects_pipeline_with_an_interior_comment() {
    let input = "(-> value\n    ;; note\n    (normalize mode)\n    render)";
    let (tree, target) = parsed(input, Dialect::Clojure);
    let err = plan_unthread_expression(UnthreadExpressionRequest {
        input,
        tree: &tree,
        dialect: Dialect::Clojure,
        path: Some("0".parse().expect("path")),
        target,
        style: None,
        operator: None,
    })
    .expect_err("a comment inside the pipeline must not be silently discarded");

    assert!(err.to_string().contains("comment"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_unthread_first_output_is_parseable_and_stable(
        base in symbol_strategy(),
        inner in symbol_strategy(),
        outer in symbol_strategy(),
        arg in symbol_strategy(),
    ) {
        prop_assume!(base != inner);
        prop_assume!(base != outer);
        prop_assume!(inner != outer);

        let input = format!("(-> {base} ({inner} {arg}) {outer})");
        let (tree, target) = parsed(&input, Dialect::Clojure);
        let plan = plan_unthread_expression(UnthreadExpressionRequest {
            input: &input,
            tree: &tree,
            dialect: Dialect::Clojure,
            path: Some("0".parse().expect("path")),
            target,
            style: None,
            operator: None,
        })
        .expect("plan");

        prop_assert_eq!(
            plan.replacement,
            format!("({outer} ({inner} {base} {arg}))")
        );
        prop_assert!(SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::Clojure).is_ok());
        prop_assert!(plan.changed);
    }
}
