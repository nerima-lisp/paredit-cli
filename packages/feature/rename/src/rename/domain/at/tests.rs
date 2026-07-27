use super::{RenameAtError, plan_rename_at};
use crate::domain::dialect::Dialect;
use crate::domain::rename::{RenameAtNamespace, RenameAtRequest};
use crate::domain::sexpr::{ByteOffset, SymbolName};

mod global;
mod local;
mod macro_namespaces;
mod nested_lambda_initializers;
mod safety;
mod validation;
mod value;

fn request<'a>(input: &'a str, needle: &str, to: &str) -> RenameAtRequest<'a> {
    RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(input.find(needle).expect("needle")),
        to: SymbolName::new(to).expect("symbol"),
    }
}
