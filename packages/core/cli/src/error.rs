//! Typed failures for the CLI's I/O conventions.
//!
//! Section 9.2, last of the core packages. This is the one the spec warned
//! about (§9.2.1): *do not mechanically turn every `.context(...)` into a
//! variant*, because most of the context here is a path and an operation name
//! attached to an [`std::io::Error`], and expanding those into variants gives
//! one variant per call site and explains nothing.
//!
//! So the split is between context and decision:
//!
//! - [`CliError::Io`] carries the context string, exactly as `.context()`
//!   produced it, with the [`std::io::Error`] as its source. Ten sites.
//! - [`IoRefusal`] is where the CLI *decided* not to do something a caller
//!   asked for: an input over the size limit, a write target that is a symlink
//!   or not a regular file, overlapping rewrite spans, output that does not
//!   reparse. These are policy, not I/O, and they are the ones a caller might
//!   want to handle rather than report.
//! - [`ArgumentError`] is a usage problem, resolvable by changing the command
//!   line and by nothing else.
//! - [`CleanupFailure`] is the one this conversion actually clarified — see
//!   its own documentation.
//!
//! Messages are reproduced exactly.
//!
//! [`CommandFailure`], at the bottom of this file, is the later addition: the
//! sum of "the command failed" and "a requested gate tripped", which is what a
//! command entry point actually returns and what decides the exit code.

use std::string::FromUtf8Error;

use thiserror::Error;

use paredit_core_edit::EditRefusal;
use paredit_core_syntax::selector::SelectorError;
use paredit_core_syntax::sexpr::{ParseError, SexprError};
use paredit_core_workspace::workspace::WorkspaceError;

use crate::gate::GateFailure;

/// The CLI declined to read or write something, as policy rather than because
/// the filesystem objected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IoRefusal {
    #[error("refusing to read {description}: input exceeds {limit} bytes")]
    InputTooLarge { description: String, limit: u64 },

    #[error("refusing to write symlink {path}")]
    WriteSymlink { path: String },

    #[error("refusing to write non-regular file {path}")]
    WriteNonRegularFile { path: String },

    #[error("refusing overlapping rewrite spans")]
    OverlappingRewriteSpans,

    #[error("rewrite span is outside input or not UTF-8 aligned")]
    RewriteSpanOutOfBounds,

    #[error("refusing to write: rewritten source does not reparse")]
    RewriteDoesNotReparse,

    #[error("duplicate write target {path}")]
    DuplicateWriteTarget { path: String },

    #[error("write target has no file name: {path}")]
    WriteTargetHasNoFileName { path: String },

    #[error("refusing to write {path}: rewritten source does not reparse")]
    NamedRewriteDoesNotReparse { path: String },

    // --- the target is not what it was when we read it ---
    //
    // These six are the write path's time-of-check-to-time-of-use guards.
    // Grouped here rather than in their own type because a caller's response
    // is the same for all of them: re-read and retry, or stop.
    #[error("refusing replaced target {path}")]
    ReplacedTarget { path: String },

    #[error("refusing to replace hard-linked target {path}")]
    HardLinkedTarget { path: String },

    #[error("refusing target replaced since parsing {path}")]
    TargetReplacedSinceParsing { path: String },

    #[error("refusing target changed since parsing {path}")]
    TargetChangedSinceParsing { path: String },

    #[error("refusing target removed since parsing {path}")]
    TargetRemovedSinceParsing { path: String },

    #[error("refusing writable parent directory {path}")]
    WritableParentDirectory { path: String },

    /// A directory *above* the immediate parent is a symlink. `O_NOFOLLOW`
    /// only constrains the last path component the kernel resolves, so a
    /// symlinked grandparent (or higher) would otherwise be followed
    /// silently even though a symlinked immediate parent or a symlinked
    /// target file is already refused.
    #[error("refusing to write through symlinked ancestor directory of {path}")]
    SymlinkedAncestorDirectory { path: String },

    /// Another paredit process already holds this target's write lock.
    ///
    /// The existing digest/identity re-validation already refuses a write
    /// whose target changed underneath it, so two concurrent invocations of
    /// the same target never corrupt anything; this exists to fail one of
    /// them fast, with a clear reason, instead of doing the (possibly slow)
    /// staging work only to lose that race at the very end.
    #[error("refusing to write {path}: another process holds its write lock")]
    WriteTargetLocked { path: String },

    /// `--encoding <e>` named something other than UTF-8, and this run tried
    /// to write.
    ///
    /// The tool works in UTF-8 internally regardless of `--encoding`; writing
    /// what it holds back out under a different declared encoding, without
    /// re-encoding it, would silently change the file's on-disk bytes to
    /// UTF-8. Refused rather than done quietly.
    #[error("refusing to write: --encoding {encoding} does not support --write yet")]
    WriteRequiresUtf8Encoding { encoding: &'static str },

    /// `--dry-run` reached a write the argument layer could not turn into a
    /// preview, because the command does not spell writing as `--write`.
    #[error(
        "refusing to write: --dry-run is in force. \
         Drop it, or use the command's own preview flag (--diff on an edit, \
         --fix --diff on lint)"
    )]
    DryRun,
}

