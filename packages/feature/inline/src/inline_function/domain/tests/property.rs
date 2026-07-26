use proptest::{prelude::*, test_runner::TestCaseError};

use super::*;

proptest! {
    #[test]
    fn pbt_single_reference_inline_output_remains_parseable(
        name in "[a-z][a-z0-9]{0,8}",
        param in "[a-z][a-z0-9]{0,8}",
        argument in "[-]?[0-9]{1,4}",
        addend in "[-]?[0-9]{1,4}",
    ) {
        let input = format!("(defun {name} ({param}) (+ {param} {addend}))\n(print ({name} {argument}))");
        let plan = plan_inline_function(default_inline_request(&input))
            .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(plan.calls[0].parameters[0].reference_count, 1);
        prop_assert_eq!(
            &plan.rewritten,
            &format!("(defun {name} ({param}) (+ {param} {addend}))\n(print (+ {argument} {addend}))")
        );
        SyntaxTree::parse_with_dialect(&plan.rewritten, plan.dialect)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }

    #[test]
    fn pbt_duplicate_evaluation_policy_controls_multi_reference_rewrites(
        name in "[a-z][a-z0-9]{0,8}",
        param in "[a-z][a-z0-9]{0,8}",
        callee in "[a-z][a-z0-9]{0,8}",
    ) {
        let input = format!("(defun {name} ({param}) (+ {param} {param}))\n(print ({name} ({callee})))");
        let default_result = plan_inline_function(default_inline_request(&input));
        prop_assert!(default_result.is_err());

        let plan = duplicate_evaluation_plan(&input);

        prop_assert_eq!(plan.calls[0].parameters[0].reference_count, 2);
        prop_assert_eq!(
            &plan.rewritten,
            &format!("(defun {name} ({param}) (+ {param} {param}))\n(print (+ ({callee}) ({callee})))")
        );
        SyntaxTree::parse_with_dialect(&plan.rewritten, plan.dialect)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}
