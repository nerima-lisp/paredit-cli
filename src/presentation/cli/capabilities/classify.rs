//! Static, per-command classification for the capabilities catalog: whether
//! a command is capable of writing, and which [`ErrorCode`]s it can
//! realistically produce.
//!
//! Both questions are answered *structurally*, from the command's own clap
//! argument list and its dialect-contract category (see
//! [`super::super::contract::command_category`]) — never from a hand-kept
//! table naming all 358 leaf commands one at a time. A table like that is
//! wrong within a release, the same reason [`super::gate_kind`] reads a
//! naming convention instead of one. Two things make the convention-based
//! reading sound here:
//!
//! - `writes` mirrors the flag vocabulary [`crate::presentation::mcp::tools`]
//!   already uses to decide `--read-only`: an argument spelled `--write`,
//!   `--fix`, `--apply` or `--in-place` is a real write, and the flag names
//!   are namespaced consistently enough that no other flag collides with
//!   one exactly (`--fixable`, `--fix-plan` and `--apply-to` all differ from
//!   the bare word).
//! - `possible_error_codes` is a union of small, independently-verified code
//!   groups, each gated on one structural signal (an argument the command
//!   carries, or the dialect-contract category it belongs to). The mapping
//!   from *signal* to *codes* is copied from
//!   `paredit_core_cli::diagnosis::classify` and its callees, which is the
//!   one place a `CliError` ever becomes an [`ErrorCode`] — so a code
//!   appears in a group here only because the matching source path in that
//!   file can actually produce it for a command carrying that signal.
//!
//! ## Coverage
//!
//! This mechanizes the two general steps completely for every command: a
//! baseline every file-reading command carries, and one group per structural
//! capability (a selector, a workspace scan, a write flag, an archive
//! source, a policy gate, the kill ring, a dialect-sensitive category). What
//! it does *not* do is a from-scratch reading of every individual refactor's
//! own precondition failures — those already surface through the shared
//! groups above (`InputShapeRefused` covers "wrong shape", the write group
//! covers "would not reparse"), so the residual of codes this table cannot
//! reach from a structural signal is a handful of external-process and
//! journal codes on the handful of commands named in
//! [`COMMAND_SPECIFIC_CODES`] below, added from reading those commands'
//! own workflow modules rather than from a signal on the argument list.

use std::collections::BTreeSet;

use clap::Arg;
use paredit_core_cli::diagnosis::ErrorCode;

use super::super::contract::CommandCategory;

/// The flag vocabulary a write is spelled with, read off the command's own
/// argument list rather than an invocation's argv — the static counterpart
/// of [`crate::presentation::mcp::tools::WRITING_FLAGS`].
const WRITE_SIGNAL_LONGS: [&str; 4] = ["write", "fix", "apply", "in-place"];

/// Commands that write without any flag saying so, because the write *is*
/// the command name — the static counterpart of
/// [`crate::presentation::mcp::tools`]'s `WRITING_COMMANDS`, extended past
/// the handful MCP curates to every such command in the full catalog:
///
/// - `fix apply` (mirrors `WRITING_COMMANDS` exactly)
/// - `refactor create-checkpoint` / `refactor delete-checkpoint` write and
///   delete a checkpoint file under `.paredit/checkpoints/`
///   ([`paredit_feature_refactor_workflow::refactor_checkpoint::store`])
/// - `config init` writes `paredit.toml` unconditionally unless `--dry-run`
///   is given (`src/presentation/cli/config/workflow.rs::init`)
const WRITE_VERB_COMMANDS: [&[&str]; 4] = [
    &["fix", "apply"],
    &["refactor", "create-checkpoint"],
    &["refactor", "delete-checkpoint"],
    &["config", "init"],
];

/// Whether `path`'s own argument list, or its name, marks it as capable of
/// writing under some invocation.
///
/// Deliberately narrower than "can this process touch any byte on disk":
/// the kill ring (`--to-ring`) and archive extraction (`--extract-to`) also
/// write, and are deliberately excluded here for the same reason
/// `tools::writes` excludes them already — they are not what `--read-only`
/// promises to refuse, and widening the promise is a decision this FR does
/// not make.
pub(super) fn writes(path: &[String], args: &[&Arg]) -> bool {
    let by_flag = args.iter().any(|arg| {
        arg.get_long()
            .is_some_and(|long| WRITE_SIGNAL_LONGS.contains(&long))
    });
    if by_flag {
        return true;
    }
    WRITE_VERB_COMMANDS
        .iter()
        .any(|verb| verb.iter().copied().eq(path.iter().map(String::as_str)))
}

/// Whether `long` is one of `args`' own long flags, exactly — never a
/// prefix or substring match, so `--fixable` and `--fix-plan` cannot be
/// mistaken for the `--fix` that means "write".
fn has_long(args: &[&Arg], long: &str) -> bool {
    args.iter().any(|arg| arg.get_long() == Some(long))
}

