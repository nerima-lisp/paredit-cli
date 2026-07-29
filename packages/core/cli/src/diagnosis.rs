//! What went wrong, named stably, and what to do about it.
//!
//! An exit code says a command failed. It does not say *what kind* of failure,
//! and an agent deciding whether to retry, re-read, or give up needs that far
//! more than it needs prose. So every failure this tool can produce is
//! classified into a [`ErrorCode`] — stable, namespaced, and safe to match on
//! — and carries whatever [`Repair`] steps actually apply.
//!
//! Two deliberate limits:
//!
//! - **The classification is coarser than the error types.** There are roughly
//!   sixty error variants and thirty-odd codes, because the question a code
//!   answers is "what should the caller do differently", and a dozen shape
//!   refusals all answer it the same way: the selected form is not the shape
//!   this operation works on.
//! - **A repair is only offered when it is real.** A suggestion that cannot be
//!   run, or that would not have helped, is worse than none: it costs a round
//!   trip to find out. Where the file is known the `command` is a command line
//!   that can be run as written; where it is not, there is only prose.

use std::fmt;

use paredit_core_edit::EditRefusal;
use paredit_core_syntax::selector::error::SelectorError;
use paredit_core_syntax::sexpr::{
    PathError, SelectionError, SexprError, SpanError, StructureError, SymbolError,
};
use paredit_core_workspace::workspace::WorkspaceError;
use serde_json::{Value as Json, json};

use crate::error::{
    ArgumentError, CleanupFailure, CliError, CommandFailure, IoRefusal, WriteTargetError,
};
use crate::gate::GATE_FAILURE_EXIT_CODE;
use crate::messages::{Message, say};

/// The class of a failure, from the caller's point of view.
///
/// Coarser than the code so that a consumer can branch on "is this worth
/// retrying" without enumerating every code this build happens to have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// The command line does not describe a runnable request. Retrying the
    /// same invocation cannot help.
    Argument,
    /// The selection did not resolve. A different `--path` or `--at` might.
    Selection,
    /// The input is not what the operation needs. The source has to change.
    Input,
    /// The tool declined for safety. The state has to change first.
    Refusal,
    /// The filesystem or the environment failed. Retrying might work.
    Environment,
    /// A requested policy gate tripped. The report was printed; this is the
    /// gate doing its job, not a malfunction.
    Gate,
    /// Not classified. A bug in this table rather than in the caller.
    Internal,
}

impl Category {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Argument => "argument",
            Self::Selection => "selection",
            Self::Input => "input",
            Self::Refusal => "refusal",
            Self::Environment => "environment",
            Self::Gate => "gate",
            Self::Internal => "internal",
        }
    }

    /// One line describing this category, in the configured language.
    ///
    /// The *label* stays English in every language: it is an identifier a
    /// consumer matches on. Only the description is translated.
    #[must_use]
    pub fn describe(self) -> &'static str {
        use crate::messages::{Message, say};
        say(match self {
            Self::Argument => Message::CategoryArgument,
            Self::Selection => Message::CategorySelection,
            Self::Input => Message::CategoryInput,
            Self::Refusal => Message::CategoryRefusal,
            Self::Environment => Message::CategoryEnvironment,
            Self::Gate => Message::CategoryGate,
            Self::Internal => Message::CategoryInternal,
        })
    }

    /// Whether re-running the identical command could plausibly succeed.
    ///
    /// Stated once, here, so an agent does not have to infer it from prose. A
    /// selection failure is *not* retryable: the same path will not resolve
    /// the second time either.
    #[must_use]
    pub const fn retryable(self) -> bool {
        matches!(self, Self::Environment)
    }
}

/// A stable identifier for a class of failure.
///
/// The string is the contract. Adding a code is a compatible change; renaming
/// one is not, which is why the names describe the failure rather than the
/// call site that raised it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ArgumentNoInput,
    ArgumentTargetRequired,
    ArgumentTargetAmbiguous,
    ArgumentWriteRequiresFile,
    ArgumentConflictingInputs,
    ArgumentInvalidGlob,
    ArgumentNeedsRepository,
    ArgumentNoInputsProduced,
    ArgumentArchiveDestination,
    ArgumentKillRingIndex,
    /// Two flags that do not combine, or one that needs another.
    ArgumentFlagCombination,
    /// A name this build does not have — a rule, a preset, a migration.
    ArgumentUnknownName,

    SelectionPathNotReachable,
    SelectionPathInvalid,
    SelectionOffsetNotFound,
    SelectionStale,
    SelectionSpanInvalid,
    SelectionNoMatch,
    SelectionAmbiguous,
    SelectionPatternInvalid,
    SelectionMalformed,

    InputUnparsable,
    InputNotUtf8,
    InputShapeRefused,
    InputSymbolInvalid,
    /// The command is not defined for this file's dialect.
    ///
    /// Its own code rather than [`Self::InputShapeRefused`] because the two
    /// call for opposite actions: a shape refusal means "select a different
    /// form in this file", and this one means "this file is the wrong
    /// language for this command, stop retrying it here".
    InputDialectUnsupported,

    RefusalInputTooLarge,
    RefusalWriteTarget,
    RefusalTargetChanged,
    RefusalRewriteDoesNotReparse,
    RefusalOverlappingSpans,
    RefusalSpanOutOfBounds,
    RefusalWorkspace,
    RefusalDryRun,
    RefusalMatchShifted,
    RefusalEncodingWrite,

    EnvironmentIo,
    EnvironmentWorkspaceLimit,
    EnvironmentUnavailable,
    EnvironmentRollbackFailed,
    EnvironmentCleanupAfterCommit,
    /// The wall-clock budget ran out. Nothing is wrong with the input: the
    /// same command with a larger `--timeout-ms`, or none, would succeed.
    EnvironmentTimeout,

    GateFailed,
    Internal,
}

impl ErrorCode {
    /// The identifier a consumer matches on.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ArgumentNoInput => "argument.no-input",
            Self::ArgumentTargetRequired => "argument.target-required",
            Self::ArgumentTargetAmbiguous => "argument.target-ambiguous",
            Self::ArgumentWriteRequiresFile => "argument.write-requires-file",
            Self::ArgumentConflictingInputs => "argument.conflicting-inputs",
            Self::ArgumentInvalidGlob => "argument.invalid-glob",
            Self::ArgumentNeedsRepository => "argument.needs-repository",
            Self::ArgumentNoInputsProduced => "argument.no-inputs-produced",
            Self::ArgumentArchiveDestination => "argument.archive-destination",
            Self::ArgumentKillRingIndex => "argument.kill-ring-index",
            Self::ArgumentFlagCombination => "argument.flag-combination",
            Self::ArgumentUnknownName => "argument.unknown-name",

