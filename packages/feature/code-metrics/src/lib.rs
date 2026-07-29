#![doc = include_str!("../README.md")]

pub mod cohesion_report;
pub mod debt_score_report;
pub mod docstring_report;
pub mod duplication_ratio_report;
pub mod hotspot_report;
pub mod indentation_report;
pub mod line_metrics_report;
pub mod todo_report;

// The composition root sees each slice's Args type and run fn (section 4.2).
pub use cohesion_report::cli::{CohesionReportArgs, cohesion_report};
pub use debt_score_report::cli::{DebtScoreReportArgs, debt_score_report};
pub use docstring_report::cli::{DocstringReportArgs, docstring_report};
pub use duplication_ratio_report::cli::{DuplicationRatioReportArgs, duplication_ratio_report};
pub use hotspot_report::cli::{HotspotReportArgs, hotspot_report};
pub use indentation_report::cli::{IndentationReportArgs, indentation_report};
pub use line_metrics_report::cli::{LineMetricsReportArgs, line_metrics_report};
pub use todo_report::cli::{TodoReportArgs, todo_report};
