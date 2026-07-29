//! Cross-file duplicate-slot-name detection across explicit files.

pub use crate::duplicate_slot_report::domain::{
    DuplicateSlotItem, DuplicateSlotPolicy, DuplicateSlotPolicyOptions, DuplicateSlotSummary,
    collect_duplicate_slots, evaluate_duplicate_slot_policy, summarize_duplicate_slots,
};
