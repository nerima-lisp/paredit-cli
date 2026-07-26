use std::path::PathBuf;

use clap::{Args, ValueEnum};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, Path};

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
/// parentheses, square brackets, or curly braces.
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
    #[arg(long, value_enum, default_value_t = WrapDelimiter::Paren)]
    pub delimiter: WrapDelimiter,
}

/// The list delimiter a `wrap` edit surrounds the selection with.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum WrapDelimiter {
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
