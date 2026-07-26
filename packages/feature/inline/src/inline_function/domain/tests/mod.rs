use crate::domain::sexpr::{Path, SyntaxTree};

use super::*;

mod common_lisp;
mod dialect;
mod property;

fn path(value: &str) -> Path {
    value.parse().expect("path")
}

fn parameter<'a>(plan: &'a InlineFunctionCallPlan, name: &str) -> &'a InlineFunctionParameterPlan {
    plan.parameters
        .iter()
        .find(|parameter| parameter.name == name)
        .expect("parameter")
}

fn default_inline_request(input: &str) -> InlineFunctionRequest<'_> {
    InlineFunctionRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        call_paths: vec![path("1.1")],
        all_calls: false,
        remove_definition: false,
        allow_duplicate_evaluation: false,
        allow_drop_arguments: false,
    }
}

fn inline_plan(input: &str) -> InlineFunctionPlan {
    plan_inline_function(default_inline_request(input)).expect("plan")
}

fn inline_error(input: &str, context: &str) -> anyhow::Error {
    plan_inline_function(default_inline_request(input)).expect_err(context)
}

fn all_calls_request(input: &str, dialect: Dialect) -> InlineFunctionRequest<'_> {
    InlineFunctionRequest {
        input,
        dialect,
        definition_path: path("0"),
        call_paths: Vec::new(),
        all_calls: true,
        remove_definition: false,
        allow_duplicate_evaluation: false,
        allow_drop_arguments: false,
    }
}

fn all_calls_plan(input: &str, dialect: Dialect) -> InlineFunctionPlan {
    plan_inline_function(all_calls_request(input, dialect)).expect("plan")
}

fn remove_definition_plan(input: &str, dialect: Dialect) -> InlineFunctionPlan {
    plan_inline_function(InlineFunctionRequest {
        remove_definition: true,
        ..all_calls_request(input, dialect)
    })
    .expect("plan")
}

fn duplicate_evaluation_plan(input: &str) -> InlineFunctionPlan {
    plan_inline_function(InlineFunctionRequest {
        allow_duplicate_evaluation: true,
        ..default_inline_request(input)
    })
    .expect("plan")
}
