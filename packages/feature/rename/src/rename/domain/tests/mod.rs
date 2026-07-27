mod binding;
mod function;
mod local_function;
mod macrolet;
mod replace_call;
mod scoped_form;
mod symbol_macro;
mod unwrap;
mod wrap;

use super::*;

pub(super) use crate::domain::dialect::Dialect;
pub(super) use crate::domain::sexpr::{Path, SymbolName, SyntaxTree};

pub(super) use proptest::prelude::*;

pub(super) fn symbol_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,8}".prop_filter("not reserved", |symbol| {
        !matches!(
            symbol.as_str(),
            "defun" | "fn" | "lambda" | "let" | "nil" | "t" | "true" | "false"
        )
    })
}
