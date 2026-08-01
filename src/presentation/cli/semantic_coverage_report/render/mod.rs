use paredit_core_cli::CliResult;

use crate::presentation::cli::OutputFormat;
use crate::semantic_coverage::{
    DialectCoveragePolicyReport, SemanticCoveragePolicy, SemanticCoverageReport,
};

mod json;
mod text;

pub(in crate::presentation::cli) fn print_semantic_coverage_report(
    report: &SemanticCoverageReport,
    policy: &SemanticCoveragePolicy,
    dialect_policy: &DialectCoveragePolicyReport,
    top: usize,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            text::print_semantic_coverage_report(report, policy, dialect_policy, top);
        }
        OutputFormat::Json => {
            json::print_semantic_coverage_report(report, policy, dialect_policy, top)?;
        }
    }

    Ok(())
}

/// A percentage, or `0.0` for a `0/0` ratio — printed as `n/a` at the call
/// site instead of a bogus `0.0%` when the denominator is empty.
pub(super) fn percentage(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator > 0).then(|| (numerator as f64 / denominator as f64) * 100.0)
}
