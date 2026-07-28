//! The inputs the CLI property tests shrank to when they found real bugs.
//!
//! §12.4 moves the exhaustive search in-process, and §12.2 warns that the
//! recorded shrinks "must be carried over in any option". A proptest
//! regressions file cannot carry across that move: it is keyed by seed, and a
//! seed only means anything to the test and input shape that produced it. The
//! in-process tests take a different shape from the CLI ones, so replaying the
//! seed would explore different inputs and prove nothing.
//!
//! Carrying them over faithfully means asserting the shrunk values directly.
//! These are the exact inputs recorded in
//! `tests/cli/function_parameter/*/property.proptest-regressions`, each one a
//! case that used to fail.

use super::*;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

/// `tests/cli/function_parameter/add/property.proptest-regressions`, both
/// recorded shrinks.
#[test]
fn recorded_add_parameter_shrinks_still_rewrite_correctly() {
    for (name, a, b, c, first, second, third) in [
        ("d1r", "y", "cs19612", "fn6", "2765", "-13", "72"),
        ("s6z1bx", "yyu5r3l", "wj3p547su", "nwty", "6259", "-2", "3"),
    ] {
        let input = format!(
            "(defun {name} ({a} {b}) (list {a} {b} {c}))\n(print ({name} {first} {second}))\n"
        );
        let plan = plan_add_function_parameter(AddFunctionParameterRequest {
            input: &input,
            dialect: Dialect::CommonLisp,
            definition_path: path("0"),
            name: symbol(c),
            argument: third.to_owned(),
            call_paths: Vec::new(),
            all_calls: true,
            insert: FunctionParameterInsert::End,
            section: FunctionParameterSection::Auto,
        })
        .expect("recorded shrink must still plan");

        assert_eq!(
            plan.rewritten,
            format!(
                "(defun {name} ({a} {b} {c}) (list {a} {b} {c}))\n(print ({name} {first} {second} {third}))\n"
            ),
            "recorded shrink: {name} {a} {b} {c}"
        );
        SyntaxTree::parse(&plan.rewritten).expect("rewritten output must reparse");
    }
}

/// `tests/cli/function_parameter/move_parameter/property.proptest-regressions`.
#[test]
fn recorded_move_parameter_shrink_still_rewrites_correctly() {
    let (name, a, b, c, first, second, third) = (
        "i966i",
        "on8t",
        "e26s9v4m2",
        "buw995uj",
        "-6837",
        "75",
        "-78",
    );
    let input = format!(
        "(defun {name} ({a} {b} {c}) (list {a} {b} {c}))\n(print ({name} {first} {second} {third}))\n"
    );
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input: &input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol(c),
        to_index: 0,
        call_paths: Vec::new(),
        all_calls: true,
    })
    .expect("recorded shrink must still plan");

    assert_eq!(
        plan.rewritten,
        format!(
            "(defun {name} ({c} {a} {b}) (list {a} {b} {c}))\n(print ({name} {third} {first} {second}))\n"
        )
    );
    SyntaxTree::parse(&plan.rewritten).expect("rewritten output must reparse");
}

/// `tests/cli/function_parameter/swap/property.proptest-regressions`.
#[test]
fn recorded_swap_parameters_shrink_still_rewrites_correctly() {
    let (name, a, b, c, first, second, third) =
        ("s78", "su43s6", "nw4vdj8", "ti08do", "-6352", "-861", "877");
    let input = format!(
        "(defun {name} ({a} {b} {c}) (list {a} {b} {c}))\n(print ({name} {first} {second} {third}))\n"
    );
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input: &input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        left_name: symbol(a),
        right_name: symbol(c),
        call_paths: Vec::new(),
        all_calls: true,
    })
    .expect("recorded shrink must still plan");

    assert_eq!(
        plan.rewritten,
        format!(
            "(defun {name} ({c} {b} {a}) (list {a} {b} {c}))\n(print ({name} {third} {second} {first}))\n"
        )
    );
    SyntaxTree::parse(&plan.rewritten).expect("rewritten output must reparse");
}
