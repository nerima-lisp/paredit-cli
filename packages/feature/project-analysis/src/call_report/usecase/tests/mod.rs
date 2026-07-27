use proptest::prelude::*;

use crate::application::usecase::call_report::build_call_report;
use crate::domain::definition::DefinitionCategory;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{SymbolName, SyntaxTree};

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
