use std::path::PathBuf;

use proptest::prelude::*;

use super::*;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::SyntaxTree;

mod basics;
mod definitions;
mod policy_property;

fn source(input: &str) -> SignatureReportSource {
    SignatureReportSource {
        path: PathBuf::from("input.lisp"),
        dialect: Dialect::CommonLisp,
        tree: SyntaxTree::parse(input).expect("test input should parse"),
    }
}

fn symbol_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,8}".prop_filter("exclude definition heads", |symbol| {
        !matches!(
            symbol.as_str(),
            "defun" | "fn" | "lambda" | "let" | "nil" | "t" | "true" | "false"
        )
    })
}
