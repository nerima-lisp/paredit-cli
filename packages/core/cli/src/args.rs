use std::path::PathBuf;

use clap::{ArgAction, Args, ValueEnum};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::{
    LinePosition, Pattern, RelativeStep, SelectorError, SelectorRequest, SelectorTerm,
    StableSelectorId,
};
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

/// Every way of naming the form a command acts on.
///
/// Flattened into [`TargetArgs`], [`ReplaceArgs`] and the report commands, so
/// one struct is the whole selector surface and adding a selector reaches
/// every command that takes one. `--path` and `--at` stay first because they
/// are the exact, cheap addresses; the rest exist to save the round trip
/// through `inspect outline` that building one of those costs.
///
/// Exactly one base selector may be given. `--parent` / `--child` /
/// `--sibling` modify whichever base was chosen and are applied in that order.
#[derive(Debug, Clone, Args)]
pub struct SelectorArgs {
    /// Select by child index path, for example 0.2.1.
    #[arg(long, group = "selector-base")]
    pub path: Option<Path>,
    /// Select the smallest expression containing byte offset.
    #[arg(long, group = "selector-base")]
    pub at: Option<usize>,
    /// Select the smallest expression at LINE[:COLUMN], both 1-based.
    #[arg(long, value_name = "LINE[:COLUMN]", group = "selector-base")]
    pub line_column: Option<LinePosition>,
    /// Select the definition named SYMBOL.
    #[arg(long = "name", value_name = "SYMBOL", group = "selector-base")]
    pub selector_name: Option<String>,
    /// Select with a compact selector: name:foo, line:12:5, query:(...), sel:ID, or a path.
    #[arg(long, value_name = "SELECTOR", group = "selector-base")]
    pub select: Option<String>,
    /// Select forms matching an S-expression pattern, such as '(defun ?name ...)'.
    #[arg(long, value_name = "PATTERN", group = "selector-base")]
    pub query: Option<String>,
    /// With --query, select the named capture instead of the whole match.
    #[arg(long, value_name = "NAME", requires = "query")]
    pub capture: Option<String>,
    /// Select the form carrying a stable selector id from `inspect resolve`.
    #[arg(long, value_name = "ID", group = "selector-base")]
    pub id: Option<String>,
    /// Start of a multi-form range, as a compact selector such as 0.2 or name:foo.
    #[arg(
        long,
        value_name = "SELECTOR",
        group = "selector-base",
        requires = "to"
    )]
    pub from: Option<String>,
    /// End of a multi-form range, as a compact selector.
    #[arg(long, value_name = "SELECTOR", requires = "from")]
    pub to: Option<String>,
    /// Move to the enclosing form. Repeat to climb further.
    #[arg(long, action = ArgAction::Count)]
    pub parent: u8,
    /// Move to child N of the selected form.
    #[arg(long, value_name = "N")]
    pub child: Option<usize>,
    /// Move N siblings forward, or backward when negative.
    #[arg(long, value_name = "N", allow_hyphen_values = true)]
    pub sibling: Option<isize>,
    /// Act on every match instead of refusing a selector that matches more than one form.
    #[arg(long)]
    pub all: bool,
}

impl SelectorArgs {
    /// The base selectors clap accepts, named in the order they are listed.
    pub const EXPECTED: &'static str =
        "--path, --at, --line-column, --name, --query, --id, --select, or --from/--to";

    /// Builds the resolvable request, parsing the pattern with `dialect`.
    ///
    /// Parsing is deferred to here rather than done in a clap value parser
    /// because a pattern is read with the *file's* reader, and the dialect is
    /// only known once the input has been read.
    pub fn to_request(&self, dialect: Dialect) -> Result<SelectorRequest, SelectorError> {
        let term = self.base_term(dialect)?;
        let range_end = self
            .to
            .as_deref()
            .map(|value| SelectorTerm::parse_compact(value, dialect))
            .transpose()?;

        let mut steps = Vec::new();
        steps.extend(std::iter::repeat_n(
            RelativeStep::Parent,
            usize::from(self.parent),
        ));
        if let Some(offset) = self.sibling {
            steps.push(RelativeStep::Sibling(offset));
        }
        if let Some(index) = self.child {
            steps.push(RelativeStep::Child(index));
        }

        Ok(SelectorRequest {
            term,
            range_end,
            steps,
            all: self.all,
        })
    }

