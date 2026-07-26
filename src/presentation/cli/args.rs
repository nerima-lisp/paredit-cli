use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Delimiter, Path};

#[derive(Debug, Args)]
pub(super) struct AnalyzeArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub(super) file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub(super) output: OutputFormat,
}

#[derive(Debug, Args)]
pub(super) struct FormatArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub(super) file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Number of spaces per nesting level.
    #[arg(long, default_value_t = 2)]
    pub(super) indent: usize,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub(super) write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub(super) diff: bool,
}

#[derive(Debug, Args)]
pub(super) struct RepairArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub(super) file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Write the repaired document back to --file instead of stdout.
    #[arg(long)]
    pub(super) write: bool,
    /// Print a unified diff against the input instead of the repaired document.
    #[arg(long)]
    pub(super) diff: bool,
}

#[derive(Debug, Args)]
pub(crate) struct TargetArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub(super) file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Select by child index path, for example 0.2.1.
    #[arg(long, conflicts_with = "at")]
    pub(super) path: Option<Path>,
    /// Select the smallest expression containing byte offset.
    #[arg(long, conflicts_with = "path")]
    pub(super) at: Option<usize>,
}

/// Target selection plus in-place write support for mutating edit commands.
/// `select` keeps the plain [`TargetArgs`] because it never rewrites source.
#[derive(Debug, Args)]
pub(crate) struct EditTargetArgs {
    #[command(flatten)]
    pub(super) target: TargetArgs,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub(super) write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub(super) diff: bool,
}

#[derive(Debug, Args)]
pub(super) struct ReplaceArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub(super) file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub(super) dialect: Option<DialectArg>,
    /// Select by child index path, for example 0.2.1.
    #[arg(long, conflicts_with = "at")]
    pub(super) path: Option<Path>,
    /// Select the smallest expression containing byte offset.
    #[arg(long, conflicts_with = "path")]
    pub(super) at: Option<usize>,
    /// Replacement S-expression text.
    #[arg(long)]
    pub(super) with: String,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub(super) write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub(super) diff: bool,
}

/// Target selection plus in-place write support and a wrapping delimiter.
/// `wrap` extends [`EditTargetArgs`] with `--delimiter` so callers can wrap in
/// parentheses, square brackets, or curly braces.
#[derive(Debug, Args)]
pub(crate) struct WrapArgs {
    #[command(flatten)]
    pub(super) target: TargetArgs,
    /// Write the rewritten document back to --file instead of stdout.
    #[arg(long)]
    pub(super) write: bool,
    /// Print a unified diff against the input instead of the rewritten document.
    #[arg(long)]
    pub(super) diff: bool,
    /// Delimiter to wrap the selected expression in.
    #[arg(long, value_enum, default_value_t = WrapDelimiter::Paren)]
    pub(super) delimiter: WrapDelimiter,
}

/// The list delimiter a `wrap` edit surrounds the selection with.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum WrapDelimiter {
    Paren,
    Bracket,
    Brace,
}

impl From<WrapDelimiter> for Delimiter {
    fn from(value: WrapDelimiter) -> Self {
        match value {
            WrapDelimiter::Paren => Self::Paren,
            WrapDelimiter::Bracket => Self::Bracket,
            WrapDelimiter::Brace => Self::Brace,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum DialectArg {
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
pub(super) enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum MoveInsert {
    Append,
    Before,
    After,
}

impl MoveInsert {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Append => "append",
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum ParameterInsert {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum ThreadStyleArg {
    First,
    Last,
}

#[derive(Debug)]
pub(crate) struct SourceInput {
    pub(super) text: String,
    pub(super) file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub(super) enum ImpactRiskLevel {
    Info,
    Warning,
    Error,
}
