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

pub use paredit_core_syntax::sexpr::Path;

pub use proptest::prelude::*;

pub fn symbol_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,8}".prop_filter("not reserved", |symbol| {
        !matches!(
            symbol.as_str(),
            "defun" | "fn" | "lambda" | "let" | "nil" | "t" | "true" | "false"
        )
    })
}
