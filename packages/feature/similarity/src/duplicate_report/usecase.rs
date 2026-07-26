//! Backwards-compatible application facade for duplicate-form domain analysis.

pub use crate::domain::duplicate_report::{
    DuplicateCandidateAccumulator, DuplicateCandidateGroups, DuplicateFormReport,
    DuplicateShapeReport, ReplacementPlanBatch, build_duplicate_shape_reports,
    collect_duplicate_candidates, collect_replacement_plan_batches,
};
