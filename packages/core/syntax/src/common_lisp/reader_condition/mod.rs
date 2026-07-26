//! Common Lisp reader-conditional dispatch detection.
//!
//! Legacy S-expression trees represent `#+` and `#-` as atom siblings of their
//! feature expression and guarded datum. Dialect-aware Common Lisp trees keep
//! the complete conditional as one opaque atom. This module owns the semantic
//! query across both representations.

mod dispatch;
mod query;

#[cfg(test)]
pub use dispatch::CommonLispReaderConditionalDispatch;
pub use dispatch::{CommonLispReaderConditionalForm, CommonLispReaderConditionalKind};
#[cfg(test)]
pub use query::common_lisp_reader_conditional_dispatches;
pub use query::common_lisp_reader_conditional_forms;
pub(crate) use query::reader_conditional_kind;
