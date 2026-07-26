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
fn plans_thread_first_pipeline_from_nested_calls() {
    let input = "(render (normalize value mode))";
    let (tree, target) = parsed(input, Dialect::Clojure);
    let plan = plan_thread_expression(ThreadExpressionRequest {
        input,
        tree: &tree,
        dialect: Dialect::Clojure,
        path: Some("0".parse().expect("path")),
        target,
        style: ThreadStyle::First,
        operator: SymbolName::new("->").expect("symbol"),
    })
    .expect("plan");

    assert_eq!(plan.base, "value");
    assert_eq!(plan.replacement, "(-> value (normalize mode) render)");
    assert_eq!(plan.steps.len(), 2);
    assert!(plan.changed);
}

#[test]
fn plans_thread_last_pipeline_from_nested_calls() {
    let input = "(map f (filter pred rows))";
    let (tree, target) = parsed(input, Dialect::Clojure);
    let plan = plan_thread_expression(ThreadExpressionRequest {
        input,
        tree: &tree,
        dialect: Dialect::Clojure,
        path: Some("0".parse().expect("path")),
        target,
        style: ThreadStyle::Last,
        operator: SymbolName::new("->>").expect("symbol"),
    })
    .expect("plan");

    assert_eq!(plan.base, "rows");
    assert_eq!(plan.replacement, "(->> rows (filter pred) (map f))");
    assert_eq!(plan.steps.len(), 2);
    assert!(plan.changed);
}

#[test]
fn rejects_package_qualified_already_threaded_expression() {
    let input = "(cl:-> x f)";
    let (tree, target) = parsed(input, Dialect::CommonLisp);
    let err = plan_thread_expression(ThreadExpressionRequest {
        input,
        tree: &tree,
        dialect: Dialect::CommonLisp,
        path: Some("0".parse().expect("path")),
        target,
        style: ThreadStyle::First,
        operator: SymbolName::new("->").expect("symbol"),
    })
    .expect_err("package-qualified thread operator should be rejected");

    assert!(err.to_string().contains("already threaded"));
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
        let input = "(render (normalize value mode))";
        let (tree, target) = parsed(input, dialect);
        let result = plan_thread_expression(ThreadExpressionRequest {
            input,
            tree: &tree,
            dialect,
            path: Some("0".parse().expect("path")),
            target,
            style: ThreadStyle::First,
            operator: SymbolName::new("->").expect("symbol"),
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
    let error = plan_thread_expression(ThreadExpressionRequest {
        input: ")",
        tree: &tree,
        dialect: Dialect::Unknown,
        path: Some("0".parse().expect("path")),
        target,
        style: ThreadStyle::First,
        operator: SymbolName::new("->").expect("symbol"),
    })
    .expect_err("unknown dialect should fail before malformed input and target checks");

    assert!(
        error
            .to_string()
            .contains("does not support dialect unknown")
    );
}

#[test]
fn clojure_namespace_does_not_match_an_unqualified_thread_operator() {
    let input = "(ns/-> value f)";
    let (tree, target) = parsed(input, Dialect::Clojure);
    let plan = plan_thread_expression(ThreadExpressionRequest {
        input,
        tree: &tree,
        dialect: Dialect::Clojure,
        path: Some("0".parse().expect("path")),
        target,
        style: ThreadStyle::First,
        operator: SymbolName::new("->").expect("symbol"),
    })
    .expect("namespace-qualified Clojure symbol is not the unqualified operator");

    assert_eq!(plan.replacement, "(-> value (ns/-> f))");
}

#[test]
fn preserves_dialect_reader_atoms_and_forms() {
    let cases = [
        (
            Dialect::CommonLisp,
            "(render (normalize #\\) mode))",
            "#\\)",
        ),
        (Dialect::EmacsLisp, "(render (normalize ?\\) mode))", "?\\)"),
        (
            Dialect::Clojure,
            "(render (normalize #foo/bar {:x 1} mode))",
            "#foo/bar {:x 1}",
        ),
    ];

    for (dialect, input, preserved) in cases {
        let (tree, target) = parsed(input, dialect);
        let plan = plan_thread_expression(ThreadExpressionRequest {
            input,
            tree: &tree,
            dialect,
            path: Some("0".parse().expect("path")),
            target,
            style: ThreadStyle::First,
            operator: SymbolName::new("->").expect("symbol"),
        })
        .unwrap_or_else(|error| panic!("{} reader case failed: {error}", dialect.label()));

        assert!(plan.rewritten.contains(preserved));
        SyntaxTree::parse_with_dialect(&plan.rewritten, dialect)
            .unwrap_or_else(|error| panic!("{} output did not reparse: {error}", dialect.label()));
    }
}

#[test]
fn rejects_nested_calls_with_an_interior_comment() {
    let input = "(render\n  ;; note\n  (normalize value mode))";
    let (tree, target) = parsed(input, Dialect::Clojure);
    let err = plan_thread_expression(ThreadExpressionRequest {
        input,
        tree: &tree,
        dialect: Dialect::Clojure,
        path: Some("0".parse().expect("path")),
        target,
        style: ThreadStyle::First,
        operator: SymbolName::new("->").expect("symbol"),
    })
    .expect_err("a comment inside the nested calls must not be silently discarded");

    assert!(err.to_string().contains("comment"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pbt_thread_first_output_is_parseable_and_stable(
        base in symbol_strategy(),
        inner in symbol_strategy(),
        outer in symbol_strategy(),
        arg in symbol_strategy(),
    ) {
        prop_assume!(base != inner);
        prop_assume!(base != outer);
        prop_assume!(inner != outer);

        let input = format!("({outer} ({inner} {base} {arg}))");
        let (tree, target) = parsed(&input, Dialect::Clojure);
        let plan = plan_thread_expression(ThreadExpressionRequest {
            input: &input,
            tree: &tree,
            dialect: Dialect::Clojure,
            path: Some("0".parse().expect("path")),
            target,
            style: ThreadStyle::First,
            operator: SymbolName::new("->").expect("symbol"),
        })
        .expect("plan");

        prop_assert_eq!(
            plan.replacement,
            format!("(-> {base} ({inner} {arg}) {outer})")
        );
        prop_assert!(SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::Clojure).is_ok());
        prop_assert!(plan.changed);
    }
}