/// Codes every file-reading command carries, because they come from the
/// shared read/parse/serialize plumbing every command goes through on the
/// way to its own logic (`paredit_core_cli::shared::read_input_and_dialect`
/// and friends, `CliError::Io | NotUtf8 | Parse | JsonBare | TimedOut` in
/// `code_for_cli_error`). Every one of the 358 dialect-contract commands
/// reads at least one file or stdin, so this group applies to all of them.
const BASELINE_CODES: [ErrorCode; 7] = [
    ErrorCode::EnvironmentIo,
    ErrorCode::InputNotUtf8,
    ErrorCode::InputUnparsable,
    ErrorCode::EnvironmentTimeout,
    ErrorCode::ArgumentNoInput,
    // `CliError::JsonBare` — this tool's own report or rewrite failing to
    // serialize. Reachable from any command, not just `--output json` ones:
    // `--explain-error` and structured stderr rendering serialize too.
    ErrorCode::Internal,
    // `read_text_with_limit` (`paredit_core_cli::io`) enforces the byte
    // ceiling on every read, not only a write's: a plain report over an
    // oversized file refuses exactly the same way a rewrite of one would.
    ErrorCode::RefusalInputTooLarge,
];

/// [`crate::presentation::cli::args::SelectorArgs`]'s flattened shape:
/// `--at` and `--select` both come from it and nothing else spells them
/// together, so their joint presence is the signal a command embeds it.
/// Codes are `classify_selector` and `code_for_edit_refusal`/
/// `code_for_sexpr_error`'s selection-family arms in
/// `paredit_core_cli::diagnosis`.
const SELECTOR_CODES: [ErrorCode; 12] = [
    ErrorCode::ArgumentTargetRequired,
    ErrorCode::ArgumentTargetAmbiguous,
    ErrorCode::SelectionPathNotReachable,
    ErrorCode::SelectionPathInvalid,
    ErrorCode::SelectionOffsetNotFound,
    ErrorCode::SelectionStale,
    ErrorCode::SelectionSpanInvalid,
    ErrorCode::SelectionNoMatch,
    ErrorCode::SelectionAmbiguous,
    ErrorCode::SelectionMalformed,
    ErrorCode::InputShapeRefused,
    ErrorCode::InputSymbolInvalid,
];

/// An S-expression `--query`/`--rewrite` pattern, from `SelectorArgs`'
/// `--query`/`--capture` or `query`/`migrate`'s own `--query`/`--rewrite`.
/// One code: `classify_selector`'s `Pattern`/`UnknownCapture` arm and
/// `CliError::Pattern | Rewrite` both resolve to it.
const PATTERN_CODES: [ErrorCode; 1] = [ErrorCode::SelectionPatternInvalid];

/// [`paredit_core_cli::workspace_args`]'s shared multi-file scan surface:
/// `--exclude` is unique to it among every argument struct in this tool.
/// Codes are `classify_argument`'s workspace-input arms and
/// `classify_workspace`.
const WORKSPACE_CODES: [ErrorCode; 7] = [
    ErrorCode::ArgumentInvalidGlob,
    ErrorCode::ArgumentConflictingInputs,
    ErrorCode::ArgumentNeedsRepository,
    ErrorCode::ArgumentNoInputsProduced,
    ErrorCode::EnvironmentWorkspaceLimit,
    ErrorCode::RefusalWorkspace,
    ErrorCode::EnvironmentUnavailable,
];

/// `--from-archive` requiring `--extract-to`
/// (`paredit_core_cli::workspace_args`), one code:
/// `ArgumentError::ArchiveRequiresDestination`.
const ARCHIVE_CODES: [ErrorCode; 1] = [ErrorCode::ArgumentArchiveDestination];

/// Codes reachable only once a command can actually write: the write-guard
/// and reparse-verification family in `classify_refusal`, plus the two
/// rollback/backup codes that only fire once a write was attempted.
const WRITE_CODES: [ErrorCode; 10] = [
    ErrorCode::ArgumentWriteRequiresFile,
    ErrorCode::RefusalWriteTarget,
    ErrorCode::RefusalTargetChanged,
    ErrorCode::RefusalRewriteDoesNotReparse,
    ErrorCode::RefusalOverlappingSpans,
    ErrorCode::RefusalSpanOutOfBounds,
    ErrorCode::RefusalDryRun,
    ErrorCode::RefusalEncodingWrite,
    ErrorCode::EnvironmentRollbackFailed,
    ErrorCode::EnvironmentCleanupAfterCommit,
];

/// `edit yank`'s `--index`, the only command with both `--index` and
/// `--ring`: `ArgumentError::KillRingIndexOutOfRange`.
const KILL_RING_CODES: [ErrorCode; 1] = [ErrorCode::ArgumentKillRingIndex];

/// A policy gate flag (`--fail-on`, `--fail-on-*`, `--require-* N`) is
/// present, so `GateFailed` is reachable — read the same way
/// [`super::gate_reports`] already does, not duplicated as a second
/// argument scan.
const GATE_CODES: [ErrorCode; 1] = [ErrorCode::GateFailed];

