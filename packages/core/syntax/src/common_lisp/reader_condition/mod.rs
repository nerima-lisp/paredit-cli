//! Common Lisp reader-conditional dispatch detection.
//!
//! Every reader keeps a complete conditional — the `#+`/`#-` dispatch, its
//! feature expression, and the datum it guards — together as one opaque atom.
//! The permissive `Dialect::Unknown` reader used to be the exception, tearing
//! the three apart into sibling atoms; it no longer is, so this module has one
//! representation to query rather than two.
//!
//! An incomplete conditional never reaches here at all: a `#+` with nothing to
//! guard is a parse error in every dialect, so a tree that exists always has
//! both components. See `an_incomplete_conditional_is_refused_by_both_readers`
//! in this module's tests for why refusing is the intended behaviour.

mod dispatch;
mod query;

#[cfg(test)]
pub use dispatch::CommonLispReaderConditionalDispatch;
pub use dispatch::{CommonLispReaderConditionalForm, CommonLispReaderConditionalKind};
#[cfg(test)]
pub use query::common_lisp_reader_conditional_dispatches;
pub use query::common_lisp_reader_conditional_forms;
pub use query::reader_conditional_kind;