    fn base_term(&self, dialect: Dialect) -> Result<SelectorTerm, SelectorError> {
        if let Some(path) = &self.path {
            return Ok(SelectorTerm::Path(path.clone()));
        }
        if let Some(offset) = self.at {
            return Ok(SelectorTerm::Offset(offset));
        }
        if let Some(position) = self.line_column {
            return Ok(SelectorTerm::Position(position));
        }
        if let Some(name) = &self.selector_name {
            return Ok(SelectorTerm::Name(name.clone()));
        }
        if let Some(value) = &self.select {
            return SelectorTerm::parse_compact(value, dialect);
        }
        if let Some(query) = &self.query {
            return Ok(SelectorTerm::Query {
                pattern: Pattern::parse(query, dialect)?,
                source: query.clone(),
                capture: self.capture.clone(),
            });
        }
        if let Some(id) = &self.id {
            // Not `parse_compact`: a bare id is hex, which that function reads
            // as a path only when it is all digits and dots.
            return StableSelectorId::parse(id).map(SelectorTerm::Id);
        }
        if let Some(from) = &self.from {
            return SelectorTerm::parse_compact(from, dialect);
        }
        Err(SelectorError::Missing {
            expected: Self::EXPECTED.to_owned(),
        })
    }
}

/// The selector surface for commands whose own flags already use these names.
///
/// `refactor introduce-let --name` is the *new binding's* name, `refactor
/// rename-binding --from`/`--to` are symbols, and several more collide the
/// same way. Rather than give those commands a second, differently-spelled
/// `--name`, they take one `--select` carrying a compact selector — the same
/// grammar `--from`/`--to` use — so every selector kind is still reachable
/// and no meaning is overloaded.
///
/// Relative moves and `--all` are deliberately absent: these commands act on
/// exactly one form, and a fan-out over them is a different feature with its
/// own safety questions.
#[derive(Debug, Clone, Args)]
pub struct CompactSelectorArgs {
    /// Select by child index path, for example 0.2.1.
    #[arg(long, group = "compact-selector-base")]
    pub path: Option<Path>,
    /// Select the smallest expression containing byte offset.
    #[arg(long, group = "compact-selector-base")]
    pub at: Option<usize>,
    /// Select with a compact selector: name:foo, line:12:5, query:(...), sel:ID, or a path.
    #[arg(long, value_name = "SELECTOR", group = "compact-selector-base")]
    pub select: Option<String>,
}

impl CompactSelectorArgs {
    /// The base selectors this variant accepts.
    pub const EXPECTED: &'static str = "--path, --at, or --select";

    /// Builds a single-match request.
    pub fn to_request(&self, dialect: Dialect) -> Result<SelectorRequest, SelectorError> {
        let term = if let Some(path) = &self.path {
            SelectorTerm::Path(path.clone())
        } else if let Some(offset) = self.at {
            SelectorTerm::Offset(offset)
        } else if let Some(value) = &self.select {
            SelectorTerm::parse_compact(value, dialect)?
        } else {
            return Err(SelectorError::Missing {
                expected: Self::EXPECTED.to_owned(),
            });
        };
        Ok(SelectorRequest::new(term))
    }
}

#[derive(Debug, Args)]
pub struct TargetArgs {
    /// Input file. Reads stdin when omitted.
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    #[command(flatten)]
    pub selector: SelectorArgs,
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
    #[command(flatten)]
    pub selector: SelectorArgs,
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