            Self::SelectionPathNotReachable => "selection.path-not-reachable",
            Self::SelectionPathInvalid => "selection.path-invalid",
            Self::SelectionOffsetNotFound => "selection.offset-not-found",
            Self::SelectionStale => "selection.stale",
            Self::SelectionSpanInvalid => "selection.span-invalid",
            Self::SelectionNoMatch => "selection.no-match",
            Self::SelectionAmbiguous => "selection.ambiguous",
            Self::SelectionPatternInvalid => "selection.pattern-invalid",
            Self::SelectionMalformed => "selection.malformed",

            Self::InputUnparsable => "input.unparsable",
            Self::InputNotUtf8 => "input.not-utf8",
            Self::InputShapeRefused => "input.shape-refused",
            Self::InputSymbolInvalid => "input.symbol-invalid",
            Self::InputDialectUnsupported => "input.dialect-unsupported",

            Self::RefusalInputTooLarge => "refusal.input-too-large",
            Self::RefusalWriteTarget => "refusal.write-target",
            Self::RefusalTargetChanged => "refusal.target-changed",
            Self::RefusalRewriteDoesNotReparse => "refusal.rewrite-does-not-reparse",
            Self::RefusalOverlappingSpans => "refusal.overlapping-spans",
            Self::RefusalSpanOutOfBounds => "refusal.span-out-of-bounds",
            Self::RefusalWorkspace => "refusal.workspace",
            Self::RefusalDryRun => "refusal.dry-run",
            Self::RefusalMatchShifted => "refusal.match-shifted",
            Self::RefusalEncodingWrite => "refusal.encoding-write",

            Self::EnvironmentIo => "environment.io",
            Self::EnvironmentWorkspaceLimit => "environment.workspace-limit",
            Self::EnvironmentUnavailable => "environment.unavailable",
            Self::EnvironmentRollbackFailed => "environment.rollback-failed",
            Self::EnvironmentCleanupAfterCommit => "environment.cleanup-after-commit",
            Self::EnvironmentTimeout => "environment.timeout",

            Self::GateFailed => "gate.failed",
            Self::Internal => "internal.unclassified",
        }
    }

    #[must_use]
    pub const fn category(self) -> Category {
        match self {
            Self::ArgumentNoInput
            | Self::ArgumentTargetRequired
            | Self::ArgumentTargetAmbiguous
            | Self::ArgumentWriteRequiresFile
            | Self::ArgumentConflictingInputs
            | Self::ArgumentInvalidGlob
            | Self::ArgumentNeedsRepository
            | Self::ArgumentNoInputsProduced
            | Self::ArgumentArchiveDestination
            | Self::ArgumentKillRingIndex
            | Self::ArgumentFlagCombination
            | Self::ArgumentUnknownName => Category::Argument,

            Self::SelectionPathNotReachable
            | Self::SelectionPathInvalid
            | Self::SelectionOffsetNotFound
            | Self::SelectionStale
            | Self::SelectionSpanInvalid
            | Self::SelectionNoMatch
            | Self::SelectionAmbiguous
            | Self::SelectionPatternInvalid
            | Self::SelectionMalformed => Category::Selection,

            Self::InputUnparsable
            | Self::InputNotUtf8
            | Self::InputShapeRefused
            | Self::InputSymbolInvalid
            | Self::InputDialectUnsupported => Category::Input,

            Self::RefusalInputTooLarge
            | Self::RefusalWriteTarget
            | Self::RefusalTargetChanged
            | Self::RefusalRewriteDoesNotReparse
            | Self::RefusalOverlappingSpans
            | Self::RefusalSpanOutOfBounds
            | Self::RefusalWorkspace
            | Self::RefusalDryRun
            | Self::RefusalMatchShifted
            | Self::RefusalEncodingWrite => Category::Refusal,

            Self::EnvironmentIo
            | Self::EnvironmentWorkspaceLimit
            | Self::EnvironmentUnavailable
            | Self::EnvironmentRollbackFailed
            | Self::EnvironmentCleanupAfterCommit
            | Self::EnvironmentTimeout => Category::Environment,

            Self::GateFailed => Category::Gate,
            Self::Internal => Category::Internal,
        }
    }

    /// The process exit status this failure produces.
    ///
    /// Only a gate has its own; everything else is a plain failure. Reported
    /// alongside the code so a consumer can tell the two apart *before*
    /// deciding what an exit status meant.
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::GateFailed => GATE_FAILURE_EXIT_CODE as u8,
            _ => 1,
        }
    }

    /// The documentation page explaining this code, with the label as the
    /// page's own anchor.
    ///
    /// Built from the label rather than a second table, so a code and its
    /// anchor cannot drift from each other — `tests/cli/errors_doc.rs` checks
    /// that the anchor this produces actually exists in `docs/src/errors.md`.
    #[must_use]
    pub fn doc_url(self) -> String {
        format!(
            "https://nerima-lisp.github.io/paredit-cli/errors/#{}",
            self.label()
        )
    }

    /// Every code, so a contract test can check the table is total and the
    /// labels unique.
    pub const ALL: [Self; 44] = [
        Self::ArgumentNoInput,
        Self::ArgumentTargetRequired,
        Self::ArgumentTargetAmbiguous,
        Self::ArgumentWriteRequiresFile,
        Self::ArgumentConflictingInputs,
        Self::ArgumentInvalidGlob,
        Self::ArgumentNeedsRepository,
        Self::ArgumentNoInputsProduced,
        Self::ArgumentArchiveDestination,
        Self::ArgumentKillRingIndex,
        Self::ArgumentFlagCombination,
        Self::ArgumentUnknownName,
        Self::SelectionPathNotReachable,
        Self::SelectionPathInvalid,
        Self::SelectionOffsetNotFound,
        Self::SelectionStale,
        Self::SelectionSpanInvalid,
        Self::SelectionNoMatch,
        Self::SelectionAmbiguous,
        Self::SelectionPatternInvalid,
        Self::SelectionMalformed,
        Self::InputUnparsable,
        Self::InputNotUtf8,
        Self::InputShapeRefused,
        Self::InputSymbolInvalid,
        Self::InputDialectUnsupported,
        Self::RefusalInputTooLarge,
        Self::RefusalWriteTarget,
        Self::RefusalTargetChanged,
        Self::RefusalRewriteDoesNotReparse,
        Self::RefusalOverlappingSpans,
        Self::RefusalSpanOutOfBounds,
        Self::RefusalWorkspace,
        Self::RefusalDryRun,
        Self::RefusalMatchShifted,
        Self::RefusalEncodingWrite,
        Self::EnvironmentIo,
        Self::EnvironmentWorkspaceLimit,
        Self::EnvironmentUnavailable,
        Self::EnvironmentRollbackFailed,
        Self::EnvironmentCleanupAfterCommit,
        Self::EnvironmentTimeout,
        Self::GateFailed,
        Self::Internal,
    ];
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// One thing that would plausibly get past this failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repair {
    /// What kind of step this is, for a consumer that would rather match than
    /// read: `inspect-first`, `change-selection`, `pass-flag`, `re-read`,
    /// `fix-source`, `check-configuration`.
    pub action: &'static str,
    /// What to do, as a catalogue entry rather than a string.
    ///
    /// A `Message` rather than text so that adding a repair without adding its
    /// translation fails to compile. A silent English fallback in a Japanese
    /// run reads as "this one is untranslatable" rather than "we forgot".
    pub detail: Message,
    /// A command line that can be run exactly as written, when the failure
    /// carried enough context to build one.
    pub command: Option<String>,
}

