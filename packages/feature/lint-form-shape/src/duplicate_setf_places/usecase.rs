//! Duplicate-setf-place (`(setf a 1 a 2)` — a variable assigned twice in one
//! form) detection across explicit files.

pub use crate::duplicate_setf_places::domain::{
    DuplicateSetfPlaceItem, DuplicateSetfPlacePolicy, DuplicateSetfPlacePolicyOptions,
    DuplicateSetfPlaceSummary, collect_duplicate_setf_places, evaluate_duplicate_setf_place_policy,
    summarize_duplicate_setf_places,
};
