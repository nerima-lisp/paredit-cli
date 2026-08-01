use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use crate::similarity_report::domain::{SimilarityReport, SimilarityReportOptions};
use paredit_core_syntax::dialect::Dialect;

#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityReportRequest {
    pub roots: Vec<PathBuf>,
    pub include_unknown: bool,
    pub include_hidden: bool,
    pub include_generated: bool,
    pub max_depth: Option<usize>,
    pub exclude: Vec<PathBuf>,
    pub forced_dialect: Option<Dialect>,
    pub options: SimilarityReportOptions,
    pub error_policy: SimilarityErrorPolicy,
    pub duplicate_policy: SimilarityDuplicatePolicy,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SimilarityInventory {
    pub files: Vec<DiscoveredSimilarityFile>,
    pub skipped_unknown_count: usize,
    pub skipped_hidden_count: usize,
    pub skipped_generated_count: usize,
    pub skipped_symlink_count: usize,
    pub skipped_excluded_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSimilarityFile {
    pub path: PathBuf,
    pub dialect: Dialect,
}

/// The outside world, as this use case needs it.
///
/// `discover` returned `anyhow::Result`, justified by the port's purpose: the
/// use case must not know what an adapter can fail with. The reasoning is
/// right; `anyhow::Error` was the wrong way to say it. It does not express
/// "some error I do not name" — it expresses "no error type at all", and the
/// classification went with it.
///
/// An associated type says the intended thing in the type system. The
/// `Into<CliError>` bound is the one requirement: whatever an adapter fails
/// with has to be reportable at the CLI boundary with a code.
pub trait SimilarityReportSourcePort: Sync {
    /// What this adapter's own failures look like.
    type Error: Into<paredit_core_cli::CliError>;

    fn discover(
        &mut self,
        request: &SimilarityReportRequest,
    ) -> Result<SimilarityInventory, Self::Error>;

    fn load(&self, file: &DiscoveredSimilarityFile) -> Result<Vec<u8>, String>;

    /// `--jobs` is what a caller sets to bound this run's worker count, and it
    /// only reaches here if this default reads it.
    fn available_parallelism(&self) -> NonZeroUsize {
        paredit_core_safety::limits::effective_jobs_or_available()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimilarityErrorPolicy {
    Fail,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimilarityDuplicatePolicy {
    Ignore,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimilarityProcessingStage {
    Read,
    Decode,
    Parse,
    Collect,
}

impl SimilarityProcessingStage {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Decode => "decode",
            Self::Parse => "parse",
            Self::Collect => "collect",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimilarityFileError {
    pub path: PathBuf,
    pub stage: SimilarityProcessingStage,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimilarityIndeterminateReason {
    ComparisonLimit { unprocessed_pairs: usize },
    CandidateLimit { omitted_candidates: usize },
    ProcessingErrors { file_count: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimilarityGateDecision {
    NotRequested,
    Passed,
    DuplicateFound { matched_pairs: usize },
    Indeterminate(SimilarityIndeterminateReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidSimilarityReportPlan {
    DuplicateInventoryPath {
        path: PathBuf,
    },
    UnknownErrorPath {
        path: PathBuf,
    },
    DuplicateErrorPath {
        path: PathBuf,
    },
    OutOfOrderError {
        previous_path: PathBuf,
        path: PathBuf,
    },
}

impl std::fmt::Display for InvalidSimilarityReportPlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateInventoryPath { path } => {
                write!(formatter, "duplicate inventory path: {}", path.display())
            }
            Self::UnknownErrorPath { path } => {
                write!(
                    formatter,
                    "error path is not in inventory: {}",
                    path.display()
                )
            }
            Self::DuplicateErrorPath { path } => {
                write!(formatter, "duplicate error path: {}", path.display())
            }
            Self::OutOfOrderError {
                previous_path,
                path,
            } => write!(
                formatter,
                "error paths are out of inventory order: {} before {}",
                previous_path.display(),
                path.display()
            ),
        }
    }
}

impl std::error::Error for InvalidSimilarityReportPlan {}

#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityReportPlan {
    report: SimilarityReport,
    inventory: SimilarityInventory,
    errors: Vec<SimilarityFileError>,
    gate: SimilarityGateDecision,
}

impl SimilarityReportPlan {
    pub fn new(
        report: SimilarityReport,
        inventory: SimilarityInventory,
        errors: Vec<SimilarityFileError>,
        duplicate_policy: SimilarityDuplicatePolicy,
    ) -> Result<Self, InvalidSimilarityReportPlan> {
        let mut inventory_positions = HashMap::with_capacity(inventory.files.len());
        for (position, file) in inventory.files.iter().enumerate() {
            if inventory_positions
                .insert(file.path.as_path(), position)
                .is_some()
            {
                return Err(InvalidSimilarityReportPlan::DuplicateInventoryPath {
                    path: file.path.clone(),
                });
            }
        }

        let mut seen_error_paths = HashSet::with_capacity(errors.len());
        let mut previous_error: Option<(usize, &Path)> = None;
        for error in &errors {
            let Some(&position) = inventory_positions.get(error.path.as_path()) else {
                return Err(InvalidSimilarityReportPlan::UnknownErrorPath {
                    path: error.path.clone(),
                });
            };
            if !seen_error_paths.insert(error.path.as_path()) {
                return Err(InvalidSimilarityReportPlan::DuplicateErrorPath {
                    path: error.path.clone(),
                });
            }
            if let Some((previous_position, previous_path)) = previous_error {
                if position <= previous_position {
                    return Err(InvalidSimilarityReportPlan::OutOfOrderError {
                        previous_path: previous_path.to_path_buf(),
                        path: error.path.clone(),
                    });
                }
            }
            previous_error = Some((position, error.path.as_path()));
        }

        let gate = evaluate_gate(duplicate_policy, &report, &errors);
        Ok(Self {
            report,
            inventory,
            errors,
            gate,
        })
    }

    #[must_use]
    pub const fn report(&self) -> &SimilarityReport {
        &self.report
    }

    #[must_use]
    pub const fn inventory(&self) -> &SimilarityInventory {
        &self.inventory
    }

    #[must_use]
    pub fn errors(&self) -> &[SimilarityFileError] {
        &self.errors
    }

    #[must_use]
    pub const fn gate(&self) -> &SimilarityGateDecision {
        &self.gate
    }
}

fn evaluate_gate(
    policy: SimilarityDuplicatePolicy,
    report: &SimilarityReport,
    errors: &[SimilarityFileError],
) -> SimilarityGateDecision {
    if policy == SimilarityDuplicatePolicy::Ignore {
        return SimilarityGateDecision::NotRequested;
    }
    if report.summary.matched_pairs() > 0 {
        return SimilarityGateDecision::DuplicateFound {
            matched_pairs: report.summary.matched_pairs(),
        };
    }
    if report.summary.comparison_limit_reached() {
        return SimilarityGateDecision::Indeterminate(
            SimilarityIndeterminateReason::ComparisonLimit {
                unprocessed_pairs: report.summary.unprocessed_pairs(),
            },
        );
    }
    if report.summary.candidate_limit_reached() {
        return SimilarityGateDecision::Indeterminate(
            SimilarityIndeterminateReason::CandidateLimit {
                omitted_candidates: report.summary.omitted_candidates(),
            },
        );
    }
    if !errors.is_empty() {
        return SimilarityGateDecision::Indeterminate(
            SimilarityIndeterminateReason::ProcessingErrors {
                file_count: errors.len(),
            },
        );
    }
    SimilarityGateDecision::Passed
}

#[derive(Debug)]
pub enum SimilarityReportWorkflowError {
    /// Whatever the source port's adapter failed with.
    ///
    /// The port still does not enumerate its adapters' failures — see
    /// [`SimilarityReportSourcePort::Error`] — but what arrives here is a
    /// `CliError` rather than an `anyhow::Error`, so it still carries a
    /// classification. Boxed only to keep this enum small.
    Source(Box<paredit_core_cli::CliError>),
    Processing(SimilarityFileError),
    WorkerPanicked,
    /// The analysis itself gave up — a resource budget, or an inconsistent
    /// report aggregate. Typed, because this use case *does* know what its own
    /// analysis can fail with.
    Analysis(crate::error::SimilarityAnalysisError),
    InvalidPlan(InvalidSimilarityReportPlan),
}

impl std::fmt::Display for SimilarityReportWorkflowError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(_) => formatter.write_str("similarity report source failed"),
            Self::Processing(error) => write!(
                formatter,
                "failed to {} {}: {}",
                error.stage.label(),
                error.path.display(),
                error.message
            ),
            Self::WorkerPanicked => formatter.write_str("similarity-report worker thread panicked"),
            Self::Analysis(_) => formatter.write_str("similarity report analysis failed"),
            Self::InvalidPlan(error) => {
                write!(formatter, "invalid similarity report plan: {error}")
            }
        }
    }
}

impl std::error::Error for SimilarityReportWorkflowError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error.as_ref()),
            Self::Analysis(error) => Some(error),
            Self::InvalidPlan(error) => Some(error),
            Self::Processing(_) | Self::WorkerPanicked => None,
        }
    }
}

