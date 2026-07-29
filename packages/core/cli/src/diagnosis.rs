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

use paredit_core_syntax::sexpr::{
    ParseError, PathError, SelectionError, SexprError, StructureError, SymbolError,
};
use paredit_core_workspace::workspace::WorkspaceError;
use serde_json::{Value as Json, json};

use crate::error::{ArgumentError, CleanupFailure, CliError, IoRefusal, WriteTargetError};
use crate::gate::{GATE_FAILURE_EXIT_CODE, GateFailure};
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

    SelectionPathNotReachable,
    SelectionPathInvalid,
    SelectionOffsetNotFound,
    SelectionStale,
    SelectionSpanInvalid,

    InputUnparsable,
    InputNotUtf8,
    InputShapeRefused,
    InputSymbolInvalid,

    RefusalInputTooLarge,
    RefusalWriteTarget,
    RefusalTargetChanged,
    RefusalRewriteDoesNotReparse,
    RefusalOverlappingSpans,
    RefusalSpanOutOfBounds,
    RefusalWorkspace,
    RefusalDryRun,

    EnvironmentIo,
    EnvironmentWorkspaceLimit,
    EnvironmentUnavailable,
    EnvironmentRollbackFailed,
    EnvironmentCleanupAfterCommit,

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

            Self::SelectionPathNotReachable => "selection.path-not-reachable",
            Self::SelectionPathInvalid => "selection.path-invalid",
            Self::SelectionOffsetNotFound => "selection.offset-not-found",
            Self::SelectionStale => "selection.stale",
            Self::SelectionSpanInvalid => "selection.span-invalid",

            Self::InputUnparsable => "input.unparsable",
            Self::InputNotUtf8 => "input.not-utf8",
            Self::InputShapeRefused => "input.shape-refused",
            Self::InputSymbolInvalid => "input.symbol-invalid",

            Self::RefusalInputTooLarge => "refusal.input-too-large",
            Self::RefusalWriteTarget => "refusal.write-target",
            Self::RefusalTargetChanged => "refusal.target-changed",
            Self::RefusalRewriteDoesNotReparse => "refusal.rewrite-does-not-reparse",
            Self::RefusalOverlappingSpans => "refusal.overlapping-spans",
            Self::RefusalSpanOutOfBounds => "refusal.span-out-of-bounds",
            Self::RefusalWorkspace => "refusal.workspace",
            Self::RefusalDryRun => "refusal.dry-run",

            Self::EnvironmentIo => "environment.io",
            Self::EnvironmentWorkspaceLimit => "environment.workspace-limit",
            Self::EnvironmentUnavailable => "environment.unavailable",
            Self::EnvironmentRollbackFailed => "environment.rollback-failed",
            Self::EnvironmentCleanupAfterCommit => "environment.cleanup-after-commit",

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
            | Self::ArgumentWriteRequiresFile => Category::Argument,

            Self::SelectionPathNotReachable
            | Self::SelectionPathInvalid
            | Self::SelectionOffsetNotFound
            | Self::SelectionStale
            | Self::SelectionSpanInvalid => Category::Selection,

            Self::InputUnparsable
            | Self::InputNotUtf8
            | Self::InputShapeRefused
            | Self::InputSymbolInvalid => Category::Input,

            Self::RefusalInputTooLarge
            | Self::RefusalWriteTarget
            | Self::RefusalTargetChanged
            | Self::RefusalRewriteDoesNotReparse
            | Self::RefusalOverlappingSpans
            | Self::RefusalSpanOutOfBounds
            | Self::RefusalWorkspace
            | Self::RefusalDryRun => Category::Refusal,

            Self::EnvironmentIo
            | Self::EnvironmentWorkspaceLimit
            | Self::EnvironmentUnavailable
            | Self::EnvironmentRollbackFailed
            | Self::EnvironmentCleanupAfterCommit => Category::Environment,

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

    /// Every code, so a contract test can check the table is total and the
    /// labels unique.
    pub const ALL: [Self; 28] = [
        Self::ArgumentNoInput,
        Self::ArgumentTargetRequired,
        Self::ArgumentTargetAmbiguous,
        Self::ArgumentWriteRequiresFile,
        Self::SelectionPathNotReachable,
        Self::SelectionPathInvalid,
        Self::SelectionOffsetNotFound,
        Self::SelectionStale,
        Self::SelectionSpanInvalid,
        Self::InputUnparsable,
        Self::InputNotUtf8,
        Self::InputShapeRefused,
        Self::InputSymbolInvalid,
        Self::RefusalInputTooLarge,
        Self::RefusalWriteTarget,
        Self::RefusalTargetChanged,
        Self::RefusalRewriteDoesNotReparse,
        Self::RefusalOverlappingSpans,
        Self::RefusalSpanOutOfBounds,
        Self::RefusalWorkspace,
        Self::RefusalDryRun,
        Self::EnvironmentIo,
        Self::EnvironmentWorkspaceLimit,
        Self::EnvironmentUnavailable,
        Self::EnvironmentRollbackFailed,
        Self::EnvironmentCleanupAfterCommit,
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
        })
    }
}

/// Classifies a failure and works out what would get past it.
#[must_use]
pub fn diagnose(error: &anyhow::Error, context: &Context) -> Diagnosis {
    let message = format!("{error:#}");
    let code = classify(error);
    Diagnosis {
        repairs: repairs(code, error, context),
        code,
        message,
    }
}

