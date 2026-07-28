mod core;
mod lists;
mod styles;

use crate::dialect::Dialect;

const MAX_INLINE_WIDTH: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Formatter {
    indent: usize,
    /// Selects the operator table that decides each list's line layout.
    ///
    /// Every dialect except [`Dialect::Clojure`] routes through the Common Lisp
    /// table, which is what [`Formatter::new`] preserves for callers that do
    /// not know the dialect.
    dialect: Dialect,
}
