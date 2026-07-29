mod calibration;
mod class;
mod external;
mod genealogy;
mod sequence;
mod shared;

pub use crate::similarity_report::domain::{classify_form_pair, structural_tree_of};
pub use calibration::{
    CloneThresholdOptions, CloneThresholdReport, DEFAULT_BUCKET_WIDTH, DEFAULT_CALIBRATION_FLOOR,
    DEFAULT_MIN_GAP_BUCKETS, DEFAULT_MIN_SAMPLE, SimilarityHistogram, SimilarityHistogramBucket,
    ThresholdCandidate, ThresholdMethod, build_clone_threshold_report,
};
pub use class::{
    ClassOverlapPolicy, CloneClass, CloneClassOptions, CloneClassReport,
    DEFAULT_HELPER_OVERHEAD_LINES, ExtractionEstimate, MemberSize, build_clone_class_report,
};
pub use external::{
    CloneExternalMatch, CloneExternalReport, ExternalCorpusStats, build_clone_external_report,
};
pub use genealogy::{
    CloneGenealogy, CloneGenealogyMember, CloneGenealogyReport, CloneHistoryPort, CloneOrigin,
    MemberHistory, build_clone_genealogy_report,
};
pub use sequence::{
    CloneSequenceGroup, CloneSequenceOccurrence, CloneSequenceOptions, CloneSequenceOptionsError,
    CloneSequenceReport, MAX_SUPPORTED_RUN_LENGTH, SequenceMatchMode, SequenceOverlapPolicy,
    SequenceSource, build_clone_sequence_report,
};
pub use shared::{FormKey, line_span_of};

#[cfg(test)]
mod tests;
