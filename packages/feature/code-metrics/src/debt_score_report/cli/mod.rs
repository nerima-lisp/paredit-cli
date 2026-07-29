pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::DebtScoreReportArgs;
pub use workflow::debt_score_report;
