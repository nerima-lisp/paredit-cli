use super::*;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use proptest::{prelude::*, test_runner::TestCaseError};

proptest! {
    /// Explores the shape the CLI property test explored - two existing
    /// parameters and `--all-calls`, so call discovery is on the path - at
    /// proptest's default 256 cases rather than the 24 a process-per-case
    /// test could afford. §12.4.
    #[test]
    fn pbt_add_parameter_output_remains_parseable(
        name in "[a-z][a-z0-9]{0,8}",
        a in "[a-z][a-z0-9]{0,8}",
        b in "[a-z][a-z0-9]{0,8}",
        c in "[a-z][a-z0-9]{0,8}",
        first in "[-]?[0-9]{1,4}",
        second in "[-]?[0-9]{1,4}",
        third in "[-]?[0-9]{1,4}",
    ) {
        prop_assume!(name != a && name != b && name != c);
        prop_assume!(a != b && a != c && b != c);
        let input = format!(
            "(defun {name} ({a} {b}) (list {a} {b} {c}))\n(print ({name} {first} {second}))\n"
        );
        let plan = plan_add_function_parameter(AddFunctionParameterRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            definition_path: path("0"),
            name: symbol(&c),
            argument: third.clone(),
            call_paths: Vec::new(),
            all_calls: true,
            insert: FunctionParameterInsert::End,
            section: FunctionParameterSection::Auto,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(
            &plan.rewritten,
            &format!(
                "(defun {name} ({a} {b} {c}) (list {a} {b} {c}))\n(print ({name} {first} {second} {third}))\n"
            )
        );
        SyntaxTree::parse(&plan.rewritten)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }

    /// Three parameters and `--all-calls`, matching the CLI property test the
    /// recorded shrink came from.
    #[test]
    fn pbt_move_parameter_output_remains_parseable(
        name in "[a-z][a-z0-9]{0,8}",
        a in "[a-z][a-z0-9]{0,8}",
        b in "[a-z][a-z0-9]{0,8}",
        c in "[a-z][a-z0-9]{0,8}",
        first in "[-]?[0-9]{1,4}",
        second in "[-]?[0-9]{1,4}",
        third in "[-]?[0-9]{1,4}",
    ) {
        prop_assume!(name != a && name != b && name != c);
        prop_assume!(a != b && a != c && b != c);
        let input = format!(
            "(defun {name} ({a} {b} {c}) (list {a} {b} {c}))\n(print ({name} {first} {second} {third}))\n"
        );
        let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            definition_path: path("0"),
            name: symbol(&c),
            to_index: 0,
            call_paths: Vec::new(),
            all_calls: true,
        })
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(
            &plan.rewritten,
            &format!(
                "(defun {name} ({c} {a} {b}) (list {a} {b} {c}))\n(print ({name} {third} {first} {second}))\n"
            )
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
