use proptest::prelude::*;

use crate::call_report::usecase::build_call_report;
use paredit_core_syntax::sexpr::SyntaxTree;

fn parse(input: &str) -> SyntaxTree {
    SyntaxTree::parse(input).expect("test input should parse")
}

fn symbol_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,8}".prop_filter("exclude definition heads", |symbol| {
        !matches!(
            symbol.as_str(),
            "defun" | "fn" | "lambda" | "let" | "nil" | "t" | "true" | "false"
        )
    })
}

mod basics;
mod local_callables;
mod property;
mod special_forms;