impl Repair {
    const fn new(action: &'static str, detail: Message) -> Self {
        Self {
            action,
            detail,
            command: None,
        }
    }

    /// The instruction, in the language this run is configured for.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        say(self.detail)
    }

    fn with_command(mut self, command: Option<String>) -> Self {
        self.command = command;
        self
    }

    /// Rendered for a terminal, so the strings are escaped here.
    ///
    /// `serde_json` escapes the C0 controls but not the bidirectional
    /// overrides, and these strings can carry a path someone else chose. The
    /// escaping has to happen on the values, not on the serialized document:
    /// escaping the document would turn its own newlines into `\u{a}` and stop
    /// it being JSON.
    #[must_use]
    pub fn to_json(&self) -> Json {
        json!({
            "action": self.action,
            "detail": self.detail(),
            "command": self
                .command
                .as_ref()
                .map(|command| crate::shared::terminal_safe(command).to_string()),
        })
    }
}

/// What the invocation was about, so a repair can name real paths.
///
/// A suggestion reading `paredit inspect outline --file <FILE>` makes the
/// caller do the substitution; one reading `--file src/core.lisp` does not.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The `--file` argument, if the command had one.
    pub file: Option<String>,
    /// The command path, as `capabilities` prints it.
    pub command_path: Option<String>,
}

impl Context {
    /// A command line targeting the same file, or `None` when there is no file
    /// to target and the suggestion would be a template rather than a command.
    fn command(&self, rest: &str) -> Option<String> {
        self.file
            .as_ref()
            .map(|file| format!("paredit {rest} --file {file}"))
    }
}

/// A classified failure.
#[derive(Debug, Clone)]
pub struct Diagnosis {
    pub code: ErrorCode,
    /// The message chain, outermost first, as the human rendering shows it.
    pub message: String,
    pub repairs: Vec<Repair>,
    /// The byte position the failure names, when it names one.
    ///
    /// Populated from whichever typed variant already carries a position —
    /// [`ParseError`] always does, a handful of [`StructureError`] and
    /// [`SelectionError`] variants do — rather than by adding a span to every
    /// variant. A caller holding the source text can render a caret under it;
    /// [`render_caret`] does exactly that for the CLI's own stderr output.
    pub offset: Option<usize>,
}

impl Diagnosis {
    #[must_use]
    pub fn to_json(&self) -> Json {
        json!({
            "code": self.code.label(),
            "category": self.code.category().label(),
            "category_description": self.code.category().describe(),
            "retryable": self.code.category().retryable(),
            "exit_code": self.code.exit_code(),
            // Escaped for the same reason a `Repair`'s strings are: this one
            // routinely carries a path the caller did not choose.
            "message": crate::shared::terminal_safe(&self.message).to_string(),
            "repairs": self.repairs.iter().map(Repair::to_json).collect::<Vec<_>>(),
            "offset": self.offset,
            "doc_url": self.code.doc_url(),
        })
    }
}

/// Classifies a failure and works out what would get past it.
#[must_use]
pub fn diagnose(failure: &CommandFailure, context: &Context) -> Diagnosis {
    // `chain()`, not `{failure:#}`: the alternate form was `anyhow`'s, and it
    // is the one thing a typed error does not inherit. `chain()` reproduces it
    // exactly — see `CliError::chain`.
    let message = failure.chain();
    let code = classify(failure);
    Diagnosis {
        repairs: repairs(code, context),
        offset: extract_offset(failure),
        code,
        message,
    }
}

/// The byte position `failure` names, when it names one at all.
///
/// Most failures are not about one place in the source — `RaiseTopLevel` is a
/// shape refusal, not a location — so this returns `None` far more often than
/// [`classify`] does.
const fn extract_offset(failure: &CommandFailure) -> Option<usize> {
    match failure {
        // A gate tripped on a report, not on a position in a file.
        CommandFailure::Gate(_) => None,
        CommandFailure::Error(error) => offset_of_cli(error),
    }
}

const fn offset_of_cli(error: &CliError) -> Option<usize> {
    match error {
        CliError::Sexpr(sexpr) => offset_of_sexpr(sexpr),
        CliError::Parse { source, .. } => Some(source.position()),
        _ => None,
    }
}

const fn offset_of_sexpr(error: &SexprError) -> Option<usize> {
    match error {
        SexprError::Structure(structure) => offset_of_structure(structure),
        SexprError::Selection(selection) | SexprError::EditSelection { source: selection } => {
            offset_of_selection(selection)
        }
        SexprError::Parse(parse) => Some(parse.position()),
        SexprError::Path(PathError::InvalidSegment { .. }) | SexprError::Symbol(_) => None,
    }
}

const fn offset_of_structure(error: &StructureError) -> Option<usize> {
    match error {
        StructureError::NotInsideStringLiteral { offset }
        | StructureError::OffsetOutsideDocument { offset, .. }
        | StructureError::NothingToDelete { offset } => Some(*offset),
        _ => None,
    }
}

const fn offset_of_selection(error: &SelectionError) -> Option<usize> {
    match error {
        SelectionError::NoExpressionAtOffset { offset } => Some(*offset),
        // The start and end of an invalid span are not the same position, but
        // either is a reasonable place to point a caret: both lie on or past
        // the boundary that made the span invalid.
        SelectionError::InvalidSpan {
            source: SpanError::StartExceedsEnd { start, .. },
        } => Some(*start),
        SelectionError::InvalidSpan {
            source: SpanError::EndExceedsInput { end, .. },
        } => Some(*end),
        SelectionError::InvalidSpan {
            source: SpanError::NotCharBoundary,
        }
        | SelectionError::SourceMismatch
        | SelectionError::TreeMismatch
        | SelectionError::PathNotReachable { .. }
        | SelectionError::PathSegmentOutOfRange { .. } => None,
    }
}

