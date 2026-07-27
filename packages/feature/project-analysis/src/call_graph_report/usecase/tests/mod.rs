use std::path::PathBuf;

use proptest::prelude::*;

use crate::domain::definition::DefinitionCategory;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{SymbolName, SyntaxTree};

use super::*;

mod basics;
mod definitions_policy;
mod shadowing;

fn parse(input: &str) -> SyntaxTree {
    SyntaxTree::parse(input).expect("valid lisp")
}

fn source(input: &str) -> CallGraphReportSource {
    CallGraphReportSource {
        path: PathBuf::from("sample.lisp"),
        dialect: Dialect::CommonLisp,
        tree: parse(input),
    }
}

fn source_with_dialect(input: &str, path: &str, dialect: Dialect) -> CallGraphReportSource {
    CallGraphReportSource {
        path: PathBuf::from(path),
        dialect,
        tree: parse(input),
    }
}

fn symbol_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,8}".prop_map(|symbol| symbol)
}
