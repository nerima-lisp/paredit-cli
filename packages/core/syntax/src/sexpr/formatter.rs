mod core;
mod lists;
mod styles;

use std::fmt;

use crate::dialect::Dialect;

pub use styles::STYLE_NAMES;

/// The compiled-in default for one inline (non-wrapped) line's width.
///
/// Still the default and no longer the only possible value: a caller may
/// lower or raise it with [`Formatter::with_max_width`].
pub const MAX_INLINE_WIDTH: usize = 80;

/// How reader-macro prefixes (`'x`, `` `x ``, `,x`, `,@x`, `#'x`) print.
///
/// `Canonical` only expands a prefix into its list form for the prefixes
/// that have exactly one portable list spelling in the document's dialect —
/// `'x` becomes `(quote x)` everywhere, and `#'f` becomes `(function f)` in
/// Common Lisp and Emacs Lisp, where that pair is itself standardized.
/// Quasiquote and unquote are never expanded, in either style, because they
/// have no such spelling: see `quote_edit`'s module doc for why. A node whose
/// prefix stack mixes a prefix this can canonicalize with one it cannot keeps
/// the entire stack in shorthand rather than partly rewriting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReaderPrefixStyle {
    /// `'x`, `#'f`. The only behavior before this option existed.
    #[default]
    Shorthand,
    /// `(quote x)`, `(function f)`, for every prefix that has one.
    Canonical,
}

/// A `format.indent-table` or `format.width-profiles` entry named a style
/// outside [`STYLE_NAMES`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownStyleName {
    pub style_name: String,
}

impl fmt::Display for UnknownStyleName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "\"{}\" is not a recognised indent style; expected one of {}",
            self.style_name,
            STYLE_NAMES.join(", ")
        )
    }
}

impl std::error::Error for UnknownStyleName {}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Project-configured overrides of which [`STYLE_NAMES`] entry a head
    /// symbol resolves to, checked before the built-in per-dialect table and
    /// winning when both name the same symbol. Empty unless set with
    /// [`Formatter::with_indent_overrides`].
    indent_overrides: Vec<(String, styles::ListStyle)>,
    /// Per-[`STYLE_NAMES`]-entry `--max-width` overrides, consulted in place
    /// of `max_width` when deciding whether a node whose head resolves to a
    /// named style fits on one line. Empty unless set with
    /// [`Formatter::with_width_profiles`].
    ///
    /// Observable in practice only for `"general"` (or a style
    /// [`Formatter::with_indent_overrides`] retargeted onto it): every other
    /// style already never compacts onto one line at all, so a profile for
    /// it is never read. See `Formatter::effective_max_width`'s doc comment
    /// in `formatter::core`.
    width_profiles: Vec<(styles::ListStyle, usize)>,
    /// How reader-macro prefixes print. `Shorthand` unless overridden with
    /// [`Formatter::with_reader_prefix_style`].
    quote_style: ReaderPrefixStyle,
}

impl Formatter {
    /// Overrides which [`STYLE_NAMES`] entry a head symbol resolves to,
    /// replacing the built-in mapping for every symbol named on the left of
    /// an `(symbol, style)` pair. Later entries for the same symbol win over
    /// earlier ones.
    ///
    /// Returns the first style name that is not in [`STYLE_NAMES`], leaving
    /// `self` unchanged, rather than silently ignoring it or panicking.
    pub fn with_indent_overrides(
        mut self,
        overrides: &[(String, String)],
    ) -> Result<Self, UnknownStyleName> {
        let mut table = Vec::with_capacity(overrides.len());
        for (symbol, style_name) in overrides {
            let style = styles::style_from_name(style_name).ok_or_else(|| UnknownStyleName {
                style_name: style_name.clone(),
            })?;
            table.push((symbol.clone(), style));
        }
        self.indent_overrides = table;
        Ok(self)
    }

    /// Overrides the `--max-width` budget used when deciding whether a node
    /// whose head resolves to a named [`STYLE_NAMES`] entry fits on one line,
    /// in place of the formatter's own `max_width`. Later entries for the
    /// same style win over earlier ones.
    ///
    /// Only ever observable for the `"general"` style in practice — see this
    /// struct's `width_profiles` field doc for why — though every entry is
    /// still validated and stored uniformly.
    ///
    /// Returns the first style name that is not in [`STYLE_NAMES`], leaving
    /// `self` unchanged, rather than silently ignoring it or panicking.
    pub fn with_width_profiles(
        mut self,
        profiles: &[(String, usize)],
    ) -> Result<Self, UnknownStyleName> {
        let mut table = Vec::with_capacity(profiles.len());
        for (style_name, width) in profiles {
            let style = styles::style_from_name(style_name).ok_or_else(|| UnknownStyleName {
                style_name: style_name.clone(),
            })?;
            table.push((style, *width));
        }
        self.width_profiles = table;
        Ok(self)
    }

    /// Overrides how reader-macro prefixes print. [`ReaderPrefixStyle::Shorthand`]
    /// by default, which is byte-identical to this formatter's behavior
    /// before this option existed.
    #[must_use]
    pub const fn with_reader_prefix_style(mut self, style: ReaderPrefixStyle) -> Self {
        self.quote_style = style;
        self
    }
}
