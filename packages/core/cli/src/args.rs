use std::path::PathBuf;

use clap::{ArgAction, Args, ValueEnum};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::{
    LinePosition, Pattern, RelativeStep, SelectorError, SelectorRequest, SelectorTerm,
    StableSelectorId,
};
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
    ///
    /// Not `--all`: the selector layer already owns that name, where it means
    /// "act on every match" rather than "peel every prefix".
    #[arg(long = "all-prefixes")]
    pub all_prefixes: bool,
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
    #[arg(long, group = "transpose-partner")]
    pub with_path: Option<Path>,
    /// The other expression, by byte offset.
    #[arg(long, group = "transpose-partner")]
    pub with_at: Option<usize>,
    /// The other expression, as a compact selector: name:foo, line:12:5, or a path.
    ///
    /// The `--with-` prefix is not decoration: the primary target already
    /// claims `--path`, `--at` and `--select` through [`SelectorArgs`], and two
    /// addresses in one command cannot share flag names.
    #[arg(long, value_name = "SELECTOR", group = "transpose-partner")]
    pub with_select: Option<String>,
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

/// The formats a *finding report* can be printed in.
///
/// A superset of [`OutputFormat`], and deliberately a separate type rather than
/// extra variants on it. `--output` is on nearly every command in this tool,
/// but only the commands that go through the shared report envelope produce a
/// list of located findings — the shape SARIF, JUnit, and Code Climate all
/// describe. Widening [`OutputFormat`] would advertise `--output sarif` on
/// `edit format`, which has nothing to put in a SARIF result, and the failure
/// would arrive at runtime instead of in the catalog.
///
/// Keeping the two apart means `inspect capabilities` reports the true format
/// set per command, so an agent can read what a command accepts rather than
/// discover it by being rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReportFormat {
    /// Tab-separated rows, one per finding. The default for terminal reading.
    Text,
    /// The tool's own JSON envelope.
    Json,
    /// SARIF 2.1.0, for GitHub code scanning and other SARIF ingesters.
    Sarif,
    /// JUnit XML, for the test-report panel of a CI system.
    Junit,
    /// Code Climate issue JSON, which GitLab Code Quality consumes.
    CodeClimate,
    /// RFC 4180 comma-separated values, for a spreadsheet.
    Csv,
    /// Tab-separated values with a header row, for `cut`/`awk`.
    Tsv,
    /// A standalone HTML page, shareable without the tool.
    Html,
    /// A Markdown table, for a pull request comment or an issue.
    Markdown,
    /// GitHub Actions workflow commands, rendered inline on the pull request.
    Github,
}

/// The drawing languages `--graph` can emit.
///
/// A separate option from `--output` rather than more `--output` values,
/// because it selects a different *view*, not a different serialization of the
/// same one. `--output json` and `--output csv` carry every field a report
/// computed; a graph carries the node and edge structure and drops the spans,
/// counts, and policy verdict that do not belong in a picture. Presenting that
/// as one more encoding of the report would misdescribe what a caller gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GraphFormat {
    /// Graphviz DOT, for `dot -Tsvg`.
    Dot,
    /// Mermaid `flowchart`, which GitHub and GitLab render inline in Markdown.
    Mermaid,
}

impl GraphFormat {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dot => "dot",
            Self::Mermaid => "mermaid",
        }
    }
}

impl From<OutputFormat> for ReportFormat {
    fn from(value: OutputFormat) -> Self {
        match value {
            OutputFormat::Text => Self::Text,
            OutputFormat::Json => Self::Json,
        }
    }
}

impl ReportFormat {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Sarif => "sarif",
            Self::Junit => "junit",
            Self::CodeClimate => "code-climate",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Html => "html",
            Self::Markdown => "markdown",
            Self::Github => "github",
        }
    }

    /// Whether this format is one of the two the tool defines itself.
    ///
    /// The rest are other people's schemas, which is what makes them worth
    /// keeping apart: a change to `text` or `json` is this tool's decision, and
    /// a change to SARIF is not.
    #[must_use]
    pub const fn is_native(self) -> bool {
        matches!(self, Self::Text | Self::Json)
    }
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
