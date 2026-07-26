use super::{target, *};

proptest! {
    #[test]
    fn pbt_single_reference_inline_output_remains_parseable(
        name in "[a-z][a-z0-9]{0,8}",
        value in "[-]?[0-9]{1,4}",
        addend in "[-]?[0-9]{1,4}",
    ) {
        let input = format!("(let (({name} {value})) (+ {name} {addend}))");
        let plan = plan_inline_let(InlineLetRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            path: None,
            target: target(&input),
            allow_duplicate_evaluation: false,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(plan.reference_count, 1);
        let expected = format!("(+ {} {})", value, addend);
        prop_assert!(plan.rewritten.contains(&expected));
        SyntaxTree::parse(&plan.rewritten)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }

    #[test]
    fn pbt_duplicate_evaluation_policy_controls_multi_reference_rewrites(
        name in "[a-z][a-z0-9]{0,8}",
        function in "[a-z][a-z0-9]{0,8}",
    ) {
        let input = format!("(let (({name} ({function}))) (+ {name} {name}))");
        let default_result = plan_inline_let(InlineLetRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            path: None,
            target: target(&input),
            allow_duplicate_evaluation: false,
        });
        prop_assert!(default_result.is_err());

        let plan = plan_inline_let(InlineLetRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            path: None,
            target: target(&input),
            allow_duplicate_evaluation: true,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(plan.reference_count, 2);
        prop_assert_eq!(&plan.rewritten, &format!("(+ ({function}) ({function}))"));
        SyntaxTree::parse(&plan.rewritten)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }

    #[test]
    fn pbt_shadowed_lambda_parameter_is_not_inlined(
        name in "[a-z][a-z0-9]{0,8}",
        value in "[-]?[0-9]{1,4}",
    ) {
        let input = format!("(let (({name} {value})) (list {name} (lambda ({name}) {name})))");
        let plan = plan_inline_let(InlineLetRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            path: None,
            target: target(&input),
            allow_duplicate_evaluation: false,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(plan.reference_count, 1);
        let expected_lambda = format!("(lambda ({name}) {name})");
        prop_assert!(plan.rewritten.contains(&expected_lambda));
        prop_assert_eq!(&plan.rewritten, &format!("(list {value} {expected_lambda})"));
        SyntaxTree::parse(&plan.rewritten)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}