/// Renders a caret under the byte position `offset` names, rustc/miette
/// style: the source line the offset falls on, then a second line of spaces
/// and a `^` under the exact column.
///
/// `None` when `offset` does not land inside `source` — a stale offset from a
/// file that changed underneath the command should not panic or point at the
/// wrong line, it should render nothing.
#[must_use]
pub fn render_caret(source: &str, offset: usize) -> Option<String> {
    if offset > source.len() || !source.is_char_boundary(offset) {
        return None;
    }
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[offset..]
        .find('\n')
        .map_or(source.len(), |index| offset + index);
    let line_number = source[..line_start].matches('\n').count() + 1;
    let column = source[line_start..offset].chars().count() + 1;
    let line = &source[line_start..line_end];

    let gutter = format!("{line_number}");
    let indent = " ".repeat(gutter.len());
    let caret_indent = " ".repeat(column.saturating_sub(1));
    Some(format!(
        "{indent} |\n{gutter} | {line}\n{indent} | {caret_indent}^"
    ))
}

/// Which exit code this failure earns.
///
/// This was six `downcast_ref` probes against an `anyhow::Error`, ending in
/// `.map_or(ErrorCode::Internal, ...)`. The probes for `SexprError`,
/// `SelectorError`, `WorkspaceError` and `ParseError` existed because those
/// types could arrive bare, having been absorbed by `anyhow` at some `?`.
/// They now arrive as the [`CliError`] variants that wrap them, which
/// [`classify_cli`] was already matching on — so the probes were a second,
/// hand-maintained copy of a table that had to agree with the first.
///
/// The `Internal` fallback is what made that dangerous rather than merely
/// redundant: a failure whose type nobody had added a probe for was reported
/// to the user as an internal malfunction, and adding a `CliError` variant
/// could not fail to compile. Now it can.
const fn classify(failure: &CommandFailure) -> ErrorCode {
    match failure {
        CommandFailure::Gate(_) => ErrorCode::GateFailed,
        CommandFailure::Error(error) => code_for_cli_error(error),
    }
}

/// Which code a [`CliError`] earns.
///
/// Public for the same reason [`code_for_edit_refusal`] is: a feature that
/// wraps one — `RemoveUnusedError::Source` carries whatever its port's adapter
/// failed with — has to answer this question, and answering it twice is how
/// the two answers drift apart.
#[must_use]
pub const fn code_for_cli_error(error: &CliError) -> ErrorCode {
    match error {
        CliError::Io { .. } | CliError::Bare(_) => ErrorCode::EnvironmentIo,
        CliError::Refused(refusal) => classify_refusal(refusal),
        CliError::Sexpr(sexpr) => code_for_sexpr_error(sexpr),
        CliError::Edit(refusal) => code_for_edit_refusal(refusal),
        // The feature stated its own code — see `ClassifiedRefusal`.
        CliError::Feature(refusal) => refusal.code(),
        CliError::Workspace(workspace) => classify_workspace(workspace),
        CliError::Argument(argument) => classify_argument(argument),
        CliError::Selector(selector) => classify_selector(selector),
        // Every way a `--rewrite` template can be wrong is the caller's
        // command line: unbalanced, two forms where one was wanted, a `?name`
        // the pattern does not bind.
        CliError::Pattern(_) | CliError::Rewrite(_) => ErrorCode::SelectionPatternInvalid,
        CliError::NotUtf8 { .. } | CliError::NotValidEncoding { .. } => ErrorCode::InputNotUtf8,
        CliError::Parse { .. } => ErrorCode::InputUnparsable,
        // A sidecar this tool owns did not parse. Same code as a source file
        // that did not: to a caller both mean "something read is malformed",
        // and the message already names which file.
        CliError::Json { .. } => ErrorCode::InputUnparsable,
        // Not `InputUnparsable`: nothing the caller supplied is at fault. This
        // variant only ever carries the failure to serialize a document this
        // tool built itself a line earlier, so it is a malfunction here — and
        // it is what keeps `Internal` a reachable code now that classification
        // is a total match rather than a fallback.
        CliError::JsonBare(_) => ErrorCode::Internal,
        CliError::CleanupAlsoFailed(CleanupFailure { .. }) => ErrorCode::EnvironmentRollbackFailed,
        CliError::WriteTarget(WriteTargetError::ParentNotADirectory { .. }) => {
            ErrorCode::EnvironmentIo
        }
        CliError::WriteTarget(_) => ErrorCode::RefusalWriteTarget,
        CliError::BackupCleanupAfterCommit { .. } => ErrorCode::EnvironmentCleanupAfterCommit,
        // The journal is this tool's own bookkeeping; failing at it is an
        // environment problem, and a replay failure is the rollback case.
        CliError::Journal(_) => ErrorCode::EnvironmentRollbackFailed,
        // The verification command named on the command line is not runnable
        // here — nothing about the source is at fault.
        CliError::External(_) => ErrorCode::EnvironmentUnavailable,
        CliError::TimedOut(_) => ErrorCode::EnvironmentTimeout,
    }
}

const fn classify_argument(error: &ArgumentError) -> ErrorCode {
    match error {
        ArgumentError::NoInput => ErrorCode::ArgumentNoInput,
        ArgumentError::TargetRequired => ErrorCode::ArgumentTargetRequired,
        ArgumentError::TargetAmbiguous => ErrorCode::ArgumentTargetAmbiguous,
        ArgumentError::WriteRequiresFile => ErrorCode::ArgumentWriteRequiresFile,
        // Lives in `ArgumentError` but is a safety refusal, not a usage
        // problem: the command line was fine and an earlier edit moved the
        // form this one was going to touch.
        ArgumentError::ConflictingInputSelectors => ErrorCode::ArgumentConflictingInputs,
        ArgumentError::FlagCombination { .. } => ErrorCode::ArgumentFlagCombination,
        ArgumentError::InvalidGlob { .. } => ErrorCode::ArgumentInvalidGlob,
        ArgumentError::SinceRequiresRepository => ErrorCode::ArgumentNeedsRepository,
        // All three mean the same thing to a caller: the selector ran and
        // produced no file inside the scanned roots.
        ArgumentError::NoManifestFound
        | ArgumentError::EmptyPathList
        | ArgumentError::EmptyArchive => ErrorCode::ArgumentNoInputsProduced,
        ArgumentError::ArchiveRequiresDestination => ErrorCode::ArgumentArchiveDestination,
        ArgumentError::AllMatchShifted { .. } => ErrorCode::RefusalMatchShifted,
        // A second address was asked for and not given: the same thing
        // `--path`'s absence means, so the same code.
        ArgumentError::SecondTargetRequired => ErrorCode::ArgumentTargetRequired,
        ArgumentError::KillRingIndexOutOfRange { .. } => ErrorCode::ArgumentKillRingIndex,
        ArgumentError::UnknownName { .. } => ErrorCode::ArgumentUnknownName,
    }
}

