use super::*;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SymbolName;
use paredit_core_syntax::sexpr::SyntaxTree;

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
        let expected_lambda = format!("(lambda ({name})\n     {name})");
        prop_assert!(plan.rewritten.contains(&expected_lambda));
        SyntaxTree::parse(&plan.rewritten)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}
