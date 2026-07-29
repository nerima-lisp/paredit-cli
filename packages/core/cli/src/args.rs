use std::path::PathBuf;

use clap::{Args, ValueEnum};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, Direction, Path, ReaderPrefix};

#[derive(Debug, Args)]
pub struct AnalyzeArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct FormatArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Number of spaces per nesting level.
    #[arg(long, default_value_t = 2)]
    pub indent: usize,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub diff: bool,
}

#[derive(Debug, Args)]
pub struct RepairArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Write the repaired document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the repaired document.
    #[arg(long)]
    pub diff: bool,
}

#[derive(Debug, Args)]
pub struct TargetArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Select by child index path, for example 0.2.1.
    #[arg(long, conflicts_with = "at")]
    pub path: Option<Path>,
    /// Select the smallest expression containing byte offset.
    #[arg(long, conflicts_with = "path")]
    pub at: Option<usize>,
}

/// Target selection plus in-place write support for mutating edit commands.
/// `select` keeps the plain [`TargetArgs`] because it never rewrites source.
#[derive(Debug, Args)]
pub struct EditTargetArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub diff: bool,
}

#[derive(Debug, Args)]
pub struct ReplaceArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Select by child index path, for example 0.2.1.
    #[arg(long, conflicts_with = "at")]
    pub path: Option<Path>,
    /// Select the smallest expression containing byte offset.
    #[arg(long, conflicts_with = "path")]
    pub at: Option<usize>,
    /// Replacement S-expression text.
    #[arg(long)]
    pub with: String,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub diff: bool,
}

/// Target selection plus in-place write support and a wrapping delimiter.
/// `wrap` extends [`EditTargetArgs`] with `--delimiter` so callers can wrap in
/// parentheses, square brackets, curly braces, or a string literal, and with
/// `--prefix` for the reader sugar that is not a delimiter pair at all.
#[derive(Debug, Args)]
pub struct WrapArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub diff: bool,
    /// Delimiter to wrap the selected expression in.
    #[arg(long, value_enum, default_value_t = WrapDelimiter::Paren, conflicts_with = "prefix")]
    pub delimiter: WrapDelimiter,
    /// Reader prefix to attach instead of wrapping in a delimiter pair.
    #[arg(long, value_enum, conflicts_with = "delimiter")]
    pub prefix: Option<ReaderPrefixArg>,
}

/// What a `wrap` edit surrounds the selection with.
///
/// `doublequote` is not a list delimiter: it produces a string literal, and the
/// selection's own quotes and backslashes are escaped on the way in, which is
/// what `paredit-meta-doublequote` does in Emacs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum WrapDelimiter {
    Paren,
    Bracket,
    Brace,
    Doublequote,
}

impl WrapDelimiter {
    /// The list delimiter this value names, or `None` for `doublequote`.
    #[must_use]
    pub const fn list_delimiter(self) -> Option<Delimiter> {
        match self {
            Self::Paren => Some(Delimiter::Paren),
            Self::Bracket => Some(Delimiter::Bracket),
            Self::Brace => Some(Delimiter::Brace),
            Self::Doublequote => None,
        }
    }
}

/// The reader sugar `wrap --prefix` attaches and `unwrap-prefix` removes.
///
/// Deliberately not every [`ReaderPrefix`] the parser knows: `#.` makes the
/// form opaque to every structural refactor that follows, and `#?`/`^` belong
/// to shapes a caller should be writing rather than growing one prefix at a
/// time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReaderPrefixArg {
    Quote,
    Quasiquote,
    Unquote,
    UnquoteSplicing,
    SharpQuote,
}

impl From<ReaderPrefixArg> for ReaderPrefix {
    fn from(value: ReaderPrefixArg) -> Self {
        match value {
            ReaderPrefixArg::Quote => Self::Quote,
            ReaderPrefixArg::Quasiquote => Self::Quasiquote,
            ReaderPrefixArg::Unquote => Self::Unquote,
            ReaderPrefixArg::UnquoteSplicing => Self::UnquoteSplicing,
            ReaderPrefixArg::SharpQuote => Self::Function,
        }
    }
}

/// Target selection plus in-place write support and a prefix-removal mode.
#[derive(Debug, Args)]
pub struct UnwrapPrefixArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub diff: bool,
    /// Remove every reader prefix rather than only the outermost one.
    #[arg(long)]
    pub all: bool,
}

/// Target selection plus in-place write support and a raise depth.
#[derive(Debug, Args)]
pub struct RaiseArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub diff: bool,
    /// How many enclosing lists to replace with the selection.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    pub levels: u32,
}

/// A cursor position rather than a form: the character edits address a byte
/// offset, because "the character after point" is not a subtree.
#[derive(Debug, Args)]
pub struct CursorArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Cursor position as a byte offset into the document.
    #[arg(long)]
    pub at: usize,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub diff: bool,
}