#[cfg(test)]
mod workflow_error_tests {
    use std::error::Error as _;

    use super::SimilarityReportWorkflowError;

    #[test]
    fn source_display_adds_boundary_context_without_repeating_cause() {
        let error =
            SimilarityReportWorkflowError::Source(Box::new(paredit_core_cli::CliError::Io {
                context: "could not discover inputs".to_owned(),
                source: std::io::Error::other("filesystem unavailable"),
            }));

        assert_eq!(error.to_string(), "similarity report source failed");
        let source = error.source().expect("source error must be retained");
        assert_eq!(source.to_string(), "could not discover inputs");
        assert_eq!(
            source
                .source()
                .expect("root cause must be retained")
                .to_string(),
            "filesystem unavailable"
        );
    }

    #[test]
    fn analysis_display_adds_boundary_context_without_repeating_cause() {
        // Now typed: the budget that was exceeded is a value, not a message,
        // so a caller can offer "narrow the search" without parsing prose.
        let error = SimilarityReportWorkflowError::Analysis(
            crate::error::SimilarityBudgetError::Comparisons {
                comparisons: 200_000_000,
                limit: 100_000_000,
            }
            .into(),
        );

        assert_eq!(error.to_string(), "similarity report analysis failed");
        let source = error.source().expect("analysis error must be retained");
        assert_eq!(
            source.to_string(),
            "similarity comparison budget exceeded: 200000000 comparisons, limit 100000000"
        );

        // The point of typing this: the caller no longer needs the message at
        // all. It can ask whether the run was too large - and therefore worth
        // retrying with a narrower search - by matching the variant.
        let SimilarityReportWorkflowError::Analysis(analysis) = &error else {
            panic!("expected an analysis failure, got {error:?}");
        };
        assert!(matches!(
            analysis,
            crate::error::SimilarityAnalysisError::Budget(
                crate::error::SimilarityBudgetError::Comparisons { .. }
            )
        ));
    }
}
