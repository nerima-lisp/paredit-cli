//! The vocabulary of the value context.

mod literal;
mod table;
mod value;

pub use literal::{FloatLiteral, KeywordName, LiteralValue, PropagatableValue, TextLiteral};
pub use table::{ValueTable, ValueTableBuilder};
pub use value::Value;
