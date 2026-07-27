use super::*;
use proptest::{prelude::*, test_runner::TestCaseError};

proptest! {
    #[test]
    fn pbt_add_parameter_output_remains_parseable(
        name in "[a-z][a-z0-9]{0,8}",
        param in "[a-z][a-z0-9]{0,8}",
        added in "[a-z][a-z0-9]{0,8}",
        value in "[-]?[0-9]{1,4}",
        argument in "[-]?[0-9]{1,4}",
    ) {
        prop_assume!(name != param);
        prop_assume!(name != added);
        prop_assume!(param != added);
        let input = format!("(defun {name} ({param}) {param})\n(print ({name} {value}))");
        let plan = plan_add_function_parameter(AddFunctionParameterRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            definition_path: path("0"),
            name: symbol(&added),
            argument: argument.clone(),
            call_paths: vec![path("1.1")],
            all_calls: false,
            insert: FunctionParameterInsert::End,
            section: FunctionParameterSection::Auto,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(
            &plan.rewritten,
            &format!("(defun {name} ({param} {added}) {param})\n(print ({name} {value} {argument}))")
        );
        SyntaxTree::parse(&plan.rewritten)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }

    #[test]
    fn pbt_move_parameter_output_remains_parseable(
        name in "[a-z][a-z0-9]{0,8}",
        a in "[a-z][a-z0-9]{0,8}",
        b in "[a-z][a-z0-9]{0,8}",
        first in "[-]?[0-9]{1,4}",
        second in "[-]?[0-9]{1,4}",
    ) {
        prop_assume!(name != a);
        prop_assume!(name != b);
        prop_assume!(a != b);
        let input = format!("(defun {name} ({a} {b}) (list {a} {b}))\n(print ({name} {first} {second}))");
        let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            definition_path: path("0"),
            name: symbol(&b),
            to_index: 0,
            call_paths: vec![path("1.1")],
            all_calls: false,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(
            &plan.rewritten,
            &format!("(defun {name} ({b} {a}) (list {a} {b}))\n(print ({name} {second} {first}))")
        );
        SyntaxTree::parse(&plan.rewritten)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }

    #[test]
    fn pbt_remove_parameter_output_remains_parseable(
        name in "[a-z][a-z0-9]{0,8}",
        a in "[a-z][a-z0-9]{0,8}",
        b in "[a-z][a-z0-9]{0,8}",
        first in "[-]?[0-9]{1,4}",
        second in "[-]?[0-9]{1,4}",
    ) {
        prop_assume!(name != a);
        prop_assume!(name != b);
        prop_assume!(a != b);
        let input = format!("(defun {name} ({a} {b}) {a})\n(print ({name} {first} {second}))");
        let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            definition_path: path("0"),
            name: symbol(&b),
            call_paths: vec![path("1.1")],
            all_calls: false,
            missing_argument_policy: MissingArgumentPolicy::Reject,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(
            &plan.rewritten,
            &format!("(defun {name} ({a}) {a})\n(print ({name} {first}))")
        );
        SyntaxTree::parse(&plan.rewritten)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }

    #[test]
    fn pbt_swap_parameters_output_remains_parseable(
        name in "[a-z][a-z0-9]{0,8}",
        a in "[a-z][a-z0-9]{0,8}",
        b in "[a-z][a-z0-9]{0,8}",
        c in "[a-z][a-z0-9]{0,8}",
        first in "[-]?[0-9]{1,4}",
        second in "[-]?[0-9]{1,4}",
        third in "[-]?[0-9]{1,4}",
    ) {
        prop_assume!(name != a);
        prop_assume!(name != b);
        prop_assume!(name != c);
        prop_assume!(a != b);
        prop_assume!(a != c);
        prop_assume!(b != c);
        let input = format!("(defun {name} ({a} {b} {c}) (list {a} {b} {c}))\n(print ({name} {first} {second} {third}))");
        let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            definition_path: path("0"),
            left_name: symbol(&a),
            right_name: symbol(&c),
            call_paths: vec![path("1.1")],
            all_calls: false,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(
            &plan.rewritten,
            &format!("(defun {name} ({c} {b} {a}) (list {a} {b} {c}))\n(print ({name} {third} {second} {first}))")
        );
        SyntaxTree::parse(&plan.rewritten)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }

    #[test]
    fn pbt_reorder_parameters_output_remains_parseable(
        name in "[a-z][a-z0-9]{0,8}",
        a in "[a-z][a-z0-9]{0,8}",
        b in "[a-z][a-z0-9]{0,8}",
        c in "[a-z][a-z0-9]{0,8}",
        first in "[-]?[0-9]{1,4}",
        second in "[-]?[0-9]{1,4}",
        third in "[-]?[0-9]{1,4}",
    ) {
        prop_assume!(name != a);
        prop_assume!(name != b);
        prop_assume!(name != c);
        prop_assume!(a != b);
        prop_assume!(a != c);
        prop_assume!(b != c);
        let input = format!("(defun {name} ({a} {b} {c}) (list {a} {b} {c}))\n(print ({name} {first} {second} {third}))");
        let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            definition_path: path("0"),
            parameter_order: vec![symbol(&c), symbol(&a), symbol(&b)],
            call_paths: vec![path("1.1")],
            all_calls: false,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(
            &plan.rewritten,
            &format!("(defun {name} ({c} {a} {b}) (list {a} {b} {c}))\n(print ({name} {third} {first} {second}))")
        );
        SyntaxTree::parse(&plan.rewritten)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}
