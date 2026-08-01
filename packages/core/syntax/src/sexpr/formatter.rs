mod core;
mod lists;
mod styles;

use crate::dialect::Dialect;

/// The compiled-in default for one inline (non-wrapped) line's width.
///
/// Still the default and no longer the only possible value: a caller may
/// lower or raise it with [`Formatter::with_max_width`].
pub const MAX_INLINE_WIDTH: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Formatter {
    indent: usize,
    /// Selects the operator table that decides each list's line layout.
    ///
    /// Every dialect except [`Dialect::Clojure`] routes through the Common Lisp
    /// table, which is what [`Formatter::new`] preserves for callers that do
    /// not know the dialect.
    dialect: Dialect,
    /// The width one inline (non-wrapped) line may reach before this falls
    /// back to a multi-line layout. [`MAX_INLINE_WIDTH`] unless overridden
    /// with [`Formatter::with_max_width`].
    max_width: usize,
    /// Realign `#|...|#` block comment lines to their nesting depth. `false`
    /// unless overridden with [`Formatter::with_reindent_block_comments`]: a
    /// formatting *policy* rather than a bug fix, so it stays opt-in.
    reindent_block_comments: bool,
    /// Column same-line trailing comments align to. `None` unless overridden
    /// with [`Formatter::with_comment_column`], which reproduces the
    /// original single-space, no-alignment behavior exactly.
    comment_column: Option<usize>,
    /// Most consecutive blank lines preserved from the source between
    /// top-level forms. `None` unless overridden with
    /// [`Formatter::with_max_blank_lines`], which reproduces the original
    /// behavior of always collapsing every gap to exactly one blank line.
    max_blank_lines: Option<usize>,
}