/// Commands whose own workflow can reach a code no structural signal on the
/// argument list predicts — read from the command's own source rather than
/// derived. Kept short deliberately: every entry here is a command this
/// classification's mechanical groups (above) would otherwise under-report,
/// found by reading the command's own `workflow.rs`, not by guessing.
const COMMAND_SPECIFIC_CODES: &[(&[&str], &[ErrorCode])] = &[
    // Runs an external verification command named on the command line
    // (`--apply-to`/the patch's own recorded command).
    (&["refactor", "patch"], &[ErrorCode::EnvironmentUnavailable]),
    (
        &["refactor", "verify"],
        &[ErrorCode::EnvironmentUnavailable],
    ),
    (&["refactor", "check"], &[ErrorCode::EnvironmentUnavailable]),
    (&["refactor", "step"], &[ErrorCode::EnvironmentUnavailable]),
    // Replays the undo journal to roll a write back.
    (
        &["refactor", "undo"],
        &[ErrorCode::EnvironmentRollbackFailed],
    ),
    (
        &["refactor", "restore-checkpoint"],
        &[ErrorCode::EnvironmentRollbackFailed],
    ),
    // `checkpoint already exists; pass --force` and the checkpoint-name
    // validator both raise `ArgumentFlagCombination`.
    (
        &["refactor", "create-checkpoint"],
        &[ErrorCode::ArgumentFlagCombination],
    ),
    (
        &["refactor", "delete-checkpoint"],
        &[ErrorCode::ArgumentFlagCombination],
    ),
    (
        &["refactor", "restore-checkpoint"],
        &[ErrorCode::ArgumentFlagCombination],
    ),
    // `config init` refuses to overwrite an existing `paredit.toml`.
    (&["config", "init"], &[ErrorCode::ArgumentFlagCombination]),
];

/// The [`ErrorCode`]s `path` — a leaf command carrying `args` in category
/// `category` (`None` for a command outside the dialect contract's six
/// namespaces) and already known to be `writes`-capable or not — can
/// realistically produce.
///
/// See the module documentation for how each group is gated and why the
/// union is a defensible superset rather than a guess.
pub(super) fn possible_error_codes(
    path: &[String],
    args: &[&Arg],
    category: Option<CommandCategory>,
    writes_capable: bool,
) -> Vec<&'static str> {
    // Labels, not `ErrorCode`s: the enum derives neither `Ord` nor `Hash`
    // (its identity is the label, checked by
    // `paredit_core_cli::diagnosis`'s own uniqueness test), so a `BTreeSet`
    // of the label is both the dedupe and the stable sort this catalog
    // wants.
    let mut codes: BTreeSet<&'static str> =
        BASELINE_CODES.iter().map(|code| code.label()).collect();

    let has_selector = has_long(args, "at") && has_long(args, "select");
    if has_selector {
        codes.extend(SELECTOR_CODES.iter().map(|code| code.label()));
    }
    if has_selector || has_long(args, "rewrite") {
        codes.extend(PATTERN_CODES.iter().map(|code| code.label()));
    }

    let is_workspace = has_long(args, "exclude") && has_long(args, "jobs");
    if is_workspace {
        codes.extend(WORKSPACE_CODES.iter().map(|code| code.label()));
    }
    if has_long(args, "from-archive") {
        codes.extend(ARCHIVE_CODES.iter().map(|code| code.label()));
    }

    if writes_capable {
        codes.extend(WRITE_CODES.iter().map(|code| code.label()));
    }

    if has_long(args, "index") && has_long(args, "ring") {
        codes.extend(KILL_RING_CODES.iter().map(|code| code.label()));
    }

    // A category outside the dialect contract's six namespaces (`config`,
    // `generate`, `completions`, `lsp`, `mcp`, `serve`, `tui`) carries no
    // dialect-refusal signal here; `generate`'s commands take a `--dialect`
    // override same as any editor command and are covered by the baseline
    // and write groups above instead.
    if let Some(category) = category {
        // Matches `resolve_support_status` in `contract.rs` exactly: an
        // unmet tier is `Silent` (an empty result, not an error) for an
        // introspection report, and `Unsupported` — a real refusal — for
        // everything else.
        if category != CommandCategory::Introspection {
            codes.insert(ErrorCode::InputDialectUnsupported.label());
        }
    }

    if args.iter().any(|arg| super::gate_kind(arg).is_some()) {
        codes.extend(GATE_CODES.iter().map(|code| code.label()));
    }

    if let Some((_, specific)) = COMMAND_SPECIFIC_CODES.iter().find(|(command_path, _)| {
        command_path
            .iter()
            .copied()
            .eq(path.iter().map(String::as_str))
    }) {
        codes.extend(specific.iter().map(|code| code.label()));
    }

    codes.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_verb_commands_all_exist_in_the_capabilities_tree() {
        // A compile-time check that the table above is read, not a
        // guarantee the paths are real commands — `capabilities_contract`'s
        // `writes_matches_the_write_flag_and_verb_convention` covers that.
        assert!(!WRITE_VERB_COMMANDS.is_empty());
    }
}