/// The path a write was aimed at cannot serve as a target.
///
/// Distinct from [`IoRefusal`] because these are not policy decisions: the
/// path is structurally unusable, and no permission or retry changes that.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WriteTargetError {
    #[error("write target parent is not a directory: {path}")]
    ParentNotADirectory { path: String },

    #[error("anchored target name does not match display path {path}")]
    AnchoredNameMismatch { path: String },

    #[error("stable filesystem identity is unavailable for retained parent directory {path}")]
    ParentIdentityUnavailable { path: String },
}

/// The command line does not describe a runnable request.
///
/// Separate from every other kind because the fix is always the same shape —
/// change the arguments — and because these are raised before any I/O happens.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArgumentError {
    #[error(
        "no input: pass --file <path> or pipe source into stdin \
         (refusing to wait on an interactive terminal)"
    )]
    NoInput,

    /// Raised by the commands that still take only `--path`/`--at`. Commands
    /// carrying the full [`SelectorArgs`](crate::args::SelectorArgs) surface
    /// report `SelectorError::Missing`, which names every selector instead.
    #[error("target required: pass --path or --at")]
    TargetRequired,

    #[error("pass only one of --path or --at")]
    TargetAmbiguous,

    #[error("--write requires --file")]
    WriteRequiresFile,

    #[error(
        "pass only one of --since, --from-git, --from-manifest, --from-archive or \
         --paths-from: each replaces the file set, and combining them would let one \
         win silently"
    )]
    ConflictingInputSelectors,

    /// Two flags that do not combine, or one that needs another.
    ///
    /// Carries the message for the same reason [`CliError::Io`] carries its
    /// context: there are sixteen of these across the feature packages, each
    /// naming a different pair of flags on a different command, and a variant
    /// per site would name the site rather than the failure. What the type is
    /// for is the *classification* — every one of them is a usage problem
    /// fixable only by changing the command line, and every one of them was
    /// reaching the user as `internal.unclassified` because `bail!` erased
    /// even that much.
    #[error("{message}")]
    FlagCombination { message: String },

    #[error("invalid {flag} pattern {pattern}: {reason}")]
    InvalidGlob {
        flag: &'static str,
        pattern: String,
        reason: String,
    },

    #[error("--since requires a git repository containing the scanned roots")]
    SinceRequiresRepository,

    #[error(
        "--from-manifest found no readable manifest: expected a .asd, deps.edn, \
         shadow-cljs.edn or project.clj beside the scanned roots"
    )]
    NoManifestFound,

    #[error("--paths-from produced no path inside the scanned roots")]
    EmptyPathList,

    #[error("--from-archive requires --extract-to <DIR>")]
    ArchiveRequiresDestination,

    #[error("--from-archive extracted no file inside the scanned roots")]
    EmptyArchive,

    /// One `--all` match was disturbed by an edit applied to a later one.
    ///
    /// Reachable only for edits that reach outside their own form — slurp and
    /// barf move a sibling across the boundary — and only when two matches are
    /// close enough to collide. Refusing keeps the file consistent with what
    /// the selector described when it was resolved.
    #[error(
        "--all: the form at byte {start} was changed by an edit to a later match; \
         re-run without --all, or narrow the selector"
    )]
    AllMatchShifted { start: usize },

    #[error("second target required: pass --with-path, --with-at or --with-select")]
    SecondTargetRequired,

    #[error("kill ring {path} holds {available} entries; --index {index} is out of range")]
    KillRingIndexOutOfRange {
        path: String,
        index: usize,
        available: usize,
    },
}

