use super::*;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SymbolName;
use paredit_core_syntax::sexpr::SyntaxTree;

fn same_structure(actual: &ExpressionView, expected: &ExpressionView) -> bool {
    let mut pending = vec![(actual, expected)];
    while let Some((actual, expected)) = pending.pop() {
        if actual.kind != expected.kind
            || actual.delimiter != expected.delimiter
            || actual.reader_prefixes != expected.reader_prefixes
            || actual.text != expected.text
            || actual.children.len() != expected.children.len()
        {
            return false;
        }
        pending.extend(actual.children.iter().zip(&expected.children));
    }
    true
}

#[test]
fn shadowed_do_parameter_regression() {
    let input = "(let ((do 0) (a 2)) (list a (lambda (do) do)))";
    let plan =
        plan_remove_unused_binding_for(input, Dialect::CommonLisp, None, Some("do"), false, true);
    assert_eq!(plan.reference_count, Some(0));
    assert!(plan.changed);
    let actual = SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
        .expect("parse rewritten binding");
    let expected = SyntaxTree::parse_with_dialect(
        "(let ((a 2)) (list a (lambda (do) do)))",
        Dialect::CommonLisp,
    )
    .expect("parse expected binding");
    assert!(same_structure(&actual.root_view(), &expected.root_view()));
}

#[test]
fn shadowed_binding_oracle_ignores_layout_but_rejects_semantic_changes() {
    let expected = SyntaxTree::parse_with_dialect(
        "(let ((a 2)) (list a (lambda (do) do)))",
        Dialect::CommonLisp,
    )
    .expect("parse expected binding");
    let reformatted = SyntaxTree::parse_with_dialect(
        "(let\n ((a 2))\n (list a\n (lambda (do)\n do)))",
        Dialect::CommonLisp,
    )
    .expect("parse reformatted binding");
    assert!(same_structure(
        &reformatted.root_view(),
        &expected.root_view()
    ));

    for mutated in [
        "(let ((do 0) (a 2)) (list a (lambda (do) do)))",
        "(let ((a 3)) (list a (lambda (do) do)))",
        "(let ((a 2)) (list a (lambda (other) do)))",
        "(let ((a 2)) (list a (lambda (do) a)))",
        "(let ((a 2)) (list a '(lambda (do) do)))",
        "(let ((a 2)) (list a (lambda (do) 'do)))",
        "(let ((a 2)) (list a (lambda (do) do))) extra",
    ] {
        let actual = SyntaxTree::parse_with_dialect(mutated, Dialect::CommonLisp)
            .expect("parse mutated binding");
        assert!(
            !same_structure(&actual.root_view(), &expected.root_view()),
            "{mutated}"
        );
    }
}

proptest! {
    #[test]
    fn pbt_unused_single_binding_output_remains_parseable(
        name in "[a-z][a-z0-9]{0,8}",
        value in "[-]?[0-9]{1,4}",
        body in "[a-z][a-z0-9]{0,8}",
    ) {
        prop_assume!(body != name);
        let input = format!("(let (({name} {value})) {body})");
        let symbol = SymbolName::new(name.clone())
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let plan = plan_remove_unused_binding(RemoveUnusedBindingRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            path: None,
            target: target(&input),
            name: Some(&symbol),
            all_bindings: false,
            allow_drop_value: true,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(plan.reference_count, Some(0));
        prop_assert_eq!(&plan.rewritten, &body);
        SyntaxTree::parse(&plan.rewritten)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }

    #[test]
    fn pbt_referenced_single_binding_is_rejected(
        name in "[a-z][a-z0-9]{0,8}",
        value in "[-]?[0-9]{1,4}",
    ) {
        let input = format!("(let (({name} {value})) {name})");
        let symbol = SymbolName::new(name.clone())
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let result = plan_remove_unused_binding(RemoveUnusedBindingRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            path: None,
            target: target(&input),
            name: Some(&symbol),
            all_bindings: false,
            allow_drop_value: true,
        });
        prop_assert!(result.is_err());
    }

    #[test]
    fn pbt_shadowed_lambda_parameter_does_not_keep_outer_binding(
        name in "[a-z][a-z0-9]{0,8}",
        other in "[a-z][a-z0-9]{0,8}",
        value in "[-]?[0-9]{1,4}",
    ) {
        prop_assume!(other != name);
        let input = format!(
            "(let (({name} {value}) ({other} 2)) (list {other} (lambda ({name}) {name})))"
        );
        let symbol = SymbolName::new(name.clone())
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let plan = plan_remove_unused_binding(RemoveUnusedBindingRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            path: None,
            target: target(&input),
            name: Some(&symbol),
            all_bindings: false,
            allow_drop_value: true,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(plan.reference_count, Some(0));
        prop_assert!(plan.changed);
        let actual = SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let expected = SyntaxTree::parse_with_dialect(
            &format!("(let (({other} 2)) (list {other} (lambda ({name}) {name})))"),
            Dialect::CommonLisp,
        )
        .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert!(same_structure(&actual.root_view(), &expected.root_view()));
    }
}
