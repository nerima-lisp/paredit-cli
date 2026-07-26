use proptest::{prelude::*, test_runner::TestCaseError};

use super::*;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionView, Path};

fn target(input: &str) -> ExpressionView {
    let tree = SyntaxTree::parse(input).expect("parse");
    tree.select_path(&"0".parse::<Path>().expect("path"))
        .expect("select")
        .view()
}

mod binding_forms;
mod property;
mod shadowing;