/// A write failed, and then undoing it failed too.
///
/// This is the variant the conversion earned. With `anyhow` the recovery
/// failure was layered on with `.context()`, which makes it the *outermost*
/// message — so the text a user saw led with "rollback/cleanup also failed"
/// and buried the write error that caused it as a cause. Reversing that would
/// change output, so the rendering is unchanged; what changes is that the two
/// failures are now separately reachable instead of being one flattened
/// string, and the type says out loud that the working tree may be in a
/// partially-written state.
#[derive(Debug, Error)]
#[error("{summary}")]
pub struct CleanupFailure {
    /// The recovery failure, rendered as `.context()` rendered it.
    pub summary: String,
    /// The failure that triggered the recovery attempt.
    #[source]
    pub cause: Box<CliError>,
}

/// Anything the CLI's I/O layer can fail with.
#[derive(Debug, Error)]
pub enum CliError {
    /// An [`std::io::Error`] with the operation and path that produced it.
    ///
    /// A `String` rather than a variant per call site: the context is the
    /// whole content, there are ten of them, and one variant each would name
    /// the call site rather than the failure.
    #[error("{context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// An [`std::io::Error`] reaching a caller with no context attached.
    ///
    /// The helpers that return a bare `io::Result` - identity snapshots,
    /// digests - were already reaching `anyhow` this way, and the rendered
    /// message was the `io::Error`'s own. `transparent` keeps that exactly.
    #[error(transparent)]
    Bare(#[from] std::io::Error),

    #[error(transparent)]
    Refused(#[from] IoRefusal),

    /// The syntax layer refused a selection or an edit the CLI passed through.
    ///
    /// Kept whole rather than flattened, so a caller can still tell a shape
    /// refusal from a stale selection - see `paredit_core_syntax::sexpr::error`.
    #[error(transparent)]
    Sexpr(#[from] SexprError),

    /// Workspace discovery declined or hit a limit.
    ///
    /// Also kept whole: `WorkspaceError` already separates a size limit from a
    /// safety refusal, and flattening it here would undo that.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),

    #[error(transparent)]
    Argument(#[from] ArgumentError),

    /// A selector named no form, named too many, or does not parse.
    ///
    /// Kept whole rather than folded into [`Self::Argument`]: a selector
    /// failure carries what it looked for and how many it found, which is
    /// what an agent needs in order to widen or narrow and try again.
    #[error(transparent)]
    Selector(#[from] SelectorError),

    #[error("{description} is not valid UTF-8")]
    NotUtf8 {
        description: String,
        #[source]
        source: FromUtf8Error,
    },

    /// `--encoding` named an encoding, and this input's bytes are not valid
    /// in it. `encoding_rs` decodes leniently by default (replacement
    /// characters for a malformed sequence); refusing here instead keeps a
    /// bad `--encoding` guess from silently reaching an analysis or a diff as
    /// mangled text.
    #[error("{description} is not valid {encoding}")]
    NotValidEncoding {
        description: String,
        encoding: &'static str,
    },

    #[error("failed to parse {origin} ({location})")]
    Parse {
        origin: String,
        location: String,
        #[source]
        source: ParseError,
    },

    #[error(transparent)]
    CleanupAlsoFailed(#[from] CleanupFailure),

    #[error(transparent)]
    WriteTarget(#[from] WriteTargetError),

    /// A JSON artifact this tool owns did not parse or did not render.
    ///
    /// Distinct from [`CliError::Parse`], which is about *source* text. This
    /// one is about the tool's own sidecar files - the kill ring today - where
    /// a malformed file is a refusal rather than an empty result.
    #[error("{context}")]
    Json {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    /// A structural edit declined to run.
    ///
    /// Added when the `anyhow` removal exposed that these were reaching the
    /// user as `internal.unclassified` — "a defect in this tool" — for every
    /// one of the 107 refusals in `paredit-core-edit`. They had no variant
    /// here, so `?` absorbed them into `anyhow::Error` and the `downcast`
    /// chain in `diagnosis` fell through to its fallback. `refactor
    /// merge-nested-let` on a Scheme file reported the tool as broken rather
    /// than the command as inapplicable.
    ///
    /// Kept whole rather than flattened: `EditRefusal`'s families already
    /// separate "wrong dialect" from "wrong shape" from "output did not
    /// reparse", which is exactly the distinction the error code needs.
    #[error(transparent)]
    Edit(#[from] EditRefusal),

    /// A refusal raised inside a feature package.
    ///
    /// The escape hatch, and the reason it is safe: the set of error codes is
    /// closed and documented, while the set of feature errors is open — 30
    /// packages, each with its own vocabulary — so `CliError` cannot enumerate
    /// them without inverting the dependency direction the workspace is built
    /// on. What it *can* do is refuse to accept one that has not been
    /// classified. See [`FeatureRefusal`].
    #[error(transparent)]
    Feature(#[from] FeatureRefusal),

    /// A JSON document reaching a caller with no context attached.
    ///
    /// The counterpart to [`Self::Bare`], and for the same reason: the ~130
    /// `cli/render.rs` files that serialize their own `json!` document were
    /// already reaching `anyhow` through a bare `?`, and the rendered message
    /// was `serde_json`'s own. `transparent` keeps that exactly.
    ///
    /// Distinct from [`Self::Json`], which names a *file* the tool owns and
    /// therefore has a path worth reporting. Here there is no path: the
    /// document was built in memory a line earlier, and the failure is a bug
    /// rather than something the caller can fix.
    #[error(transparent)]
    JsonBare(#[from] serde_json::Error),

    /// The writes landed; only tidying up after them did not.
    ///
    /// Its own variant because it is the one failure here that a caller must
    /// *not* treat as "the operation did not happen". The files are written.
    #[error("writes committed successfully, but backup cleanup failed: {details}")]
    BackupCleanupAfterCommit { details: String },

    /// The undo journal could not be written, read, or replayed.
    ///
    /// Carried whole rather than flattened into [`Self::Io`]: a journal that
    /// cannot be written before a write is a different situation from one that
    /// cannot be replayed after it, and `JournalError` already separates them.
    #[error(transparent)]
    Journal(#[from] paredit_core_safety::journal::JournalError),

    /// A `--verify-command` could not be run at all.
    ///
    /// Distinct from the command running and failing, which is a gate.
    #[error(transparent)]
    External(#[from] paredit_core_safety::external::ExternalError),

    /// The wall-clock budget ran out before the work did.
    ///
    /// Distinct from every other variant because nothing is wrong with the
    /// input: the same command with a larger budget, or none, would succeed.
    /// A caller re-reading this as a defect in the source would be drawing the
    /// wrong conclusion, so the type says which it is.
    #[error(transparent)]
    TimedOut(#[from] paredit_core_safety::deadline::TimeoutError),
}

/// A feature's own refusal, carrying the documented code it earns.
///
/// A feature keeps its rich error type internally — `PackageRefactorError`,
/// `SimilarityCandidateCollectionError`, and so on — and converts to this at
/// the point where it hands control back to the CLI. Two things happen there,
/// and the split is the whole design:
///
/// - the **decision** is preserved as a type: [`Self::code`] is one of the
///   documented codes, and `diagnosis` reads it directly rather than guessing;
/// - the **presentation** is flattened to the rendered `source()` chain, which
///   is exactly what the boundary was going to print anyway.
///
/// Flattening the payload is a real loss and worth being explicit about: a
/// caller can no longer match on a feature's internal variants once the error
/// is here. Nothing does — `report_failure` prints it and picks an exit code —
/// and the alternative was `CliError` naming all 30 feature packages.
///
/// What this is *not* is the `anyhow` situation it replaces. `anyhow` erased
/// the payload **and** the decision, which is how 107 edit refusals reached
/// users as `internal.unclassified`. Here the code is a required constructor
/// argument: there is no way to build one without answering the question.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct FeatureRefusal {
    code: crate::diagnosis::ErrorCode,
    message: String,
}

impl FeatureRefusal {
    /// Classifies a feature's error on its way to the CLI boundary.
    ///
    /// Answer `code` from the caller's point of view: what would they *do*
    /// about it? `input.shape-refused` says "select a different form",
    /// `selection.no-match` says "widen the search". `internal.unclassified`
    /// says "this tool is broken", which is almost never the honest answer for
    /// something a feature deliberately declined.
    #[must_use]
    pub fn new(code: crate::diagnosis::ErrorCode, error: &dyn std::error::Error) -> Self {
        Self {
            code,
            // The whole chain, not just the outermost message: this is the
            // last point at which the causes are still reachable.
            message: error_chain(error),
        }
    }

    /// Classifies a refusal a feature states directly, with no inner error.
    ///
    /// The typed replacement for `bail!`, and the reason it is worth having
    /// rather than insisting on an enum variant per message: 68 of these are
    /// one-off sentences on one command each — "`--insert before/after`
    /// requires `--anchor-path`", "`move-definition` requires a top-level
    /// definition path". Nothing matches on them; they are read once, by a
    /// person or an agent, and acted on.
    ///
    /// So the message stays a string, exactly as `bail!` had it, and the thing
    /// that was actually missing becomes mandatory: `code` has no default and
    /// no inference. Where a feature's refusals *do* form a set worth matching
    /// on — `PackageRefactorError`, `ExternalCheckError` — they get a real
    /// enum and reach [`Self::new`] instead.
    #[must_use]
    pub fn message(code: crate::diagnosis::ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Which documented code this refusal earns.
    #[must_use]
    pub const fn code(&self) -> crate::diagnosis::ErrorCode {
        self.code
    }
}

impl From<FeatureRefusal> for CommandFailure {
    fn from(refusal: FeatureRefusal) -> Self {
        Self::Error(CliError::Feature(refusal))
    }
}

/// Wires a feature's error type into the CLI boundary, with its classification.
///
/// Every feature that reaches a command entry point needs the same two `From`
/// impls, and the second exists only because `From` does not chain: `?` at an
/// entry point needs `TheirError -> CommandFailure` in one step. Written by
/// hand nine times, the pair was nine chances to reach for a plausible-looking
/// code without thinking; written here, the *only* thing a feature supplies is
/// the `match` that answers the question.
///
/// ```ignore
/// paredit_core_cli::impl_classified_refusal!(BindingError, |error| match error {
///     BindingError::Edit(edit) => code_for_edit_refusal(edit),
///     BindingError::Shape(_) => ErrorCode::InputShapeRefused,
/// });
/// ```
#[macro_export]
macro_rules! impl_classified_refusal {
    ($error:ty, |$binding:ident| $code:expr) => {
        impl From<$error> for $crate::CliError {
            fn from(error: $error) -> Self {
                let code = {
                    let $binding = &error;
                    $code
                };
                Self::Feature($crate::error::FeatureRefusal::new(code, &error))
            }
        }

        impl From<$error> for $crate::CommandFailure {
            fn from(error: $error) -> Self {
                Self::Error($crate::CliError::from(error))
            }
        }
    };
}

/// Flattens a lint failure into the variant that already covers it.
///
/// Deliberately not a `CliError::Lint(LintError)` variant. `LintError`'s two
/// cases are a path that did not resolve and an expired deadline, and this
/// enum already carries both — a wrapping variant would create a second route
/// to `SelectionPathNotReachable` and `EnvironmentTimeout` that
/// `diagnosis::classify_cli` would have to keep in step with the first.
impl From<paredit_core_lint_engine::error::LintError> for CliError {
    fn from(error: paredit_core_lint_engine::error::LintError) -> Self {
        use paredit_core_lint_engine::error::LintError;
        match error {
            LintError::Selection(sexpr) => Self::Sexpr(sexpr),
            LintError::TimedOut(timeout) => Self::TimedOut(timeout),
        }
    }
}

/// Naming a rule that is not registered is a command-line problem.
///
/// Raised while *choosing* rules, before any tree exists — `RuleSelectionError`
/// documents itself that way — so it belongs with the other argument errors
/// rather than with what a lint pass can fail with.
impl From<paredit_core_lint_engine::error::RuleSelectionError> for CliError {
    fn from(error: paredit_core_lint_engine::error::RuleSelectionError) -> Self {
        Self::Argument(ArgumentError::FlagCombination {
            message: error.to_string(),
        })
    }
}

impl CliError {
    /// Attaches the same context string `anyhow`'s `.context()` would have.
    pub fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> Self {
        move |source| Self::Io {
            context: context.into(),
            source,
        }
    }

    /// Wraps this failure in the recovery failure that followed it.
    #[must_use]
    pub fn with_cleanup_failure(self, summary: impl Into<String>) -> Self {
        Self::CleanupAlsoFailed(CleanupFailure {
            summary: summary.into(),
            cause: Box::new(self),
        })
    }

    /// The message chain, joined the way `anyhow`'s `{:#}` joins it.
    ///
    /// `thiserror` renders only the outermost message; `anyhow` walks
    /// `source()` and joins with `": "`. This is what the CLI prints, and what
    /// a caller inside the workspace asserts on.
    #[must_use]
    pub fn chain(&self) -> String {
        error_chain(self)
    }

    /// The deepest error in the chain, as `anyhow::Error::root_cause` returned.
    #[must_use]
    pub fn root_cause(&self) -> &(dyn std::error::Error + 'static) {
        root_cause(self)
    }
}

/// Joins an error and its `source()` chain with `": "`.
///
/// The replacement for `anyhow`'s `{:#}`, which is the one thing a typed error
/// does not inherit: `thiserror`'s `Display` renders only the outermost
/// message. Free and `dyn`-taking rather than a method, because three
/// different types need it — [`CliError`], [`CommandFailure`], and whatever
/// error a caller's `analyze_files` closure returns.
#[must_use]
pub fn error_chain(error: &dyn std::error::Error) -> String {
    let mut rendered = error.to_string();
    let mut source = error.source();
    while let Some(next) = source {
        rendered.push_str(": ");
        rendered.push_str(&next.to_string());
        source = next.source();
    }
    rendered
}

/// The deepest error in a `source()` chain.
#[must_use]
pub fn root_cause<'a>(
    error: &'a (dyn std::error::Error + 'static),
) -> &'a (dyn std::error::Error + 'static) {
    let mut cause = error;
    while let Some(source) = std::error::Error::source(cause) {
        cause = source;
    }
    cause
}

/// The result type the CLI's I/O entry points return.
pub type CliResult<T> = std::result::Result<T, CliError>;

/// Why a command did not finish with a clean exit.
///
/// Two different things share the `Err` channel, and until this type existed
/// they shared it *type-erased*: a command that could not do its work, and a
/// command that did its work, printed its report, and then tripped a
/// `--fail-on-*` gate the caller explicitly asked for. The second is not a
/// malfunction — it is the answer — but `main` still has to exit non-zero for
/// it, so both travel as `Err`.
///
/// `diagnosis::classify` used to recover the difference by probing an
/// `anyhow::Error` with a chain of `downcast_ref`, ending in
/// `.map_or(ErrorCode::Internal, ...)`. That fallback is why this type exists:
/// a typed failure nobody had written a probe for was reported to the user as
/// an internal malfunction. It is the defect §9.2 set out to remove — a
/// decision re-derived from a value whose type had been thrown away — one
/// level up from the string matching that was removed first.
///
/// As a two-variant enum the classification becomes a total `match`, so a new
/// [`CliError`] variant cannot silently become `Internal`; the compiler asks.
/// The second effect is larger: `gate_failure` returning `anyhow::Error` was
/// the *only* reason ~400 `cli/workflow.rs` and `cli/render.rs` files in
/// `feature/*` still imported `anyhow`. Every other helper they call had
/// already been typed.
#[derive(Debug, Error)]
pub enum CommandFailure {
    /// The command could not do its work.
    #[error(transparent)]
    Error(#[from] CliError),

    /// The command did its work and a requested policy gate tripped.
    ///
    /// Kept separate from [`Self::Error`] rather than folded into `CliError`
    /// because the two answer different questions. A caller reading a gate
    /// failure as a malfunction would retry; a caller reading a malfunction as
    /// a gate would report violations that were never measured.
    #[error(transparent)]
    Gate(#[from] GateFailure),
}

impl CommandFailure {
    /// The message chain, joined the way `anyhow`'s `{:#}` joins it.
    ///
    /// See [`CliError::chain`] — same reason, at the outer type.
    #[must_use]
    pub fn chain(&self) -> String {
        error_chain(self)
    }

    /// The deepest error in the chain, as `anyhow::Error::root_cause` returned.
    #[must_use]
    pub fn root_cause(&self) -> &(dyn std::error::Error + 'static) {
        root_cause(self)
    }
}

/// Routes the error types [`CliError`] already absorbs on to [`CommandFailure`],
/// so a command body keeps using `?` on them unchanged.
///
/// Written out rather than as `impl<E: Into<CliError>> From<E>`: the blanket
/// form overlaps its own `From<CliError>` and would leave `?` inferring
/// nothing. Each line here is one type that used to reach `anyhow` implicitly.
macro_rules! command_failure_from {
    ($($source:ty),+ $(,)?) => {
        $(impl From<$source> for CommandFailure {
            fn from(source: $source) -> Self {
                Self::Error(CliError::from(source))
            }
        })+
    };
}

/// The syntax layer's sub-errors, each of which `SexprError` already wraps.
///
/// `From` does not chain, so `?` on a bare `PathError` needs the middle step
/// spelled out or every call site would write `SexprError::from(x).into()`.
macro_rules! cli_error_from_sexpr {
    ($($source:ty),+ $(,)?) => {
        $(impl From<$source> for CliError {
            fn from(source: $source) -> Self {
                Self::Sexpr(SexprError::from(source))
            }
        })+
    };
}

cli_error_from_sexpr!(
    paredit_core_syntax::sexpr::PathError,
    paredit_core_syntax::sexpr::SymbolError,
    paredit_core_syntax::sexpr::StructureError,
    paredit_core_syntax::sexpr::SelectionError,
);

command_failure_from!(
    std::io::Error,
    serde_json::Error,
    IoRefusal,
    SexprError,
    WorkspaceError,
    ArgumentError,
    SelectorError,
    CleanupFailure,
    WriteTargetError,
    paredit_core_lint_engine::error::LintError,
    paredit_core_lint_engine::error::RuleSelectionError,
    paredit_core_safety::deadline::TimeoutError,
);

/// The result type a command's `workflow` entry point returns.
///
/// `()` rather than a report value: the command has already written its output
/// by the time it returns, and what the caller needs back is only whether to
/// exit zero, and if not, which non-zero.
pub type CommandResult = std::result::Result<(), CommandFailure>;

#[cfg(test)]
mod tests {
    use super::*;

    /// `chain()` must render what `anyhow`'s `{:#}` rendered, because the
    /// tests that assert on write-path failures were written against it — and
    /// because that is still what a user sees, `main` converting at the edge.
    #[test]
    fn chain_flattens_the_source_chain_like_anyhow() {
        let error = CliError::Io {
            context: "failed to open or inspect /tmp/x".to_owned(),
            source: std::io::Error::other("refusing non-regular input file /tmp/x"),
        };

        assert_eq!(error.to_string(), "failed to open or inspect /tmp/x");
        assert_eq!(
            error.chain(),
            "failed to open or inspect /tmp/x: refusing non-regular input file /tmp/x"
        );
        assert_eq!(
            error.root_cause().to_string(),
            "refusing non-regular input file /tmp/x"
        );
    }

    /// A recovery failure keeps the failure that caused it reachable, rather
    /// than flattening both into one string. The rendering is unchanged - the
    /// cleanup message still leads - but the two are now separate values.
    #[test]
    fn a_cleanup_failure_keeps_the_write_failure_it_followed() {
        let write_failure = CliError::Io {
            context: "failed to write /tmp/x".to_owned(),
            source: std::io::Error::other("disk full"),
        };
        let error = write_failure.with_cleanup_failure("rollback/cleanup also failed: denied");

        assert_eq!(error.to_string(), "rollback/cleanup also failed: denied");
        let CliError::CleanupAlsoFailed(failure) = &error else {
            panic!("expected a cleanup failure, got {error:?}");
        };
        assert_eq!(failure.cause.to_string(), "failed to write /tmp/x");
        assert_eq!(error.root_cause().to_string(), "disk full");
    }

    /// The one failure that must not be read as "nothing happened".
    #[test]
    fn a_post_commit_cleanup_failure_is_its_own_variant() {
        let error = CliError::BackupCleanupAfterCommit {
            details: "denied".to_owned(),
        };
        assert_eq!(
            error.to_string(),
            "writes committed successfully, but backup cleanup failed: denied"
        );
        assert!(!matches!(error, CliError::Refused(_)));
    }
}