const fn classify_refusal(error: &IoRefusal) -> ErrorCode {
    match error {
        IoRefusal::InputTooLarge { .. } => ErrorCode::RefusalInputTooLarge,
        IoRefusal::WriteSymlink { .. }
        | IoRefusal::WriteNonRegularFile { .. }
        | IoRefusal::DuplicateWriteTarget { .. }
        | IoRefusal::WriteTargetHasNoFileName { .. }
        | IoRefusal::WritableParentDirectory { .. }
        | IoRefusal::SymlinkedAncestorDirectory { .. } => ErrorCode::RefusalWriteTarget,
        IoRefusal::DryRun => ErrorCode::RefusalDryRun,
        IoRefusal::WriteRequiresUtf8Encoding { .. } => ErrorCode::RefusalEncodingWrite,
        IoRefusal::OverlappingRewriteSpans => ErrorCode::RefusalOverlappingSpans,
        IoRefusal::RewriteSpanOutOfBounds => ErrorCode::RefusalSpanOutOfBounds,
        IoRefusal::RewriteDoesNotReparse | IoRefusal::NamedRewriteDoesNotReparse { .. } => {
            ErrorCode::RefusalRewriteDoesNotReparse
        }
        // The six time-of-check-to-time-of-use guards, plus a lock held by
        // another process. One code because the response is the same for
        // all seven: the situation may already have resolved itself, so
        // re-read and retry rather than treat it as a shape problem.
        IoRefusal::ReplacedTarget { .. }
        | IoRefusal::HardLinkedTarget { .. }
        | IoRefusal::TargetReplacedSinceParsing { .. }
        | IoRefusal::TargetChangedSinceParsing { .. }
        | IoRefusal::TargetRemovedSinceParsing { .. }
        | IoRefusal::WriteTargetLocked { .. } => ErrorCode::RefusalTargetChanged,
    }
}

/// A selector is the other way of naming a form, so its failures are
/// selection failures. `Missing` and `Conflict` are the exceptions: those are
/// "you named nothing" and "you named two things", which is the same problem
/// `--path`/`--at` already has and deserves the same code.
const fn classify_selector(error: &SelectorError) -> ErrorCode {
    match error {
        // "You named nothing" and "you named two things" are the problems
        // `--path`/`--at` already has, and deserve the same codes.
        SelectorError::Missing { .. } => ErrorCode::ArgumentTargetRequired,
        SelectorError::Conflict { .. } => ErrorCode::ArgumentTargetAmbiguous,

        // The pattern language itself is wrong.
        SelectorError::Pattern(_) | SelectorError::UnknownCapture { .. } => {
            ErrorCode::SelectionPatternInvalid
        }

        // The selector is well-formed and names nothing, or names too much.
        SelectorError::NoMatch { .. } | SelectorError::UnknownStableId { .. } => {
            ErrorCode::SelectionNoMatch
        }
        SelectorError::Ambiguous { .. } => ErrorCode::SelectionAmbiguous,

        // A coordinate past the end names nothing, exactly as a byte offset
        // past the end does.
        SelectorError::LineOutOfRange { .. } | SelectorError::ColumnOutOfRange { .. } => {
            ErrorCode::SelectionOffsetNotFound
        }

        // Navigating off the tree: the same answer as an unreachable path.
        SelectorError::ParentAboveRoot { .. }
        | SelectorError::ChildOutOfRange { .. }
        | SelectorError::SiblingOutOfRange { .. }
        | SelectorError::SiblingWithoutParent => ErrorCode::SelectionPathNotReachable,

        // The selector's own text does not parse.
        SelectorError::ZeroLineOrColumn { .. }
        | SelectorError::MalformedPosition { .. }
        | SelectorError::UnknownSelectorPrefix { .. }
        | SelectorError::MalformedSelector { .. }
        | SelectorError::MalformedStableId { .. }
        | SelectorError::RangeParentMismatch
        | SelectorError::RangeReversed => ErrorCode::SelectionMalformed,

        // The selection resolved; this command cannot act on that shape.
        SelectorError::RangeUnsupported { .. } => ErrorCode::InputShapeRefused,

        SelectorError::Sexpr(sexpr) => code_for_sexpr_error(sexpr),
    }
}

/// Which code a structural edit's refusal earns.
///
/// The families of `EditRefusal` were drawn for exactly this question, so the
/// mapping is one arm each. What matters is that the three outcomes call for
/// three different actions: change the file's language, select a different
/// form, or fix the source so it parses.
///
/// Public because a feature that wraps an `EditRefusal` in its own error has
/// to answer the same question, and answering it twice is how the two answers
/// drift apart. `feature/package` calls this for the `Edit` arm of
/// `PackageRefactorError` and only decides the arms core knows nothing about.
#[must_use]
pub const fn code_for_edit_refusal(refusal: &EditRefusal) -> ErrorCode {
    match refusal {
        // "stop retrying this command on this file"
        EditRefusal::Dialect(_) => ErrorCode::InputDialectUnsupported,

        // "the text this edit produced, or was given, is not a document"
        EditRefusal::Document(_) => ErrorCode::InputUnparsable,

        // "the form you selected is not the shape this edit rewrites" — the
        // conservative refusals belong here too: a form with comments in it is
        // a shape this edit declines, and the action is to select another.
        EditRefusal::Conservative(_)
        | EditRefusal::Shape(_)
        | EditRefusal::Binding(_)
        | EditRefusal::LocalFunction(_)
        | EditRefusal::Insertion(_) => ErrorCode::InputShapeRefused,

        // The tree underneath the edit declined, and it already says why.
        EditRefusal::Selection(sexpr) => code_for_sexpr_error(sexpr),
    }
}

/// Which code a syntax-layer refusal earns.
///
/// Public alongside [`code_for_edit_refusal`] and [`code_for_cli_error`], for
/// the same reason: a feature that carries a `SexprError` in its own enum has
/// to answer this, and a second answer is one that can drift.
#[must_use]
pub const fn code_for_sexpr_error(error: &SexprError) -> ErrorCode {
    match error {
        SexprError::Structure(structure) => classify_structure(structure),
        SexprError::Selection(selection) | SexprError::EditSelection { source: selection } => {
            classify_selection(selection)
        }
        SexprError::Path(PathError::InvalidSegment { .. }) => ErrorCode::SelectionPathInvalid,
        SexprError::Symbol(
            SymbolError::Empty | SymbolError::ReaderDelimiterOrWhitespace { .. },
        ) => ErrorCode::InputSymbolInvalid,
        SexprError::Parse(_) => ErrorCode::InputUnparsable,
    }
}

