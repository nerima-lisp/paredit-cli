//! LeftoverFormatDebugMarker (a (format t ...) whose control string carries a DEBUG/DBG marker) detection.

pub use crate::leftover_format_debug_marker::domain::{
    LeftoverFormatDebugMarkerItem, LeftoverFormatDebugMarkerPolicy,
    LeftoverFormatDebugMarkerPolicyOptions, LeftoverFormatDebugMarkerSummary,
    collect_leftover_format_debug_marker, evaluate_leftover_format_debug_marker_policy,
    summarize_leftover_format_debug_marker,
};
