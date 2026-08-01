mod classify;
mod collect;
mod options;
mod reports;
mod types;

pub use classify::{classify_form_pair, structural_tree_of};
pub use collect::{SimilarityCandidateCollectionError, collect_similarity_candidates};
pub use options::{
    SimilarityComparisonScope, SimilarityFormScope, SimilarityOverlapPolicy,
    SimilarityReportOptions, SimilarityReportOptionsError,
};
pub use reports::{build_similarity_pairs, build_similarity_pairs_with_omissions};
pub use types::{
    // `FormHead` is in `SimilarityFormReport::new`'s signature, so the
    // constructor was not actually callable from outside this module without
    // it.
    FormHead,
    InvalidSimilarityRatio,
    InvalidSimilarityReport,
    InvalidSimilarityScore,
    PairProcessingCounts,
    PairResultCounts,
    ReportLimit,
    SharedFormText,
    SimilarityCandidate,
    SimilarityFormReport,
    SimilarityPairReport,
    SimilarityRatio,
    SimilarityReport,
    SimilarityReportSummary,
    SimilarityScore,
};

#[cfg(test)]
mod tests;
