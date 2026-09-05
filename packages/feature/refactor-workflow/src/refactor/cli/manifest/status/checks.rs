//! What a manifest was checked for, as one named value.
//!
//! Named inputs prevent policy flags and finding counts from being transposed
//! when deciding whether a refactor may be applied.

/// Whether the manifest's own policy gate passed.
///
/// A two-value enum rather than a `bool` because it sat next to another
/// `bool` in the parameter list: `Passed`/`Failed` cannot be transposed with
/// `OutputsParse` the way `true`/`true` can.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestPolicy {
    Passed,
    Failed,
}

impl ManifestPolicy {
    #[must_use]
    pub const fn passed(self) -> bool {
        matches!(self, Self::Passed)
    }

    #[must_use]
    pub const fn from_passed(passed: bool) -> Self {
        if passed { Self::Passed } else { Self::Failed }
    }
}

/// Whether every output the manifest recorded still parses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestOutputs {
    Parse,
    DoNotParse,
}

impl ManifestOutputs {
    #[must_use]
    pub const fn parse(self) -> bool {
        matches!(self, Self::Parse)
    }

    #[must_use]
    pub const fn from_parse(parse: bool) -> Self {
        if parse { Self::Parse } else { Self::DoNotParse }
    }
}

/// The per-file mismatch counts a manifest check produced.
///
/// Four `usize` fields that were four positional `usize` arguments. Named
/// fields are the whole point: the compiler still cannot tell them apart, but
/// a reader and a reviewer now can, and a struct literal names each one at the
/// call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RefactorManifestMismatchCounts {
    pub stale_files: usize,
    pub output_hash_mismatches: usize,
    pub parse_errors: usize,
    pub manifest_flag_mismatches: usize,
}

/// Everything the manifest decision is made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorManifestChecks {
    pub policy: ManifestPolicy,
    pub outputs: ManifestOutputs,
    pub counts: RefactorManifestMismatchCounts,
}

/// Whether the apply actually wrote anything.
///
/// Its own type because it was the seventh positional argument to
/// `refactor_apply_decision`, after six others, where `true` said nothing
/// about which question it answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorApplyOutcome {
    Applied,
    NotApplied,
}

impl RefactorApplyOutcome {
    #[must_use]
    pub const fn applied(self) -> bool {
        matches!(self, Self::Applied)
    }

    #[must_use]
    pub const fn from_applied(applied: bool) -> Self {
        if applied {
            Self::Applied
        } else {
            Self::NotApplied
        }
    }
}