/// Every structural refusal answers the caller the same way: the selected form
/// is not the shape this operation works on. One code, deliberately.
const fn classify_structure(_error: &StructureError) -> ErrorCode {
    ErrorCode::InputShapeRefused
}

const fn classify_selection(error: &SelectionError) -> ErrorCode {
    match error {
        SelectionError::SourceMismatch | SelectionError::TreeMismatch => ErrorCode::SelectionStale,
        SelectionError::InvalidSpan { .. } => ErrorCode::SelectionSpanInvalid,
        SelectionError::NoExpressionAtOffset { .. } => ErrorCode::SelectionOffsetNotFound,
        SelectionError::PathNotReachable { .. } | SelectionError::PathSegmentOutOfRange { .. } => {
            ErrorCode::SelectionPathNotReachable
        }
    }
}

const fn classify_workspace(error: &WorkspaceError) -> ErrorCode {
    match error {
        WorkspaceError::Limit(_) => ErrorCode::EnvironmentWorkspaceLimit,
        WorkspaceError::Refused(_) => ErrorCode::RefusalWorkspace,
        WorkspaceError::Unavailable(_) => ErrorCode::EnvironmentUnavailable,
        WorkspaceError::Io { .. } => ErrorCode::EnvironmentIo,
    }
}

/// What to suggest trying next, given the code.
///
/// Took the failure itself until the `anyhow` removal made it visible that
/// every arm ignored it (`let _ = error;`): the repair table is a function of
/// the classification alone. Keeping the parameter suggested the suggestions
/// were tailored to the specific failure, and they are not.
fn repairs(code: ErrorCode, context: &Context) -> Vec<Repair> {
    match code {
        ErrorCode::ArgumentNoInput => vec![Repair::new("pass-flag", Message::RepairNameTheSource)],

        ErrorCode::ArgumentTargetRequired | ErrorCode::SelectionPathNotReachable => vec![
            Repair::new("inspect-first", Message::RepairListPaths)
                .with_command(context.command("inspect outline --output json")),
            Repair::new("change-selection", Message::RepairSelectByOffset),
        ],

        ErrorCode::SelectionPathInvalid => {
            vec![Repair::new("change-selection", Message::RepairPathShape)]
        }

        ErrorCode::SelectionOffsetNotFound => vec![
            Repair::new("inspect-first", Message::RepairCheckLength)
                .with_command(context.command("inspect stats --output json")),
            Repair::new("change-selection", Message::RepairSelectByPath),
        ],

        ErrorCode::SelectionStale | ErrorCode::RefusalTargetChanged => vec![
            Repair::new("re-read", Message::RepairRereadFile)
                .with_command(context.command("inspect stats --output json")),
        ],

        ErrorCode::ArgumentTargetAmbiguous => {
            vec![Repair::new("pass-flag", Message::RepairOneSelector)]
        }

        ErrorCode::ArgumentWriteRequiresFile => vec![
            Repair::new("pass-flag", Message::RepairAddFile),
            Repair::new("pass-flag", Message::RepairDropWrite),
        ],

        ErrorCode::InputUnparsable => vec![
            Repair::new("inspect-first", Message::RepairGetParseOffset)
                .with_command(context.command("inspect check --output json")),
            Repair::new("fix-source", Message::RepairCloseLists)
                .with_command(context.command("edit repair-unclosed-lists --diff")),
        ],

        ErrorCode::InputShapeRefused => vec![
            Repair::new("inspect-first", Message::RepairConfirmSelection)
                .with_command(context.command("inspect form --output json")),
            Repair::new("change-selection", Message::RepairDifferentShape),
        ],

        ErrorCode::InputSymbolInvalid => vec![Repair::new("pass-flag", Message::RepairSymbolShape)],

        // The message names the flags; the only useful addition is the help
        // text that lists what they combine with.
        ErrorCode::ArgumentFlagCombination => {
            vec![Repair::new("pass-flag", Message::RepairFlagCombination)]
        }

        // No `change-selection` repair: no selection in this file would help,
        // which is the whole reason this is not `InputShapeRefused`.
        ErrorCode::InputDialectUnsupported => vec![
            Repair::new("inspect-first", Message::RepairOtherDialect)
                .with_command(context.command("inspect dialect --output json")),
        ],

        ErrorCode::RefusalInputTooLarge => {
            vec![Repair::new("change-selection", Message::RepairSmallerInput)]
        }

        ErrorCode::RefusalWriteTarget => {
            vec![Repair::new("fix-source", Message::RepairWriteTargetShape)]
        }

        ErrorCode::RefusalRewriteDoesNotReparse => vec![
            Repair::new("inspect-first", Message::RepairNothingWritten),
            Repair::new("fix-source", Message::RepairReportDefect),
        ],

        ErrorCode::RefusalOverlappingSpans | ErrorCode::RefusalSpanOutOfBounds => {
            vec![Repair::new("re-read", Message::RepairRegeneratePlan)]
        }

        ErrorCode::RefusalWorkspace | ErrorCode::EnvironmentWorkspaceLimit => vec![
            Repair::new("pass-flag", Message::RepairBoundTheWalk),
            Repair::new("check-configuration", Message::RepairConfigureExcludes)
                .with_command(Some("paredit config show --changed-only".to_owned())),
        ],

        ErrorCode::EnvironmentRollbackFailed => {
            vec![Repair::new("re-read", Message::RepairPartialWrite)]
        }

        ErrorCode::EnvironmentCleanupAfterCommit => {
            vec![Repair::new("re-read", Message::RepairBackupLeftover)]
        }

        ErrorCode::GateFailed => vec![Repair::new("inspect-first", Message::RepairReadTheReport)],

        ErrorCode::InputNotUtf8 => vec![Repair::new("fix-source", Message::RepairConvertEncoding)],

        ErrorCode::EnvironmentTimeout => {
            vec![Repair::new("raise-budget", Message::RepairRaiseBudget)]
        }

        ErrorCode::EnvironmentIo | ErrorCode::EnvironmentUnavailable | ErrorCode::Internal => {
            // Nothing specific to say, and inventing something would cost a
            // round trip to discover it did not help. The message stands
            // alone; `Internal` additionally means the table missed a case.
            Vec::new()
        }

        ErrorCode::SelectionSpanInvalid => {
            vec![Repair::new("re-read", Message::RepairSpanBoundaries)]
        }

        ErrorCode::ArgumentConflictingInputs => {
            vec![Repair::new("pass-flag", Message::RepairOneInputSelector)]
        }
        ErrorCode::ArgumentInvalidGlob => {
            vec![Repair::new("pass-flag", Message::RepairGlobSyntax)]
        }
        ErrorCode::ArgumentNeedsRepository => {
            vec![Repair::new("pass-flag", Message::RepairNeedRepository)]
        }
        ErrorCode::ArgumentNoInputsProduced => {
            vec![Repair::new(
                "change-selection",
                Message::RepairNoInputsProduced,
            )]
        }
        ErrorCode::ArgumentArchiveDestination => {
            vec![Repair::new("pass-flag", Message::RepairArchiveDestination)]
        }
        ErrorCode::ArgumentKillRingIndex => {
            vec![Repair::new("pass-flag", Message::RepairKillRingIndex)]
        }
        ErrorCode::ArgumentUnknownName => {
            vec![Repair::new("pass-flag", Message::RepairListAvailableNames)]
        }

        ErrorCode::SelectionMalformed => vec![
            Repair::new("change-selection", Message::RepairSelectorSyntax),
            Repair::new("inspect-first", Message::RepairShowMatches)
                .with_command(context.command("inspect resolve --output json")),
        ],

        ErrorCode::SelectionNoMatch => vec![
            Repair::new("change-selection", Message::RepairWidenSelector),
            Repair::new("inspect-first", Message::RepairShowMatches)
                .with_command(context.command("inspect resolve --output json")),
        ],

        ErrorCode::SelectionAmbiguous => vec![
            Repair::new("change-selection", Message::RepairNarrowSelector),
            Repair::new("inspect-first", Message::RepairShowMatches)
                .with_command(context.command("inspect resolve --output json")),
        ],

        ErrorCode::SelectionPatternInvalid => vec![
            Repair::new("change-selection", Message::RepairPatternSyntax),
            Repair::new("inspect-first", Message::RepairShowMatches)
                .with_command(context.command("inspect resolve --output json")),
        ],

        ErrorCode::RefusalMatchShifted => vec![
            Repair::new("change-selection", Message::RepairRerunWithoutAll),
            Repair::new("change-selection", Message::RepairNarrowSelector),
        ],

        ErrorCode::RefusalDryRun => vec![
            Repair::new("pass-flag", Message::RepairDropDryRun),
            Repair::new("pass-flag", Message::RepairUsePreviewFlag),
        ],

        ErrorCode::RefusalEncodingWrite => {
            vec![Repair::new(
                "pass-flag",
                Message::RepairDropEncodingForWrite,
            )]
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use paredit_core_syntax::sexpr::ParseError;

    fn context() -> Context {
        Context {
            file: Some("src/core.lisp".to_owned()),
            command_path: Some("edit wrap".to_owned()),
        }
    }

    fn diagnose_of(error: impl Into<CommandFailure>) -> Diagnosis {
        diagnose(&error.into(), &context())
    }

    #[test]
    fn every_code_has_a_unique_label() {
        let labels: BTreeSet<&str> = ErrorCode::ALL.iter().map(|code| code.label()).collect();
        assert_eq!(labels.len(), ErrorCode::ALL.len());
    }

    /// A label is the contract, so it has to be shaped like one: a namespace,
    /// a dot, and a kebab-case name. Nothing else, ever.
    #[test]
    fn every_label_is_a_dotted_kebab_identifier() {
        for code in ErrorCode::ALL {
            let label = code.label();
            let (namespace, name) = label
                .split_once('.')
                .unwrap_or_else(|| panic!("{label} has no namespace"));
            assert!(!namespace.is_empty() && !name.is_empty(), "{label}");
            assert!(
                label
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.'),
                "{label} is not lowercase kebab"
            );
        }
    }

    #[test]
    fn only_a_gate_carries_the_gate_exit_status() {
        for code in ErrorCode::ALL {
            let expected = if code == ErrorCode::GateFailed { 3 } else { 1 };
            assert_eq!(code.exit_code(), expected, "{}", code.label());
        }
    }

    /// A selection failure is the one an agent most wants to retry and least
    /// should: the same path will not resolve on the second attempt either.
    #[test]
    fn only_an_environment_failure_is_retryable() {
        assert!(!Category::Selection.retryable());
        assert!(!Category::Argument.retryable());
        assert!(!Category::Gate.retryable());
        assert!(Category::Environment.retryable());
    }

    #[test]
    fn an_argument_failure_is_classified_and_answered() {
        let diagnosis = diagnose_of(CliError::from(ArgumentError::TargetRequired));
        assert_eq!(diagnosis.code, ErrorCode::ArgumentTargetRequired);
        assert_eq!(diagnosis.code.category(), Category::Argument);
        assert!(!diagnosis.repairs.is_empty());
    }

    /// The point of the `Context`: a suggestion you can run, not a template.
    #[test]
    fn a_repair_names_the_file_the_command_was_pointed_at() {
        let diagnosis = diagnose_of(CliError::from(ArgumentError::TargetRequired));
        let command = diagnosis.repairs[0].command.as_deref().expect("a command");
        assert_eq!(
            command,
            "paredit inspect outline --output json --file src/core.lisp"
        );
    }

    #[test]
    fn without_a_file_a_repair_is_prose_rather_than_a_broken_command() {
        let diagnosis = diagnose(
            &CliError::from(ArgumentError::TargetRequired).into(),
            &Context::default(),
        );
        assert!(
            diagnosis
                .repairs
                .iter()
                .all(|repair| repair.command.is_none())
        );
        assert!(
            diagnosis
                .repairs
                .iter()
                .all(|repair| !repair.detail().is_empty())
        );
    }

    #[test]
    fn a_gate_failure_is_a_gate_rather_than_a_malfunction() {
        let diagnosis = diagnose(&crate::gate::gate_failure("boom"), &context());
        assert_eq!(diagnosis.code, ErrorCode::GateFailed);
        assert_eq!(diagnosis.code.exit_code(), 3);
        assert!(!diagnosis.code.category().retryable());
    }

    #[test]
    fn a_selection_failure_offers_the_other_way_of_selecting() {
        let diagnosis = diagnose_of(CliError::from(SexprError::from(
            SelectionError::PathNotReachable {
                path: "0.9".to_owned(),
            },
        )));
        assert_eq!(diagnosis.code, ErrorCode::SelectionPathNotReachable);
        assert!(
            diagnosis
                .repairs
                .iter()
                .any(|repair| repair.detail().contains("--at")),
            "{:?}",
            diagnosis.repairs
        );
    }

    #[test]
    fn an_offset_failure_offers_the_other_way_round() {
        let diagnosis = diagnose_of(CliError::from(SexprError::from(
            SelectionError::NoExpressionAtOffset { offset: 9_999 },
        )));
        assert_eq!(diagnosis.code, ErrorCode::SelectionOffsetNotFound);
        assert!(
            diagnosis
                .repairs
                .iter()
                .any(|repair| repair.detail().contains("--path"))
        );
    }

    /// The six time-of-check-to-time-of-use guards collapse onto one code,
    /// because a caller's response to all of them is identical.
    #[test]
    fn every_stale_target_refusal_shares_one_code() {
        for refusal in [
            IoRefusal::ReplacedTarget {
                path: "a".to_owned(),
            },
            IoRefusal::HardLinkedTarget {
                path: "a".to_owned(),
            },
            IoRefusal::TargetReplacedSinceParsing {
                path: "a".to_owned(),
            },
            IoRefusal::TargetChangedSinceParsing {
                path: "a".to_owned(),
            },
            IoRefusal::TargetRemovedSinceParsing {
                path: "a".to_owned(),
            },
        ] {
            assert_eq!(
                diagnose_of(CliError::from(refusal)).code,
                ErrorCode::RefusalTargetChanged
            );
        }
    }

    /// The one failure that must not read as "nothing happened".
    #[test]
    fn a_post_commit_cleanup_failure_says_not_to_redo_the_operation() {
        let diagnosis = diagnose_of(CliError::BackupCleanupAfterCommit {
            details: "denied".to_owned(),
        });
        assert_eq!(diagnosis.code, ErrorCode::EnvironmentCleanupAfterCommit);
        assert!(
            diagnosis.repairs[0].detail().contains("Do not redo"),
            "{:?}",
            diagnosis.repairs
        );
    }

    /// There is no longer such a thing as an unclassified failure.
    ///
    /// This test used to build `anyhow!("something else entirely")` and assert
    /// it came back as [`ErrorCode::Internal`] — the fallback arm of a
    /// `downcast` chain. Since [`CommandFailure`] is a closed sum, a failure
    /// that classifies to nothing cannot be constructed to test with: the
    /// nearest thing is to check that every variant of the sum classifies to
    /// something other than the "the table missed a case" code.
    #[test]
    fn every_failure_classifies_to_something_other_than_internal() {
        let failures = [
            CommandFailure::from(gate_failure_of("policy failed")),
            CommandFailure::from(CliError::from(ArgumentError::NoInput)),
            CommandFailure::from(CliError::from(SexprError::from(
                StructureError::RaiseTopLevel,
            ))),
            CommandFailure::from(CliError::Bare(std::io::Error::other("disk"))),
        ];

        for failure in &failures {
            assert_ne!(
                diagnose(failure, &context()).code,
                ErrorCode::Internal,
                "{failure}"
            );
        }
    }

    fn gate_failure_of(message: &str) -> crate::gate::GateFailure {
        crate::gate::GateFailure(message.to_owned())
    }

    /// The rendered message is the whole `source()` chain, joined with `": "`.
    ///
    /// This is what `anyhow`'s `{:#}` did and what `thiserror`'s `Display`
    /// does not: without `error_chain` the message here would be only
    /// `"outer"`, and the reason would be lost.
    #[test]
    fn the_message_is_the_flattened_chain() {
        let failure = CommandFailure::from(CliError::Io {
            context: "outer".to_owned(),
            source: std::io::Error::other("leaf"),
        });
        assert_eq!(diagnose(&failure, &context()).message, "outer: leaf");
    }

    #[test]
    fn the_json_carries_the_code_category_and_repairs() {
        let diagnosis = diagnose_of(CliError::from(ArgumentError::NoInput));
        let json = diagnosis.to_json();
        assert_eq!(json["code"], "argument.no-input");
        assert_eq!(json["category"], "argument");
        assert_eq!(json["retryable"], false);
        assert_eq!(json["exit_code"], 1);
        assert!(!json["repairs"].as_array().expect("repairs").is_empty());
        assert!(json["offset"].is_null());
        assert_eq!(
            json["doc_url"],
            "https://nerima-lisp.github.io/paredit-cli/errors/#argument.no-input"
        );
    }

    /// A parse failure always names a position: every `ParseError` variant
    /// carries one, so the diagnosis should surface it without a caller
    /// having to know which variant it was.
    #[test]
    fn a_parse_failure_carries_its_byte_offset() {
        let diagnosis = diagnose_of(CliError::Parse {
            origin: "a.lisp".to_owned(),
            location: "1".to_owned(),
            source: ParseError::UnclosedList(7),
        });
        assert_eq!(diagnosis.offset, Some(7));
        assert_eq!(diagnosis.to_json()["offset"], 7);
    }

    /// Most failures are not about one place in the source, and the offset
    /// should say so plainly rather than guessing.
    #[test]
    fn a_shape_refusal_carries_no_offset() {
        let diagnosis = diagnose_of(CliError::from(SexprError::from(
            StructureError::RaiseTopLevel,
        )));
        assert_eq!(diagnosis.offset, None);
    }

    #[test]
    fn a_no_expression_at_offset_failure_carries_the_offset_it_named() {
        let diagnosis = diagnose_of(CliError::from(SexprError::from(
            SelectionError::NoExpressionAtOffset { offset: 11 },
        )));
        assert_eq!(diagnosis.offset, Some(11));
    }

    #[test]
    fn the_caret_points_at_the_right_line_and_column() {
        let source = "(foo\n  (bar))\n";
        // Byte 7 is the '(' that opens "(bar))", on the second line, third column.
        let rendered = render_caret(source, 7).expect("offset is inside the source");
        assert_eq!(rendered, "  |\n2 |   (bar))\n  |   ^");
    }

    #[test]
    fn a_caret_on_the_first_line_needs_no_leading_newline_scan() {
        let source = "(foo bar)";
        // Byte 5 is the 'b' in "bar", on the only line, sixth column.
        let rendered = render_caret(source, 5).expect("offset is inside the source");
        assert_eq!(rendered, "  |\n1 | (foo bar)\n  |      ^");
    }

    #[test]
    fn a_caret_past_the_end_of_the_source_renders_nothing() {
        assert_eq!(render_caret("(foo)", 999), None);
    }

    #[test]
    fn a_caret_not_on_a_char_boundary_renders_nothing() {
        // "é" is two UTF-8 bytes; offset 1 lands inside it.
        assert_eq!(render_caret("é", 1), None);
    }

    /// `doc_url` promises an anchor exists at `docs/src/errors.md`. Checked
    /// against the file itself — via `include_str!`, so this runs regardless
    /// of the test binary's working directory — rather than trusted, so a
    /// code added without its documentation section fails here instead of
    /// producing a dead link.
    #[test]
    fn every_error_code_has_a_documented_anchor() {
        let doc = include_str!("../../../../docs/src/errors.md");
        for code in ErrorCode::ALL {
            let anchor = format!("{{ #{} }}", code.label());
            assert!(
                doc.contains(&anchor),
                "docs/src/errors.md is missing an anchor for {}: expected `{anchor}`",
                code.label()
            );
        }
    }
}