/// A cursor position plus the indentation width the reindent pass uses.
#[derive(Debug, Args)]
pub struct NewlineArgs {
    #[command(flatten)]
    pub cursor: CursorArgs,
    /// Number of spaces per nesting level when reindenting afterwards.
    #[arg(long, default_value_t = 2)]
    pub indent: usize,
    /// Insert the newline without reindenting the enclosing definition.
    #[arg(long)]
    pub no_reindent: bool,
}

/// Target selection plus the indentation width, for `reindent-defun`.
#[derive(Debug, Args)]
pub struct ReindentArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub diff: bool,
    /// Number of spaces per nesting level.
    #[arg(long, default_value_t = 2)]
    pub indent: usize,
}

/// Two selections rather than one: `transpose` swaps any two siblings, so it
/// needs a second address that `--path`/`--at` cannot carry.
#[derive(Debug, Args)]
pub struct TransposeArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    /// The other expression, by child index path.
    #[arg(long, conflicts_with = "with_at")]
    pub with_path: Option<Path>,
    /// The other expression, by byte offset.
    #[arg(long, conflicts_with = "with_path")]
    pub with_at: Option<usize>,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub diff: bool,
}

/// Which structural move `edit navigate` reports the destination of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DirectionArg {
    Forward,
    Backward,
    Up,
    Down,
}

impl From<DirectionArg> for Direction {
    fn from(value: DirectionArg) -> Self {
        match value {
            DirectionArg::Forward => Self::Forward,
            DirectionArg::Backward => Self::Backward,
            DirectionArg::Up => Self::Up,
            DirectionArg::Down => Self::Down,
        }
    }
}

/// Target selection plus a direction: `navigate` reads, it never rewrites.
#[derive(Debug, Args)]
pub struct NavigateArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    /// Which way to move from the selected expression.
    #[arg(long, value_enum)]
    pub direction: DirectionArg,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub output: OutputFormat,
}

/// Where `edit yank` puts the form it pastes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum YankPlacement {
    Before,
    After,
    Replace,
}

/// The kill ring's location and depth, shared by every command that touches it.
///
/// The path is explicit rather than implicit global state. An agent that runs
/// two refactors in two checkouts should not have them share a clipboard by
/// accident, and a ring nobody can point at cannot be inspected or reset.
#[derive(Debug, Args)]
pub struct KillRingArgs {
    /// Kill ring file. Defaults to $PAREDIT_KILL_RING, then .paredit/kill-ring.json.
    #[arg(long)]
    pub ring: Option<PathBuf>,
    /// How many entries the ring keeps before dropping the oldest.
    #[arg(long, default_value_t = 20)]
    pub ring_size: usize,
}

/// Target selection plus in-place write support and an optional ring push.
#[derive(Debug, Args)]
pub struct KillArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub diff: bool,
    /// Push the removed form onto the kill ring before removing it.
    #[arg(long)]
    pub to_ring: bool,
    #[command(flatten)]
    pub kill_ring: KillRingArgs,
}

/// Target selection for a read-only copy, with the same optional ring push.
#[derive(Debug, Args)]
pub struct CopyArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    /// Push the copied form onto the kill ring instead of only printing it.
    #[arg(long)]
    pub to_ring: bool,
    #[command(flatten)]
    pub kill_ring: KillRingArgs,
}

/// Target selection plus which ring entry to paste and where.
#[derive(Debug, Args)]
pub struct YankArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    /// Where to place the pasted form relative to the selection.
    #[arg(long, value_enum, default_value_t = YankPlacement::After)]
    pub placement: YankPlacement,
    /// Which ring entry to paste, newest first.
    #[arg(long, default_value_t = 0)]
    pub index: usize,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub diff: bool,
    #[command(flatten)]
    pub kill_ring: KillRingArgs,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DialectArg {
    CommonLisp,
    EmacsLisp,
    Lfe,
    Scheme,
    Racket,
    Clojure,
    Hy,
    Carp,
    Janet,
    Fennel,
    Unknown,
}

impl From<DialectArg> for Dialect {
    fn from(value: DialectArg) -> Self {
        match value {
            DialectArg::CommonLisp => Self::CommonLisp,
            DialectArg::EmacsLisp => Self::EmacsLisp,
            DialectArg::Lfe => Self::Lfe,
            DialectArg::Scheme => Self::Scheme,
            DialectArg::Racket => Self::Racket,
            DialectArg::Clojure => Self::Clojure,
            DialectArg::Hy => Self::Hy,
            DialectArg::Carp => Self::Carp,
            DialectArg::Janet => Self::Janet,
            DialectArg::Fennel => Self::Fennel,
            DialectArg::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MoveInsert {
    Append,
    Before,
    After,
}

impl MoveInsert {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Append => "append",
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ParameterInsert {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ThreadStyleArg {
    First,
    Last,
}

#[derive(Debug)]
pub struct SourceInput {
    pub text: String,
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum ImpactRiskLevel {
    Info,
    Warning,
    Error,
}