fn classify(error: &anyhow::Error) -> ErrorCode {
    if error.downcast_ref::<GateFailure>().is_some() {
        return ErrorCode::GateFailed;
    }
    if let Some(sexpr) = error.downcast_ref::<SexprError>() {
        return classify_sexpr(sexpr);
    }
    if let Some(workspace) = error.downcast_ref::<WorkspaceError>() {
        return classify_workspace(workspace);
    }
    if let Some(parse) = error.downcast_ref::<ParseError>() {
        let _ = parse;
        return ErrorCode::InputUnparsable;
    }
    error
        .downcast_ref::<CliError>()
        .map_or(ErrorCode::Internal, classify_cli)
}

const fn classify_cli(error: &CliError) -> ErrorCode {
    match error {
        CliError::Io { .. } | CliError::Bare(_) => ErrorCode::EnvironmentIo,
        CliError::Refused(refusal) => classify_refusal(refusal),
        CliError::Sexpr(sexpr) => classify_sexpr(sexpr),
        CliError::Workspace(workspace) => classify_workspace(workspace),
        CliError::Argument(argument) => classify_argument(argument),
        CliError::NotUtf8 { .. } => ErrorCode::InputNotUtf8,
        CliError::Parse { .. } => ErrorCode::InputUnparsable,
        CliError::CleanupAlsoFailed(CleanupFailure { .. }) => ErrorCode::EnvironmentRollbackFailed,
        CliError::WriteTarget(WriteTargetError::ParentNotADirectory { .. }) => {
            ErrorCode::EnvironmentIo
        }
        CliError::WriteTarget(_) => ErrorCode::RefusalWriteTarget,
        CliError::BackupCleanupAfterCommit { .. } => ErrorCode::EnvironmentCleanupAfterCommit,
    }
}

const fn classify_argument(error: &ArgumentError) -> ErrorCode {
    match error {
        ArgumentError::NoInput => ErrorCode::ArgumentNoInput,
        ArgumentError::TargetRequired => ErrorCode::ArgumentTargetRequired,
        ArgumentError::TargetAmbiguous => ErrorCode::ArgumentTargetAmbiguous,
        ArgumentError::WriteRequiresFile => ErrorCode::ArgumentWriteRequiresFile,
    }
}

const fn classify_refusal(error: &IoRefusal) -> ErrorCode {
    match error {
        IoRefusal::InputTooLarge { .. } => ErrorCode::RefusalInputTooLarge,
        IoRefusal::WriteSymlink { .. }
        | IoRefusal::WriteNonRegularFile { .. }
        | IoRefusal::DuplicateWriteTarget { .. }
        | IoRefusal::WriteTargetHasNoFileName { .. }
        | IoRefusal::WritableParentDirectory { .. } => ErrorCode::RefusalWriteTarget,
        IoRefusal::DryRun => ErrorCode::RefusalDryRun,
        IoRefusal::OverlappingRewriteSpans => ErrorCode::RefusalOverlappingSpans,
        IoRefusal::RewriteSpanOutOfBounds => ErrorCode::RefusalSpanOutOfBounds,
        IoRefusal::RewriteDoesNotReparse | IoRefusal::NamedRewriteDoesNotReparse { .. } => {
            ErrorCode::RefusalRewriteDoesNotReparse
        }
        // The six time-of-check-to-time-of-use guards. One code because the
        // response to all of them is the same: the file moved under us.
        IoRefusal::ReplacedTarget { .. }
        | IoRefusal::HardLinkedTarget { .. }
        | IoRefusal::TargetReplacedSinceParsing { .. }
        | IoRefusal::TargetChangedSinceParsing { .. }
        | IoRefusal::TargetRemovedSinceParsing { .. } => ErrorCode::RefusalTargetChanged,
    }
}

const fn classify_sexpr(error: &SexprError) -> ErrorCode {
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

fn repairs(code: ErrorCode, error: &anyhow::Error, context: &Context) -> Vec<Repair> {
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

        ErrorCode::EnvironmentIo | ErrorCode::EnvironmentUnavailable | ErrorCode::Internal => {
            // Nothing specific to say, and inventing something would cost a
            // round trip to discover it did not help. The message stands
            // alone; `Internal` additionally means the table missed a case.
            let _ = error;
            Vec::new()
        }

        ErrorCode::SelectionSpanInvalid => {
            vec![Repair::new("re-read", Message::RepairSpanBoundaries)]
        }

        ErrorCode::RefusalDryRun => vec![
            Repair::new("pass-flag", Message::RepairDropDryRun),
            Repair::new("pass-flag", Message::RepairUsePreviewFlag),
        ],
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn context() -> Context {
        Context {
            file: Some("src/core.lisp".to_owned()),
            command_path: Some("edit wrap".to_owned()),
        }
    }

    fn diagnose_of(error: impl Into<anyhow::Error>) -> Diagnosis {
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

    #[test]
    fn an_unclassified_failure_says_so_rather_than_guessing() {
        let diagnosis = diagnose(&anyhow::anyhow!("something else entirely"), &context());
        assert_eq!(diagnosis.code, ErrorCode::Internal);
        assert!(diagnosis.repairs.is_empty());
        assert_eq!(diagnosis.message, "something else entirely");
    }

    #[test]
    fn the_message_is_the_flattened_chain() {
        let error = anyhow::anyhow!("leaf").context("outer");
        assert_eq!(diagnose(&error, &context()).message, "outer: leaf");
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
    }
}
