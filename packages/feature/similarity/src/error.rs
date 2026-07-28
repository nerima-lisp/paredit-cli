//! Why a similarity analysis stops early.
//!
//! Section 9.2. This package's failures are almost all **resource budgets**,
//! which is a distinct kind worth naming: the analysis was not wrong and the
//! input was not malformed — the comparison space was too large to search
//! exhaustively, and stopping is the correct behaviour rather than an error in
//! the usual sense.
//!
//! A caller that can tell a budget from a defect can retry with a narrower
//! `--min-lines` or a smaller file set. Before this, it had to read the
//! message to know that.

use thiserror::Error;

use crate::form_similarity::TreeSimilarityError;

/// A bound on the size of a similarity run was exceeded.
///
/// Every variant is recoverable by asking for less.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SimilarityBudgetError {
    #[error("similarity candidate budget exceeded: {candidates} candidates, limit {limit}")]
    Candidates { candidates: usize, limit: usize },

    #[error("similarity comparison budget exceeded: {comparisons} comparisons, limit {limit}")]
    Comparisons { comparisons: usize, limit: usize },

    #[error("similarity result budget exceeded: more than {limit} retained matches")]
    Results { limit: usize },

    #[error(transparent)]
    TreeEdit(#[from] TreeSimilarityError),
}

/// Something went wrong inside the parallel comparison pass.
///
/// Separate from a budget because it is not recoverable by asking for less —
/// it means a worker died or the work assignment was inconsistent.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SimilarityWorkerError {
    #[error("similarity comparison worker thread panicked")]
    Panicked,

    #[error("similarity worker assignment has no available worker")]
    NoAvailableWorker,
}

/// Anything the similarity report's analysis can fail with.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SimilarityAnalysisError {
    #[error(transparent)]
    Budget(#[from] SimilarityBudgetError),

    #[error(transparent)]
    Worker(#[from] SimilarityWorkerError),

    /// The options the run was configured with are inconsistent. Raised
    /// before any comparison happens, so it is not a budget.
    #[error(transparent)]
    InvalidOptions(#[from] crate::similarity_report::domain::SimilarityReportOptionsError),

    /// A report aggregate would have been internally inconsistent — counts
    /// that do not add up. A defect in this package, not in the input, and
    /// distinct from a budget for exactly that reason.
    #[error(transparent)]
    InvalidReport(#[from] crate::similarity_report::domain::InvalidSimilarityReport),
}

impl From<TreeSimilarityError> for SimilarityAnalysisError {
    fn from(error: TreeSimilarityError) -> Self {
        Self::Budget(error.into())
    }
}

/// The result type the similarity analysis passes return.
pub type SimilarityAnalysisResult<T> = std::result::Result<T, SimilarityAnalysisError>;
