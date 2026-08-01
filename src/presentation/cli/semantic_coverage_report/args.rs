use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::presentation::cli::{DialectArg, OutputFormat};
use crate::semantic_coverage::DialectCoverageThreshold;

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct SemanticCoverageReportArgs {
    /// Files or directories to measure.
    #[arg(required = true)]
    pub(in crate::presentation::cli::semantic_coverage_report) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::semantic_coverage_report) dialect: Option<DialectArg>,
    /// Exit with failure when the corpus-wide variable-binding resolution rate falls below this percentage (0-100).
    #[arg(long, value_name = "PERCENT")]
    pub(in crate::presentation::cli::semantic_coverage_report) fail_under: Option<f64>,
    /// Exit with failure when a specific dialect's own variable-binding resolution rate falls below this percentage
    /// (0-100). Repeatable, one per dialect: `DIALECT=PERCENT`, e.g. `--fail-under-dialect common-lisp=90
    /// --fail-under-dialect emacs-lisp=70`. Uses the same dialect spelling as `--dialect`. A dialect with no
    /// discovered files fails loudly, the same as an empty corpus does for `--fail-under`.
    #[arg(
        long = "fail-under-dialect",
        value_name = "DIALECT=PERCENT",
        value_parser = parse_dialect_threshold
    )]
    pub(in crate::presentation::cli::semantic_coverage_report) fail_under_dialect:
        Vec<DialectCoverageThreshold>,
    /// How many ranked opacity causes and uninitialized binders to list as suggested next operators to register.
    #[arg(long, default_value_t = 10)]
    pub(in crate::presentation::cli::semantic_coverage_report) top: usize,
    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::semantic_coverage_report) output: OutputFormat,
}

/// Parses one `--fail-under-dialect DIALECT=PERCENT` occurrence.
///
/// Delegates the percentage-range check to [`DialectCoverageThreshold::new`]
/// rather than duplicating it here, so this parser and the type's own
/// constructor reject the same malformed percentages the same way.
fn parse_dialect_threshold(raw: &str) -> Result<DialectCoverageThreshold, String> {
    let (dialect_text, percent_text) = raw.split_once('=').ok_or_else(|| {
        format!(
            "invalid --fail-under-dialect value {raw:?}: expected DIALECT=PERCENT, \
             e.g. common-lisp=90"
        )
    })?;
    let dialect = DialectArg::from_str(dialect_text, true).map_err(|_| {
        format!("invalid --fail-under-dialect dialect {dialect_text:?}: not a recognized dialect")
    })?;
    let percent: f64 = percent_text.parse().map_err(|_| {
        format!("invalid --fail-under-dialect percentage {percent_text:?}: not a number")
    })?;
    DialectCoverageThreshold::new(dialect.into(), percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialect_threshold_is_parsed_from_dialect_equals_percent() {
        let threshold =
            parse_dialect_threshold("common-lisp=90").expect("well-formed value parses");
        assert_eq!(
            threshold.dialect(),
            paredit_core_syntax::dialect::Dialect::CommonLisp
        );
        assert!((threshold.percent() - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_dialect_threshold_accepts_every_repeated_occurrence_independently() {
        let first = parse_dialect_threshold("common-lisp=90").expect("well-formed value parses");
        let second = parse_dialect_threshold("emacs-lisp=70").expect("well-formed value parses");
        assert_eq!(
            first.dialect(),
            paredit_core_syntax::dialect::Dialect::CommonLisp
        );
        assert_eq!(
            second.dialect(),
            paredit_core_syntax::dialect::Dialect::EmacsLisp
        );
    }

    #[test]
    fn a_dialect_threshold_without_an_equals_sign_is_rejected() {
        assert!(parse_dialect_threshold("common-lisp90").is_err());
    }

    #[test]
    fn a_dialect_threshold_with_an_unrecognized_dialect_is_rejected() {
        assert!(parse_dialect_threshold("not-a-dialect=90").is_err());
    }

    #[test]
    fn a_dialect_threshold_with_a_non_numeric_percentage_is_rejected() {
        assert!(parse_dialect_threshold("common-lisp=ninety").is_err());
    }

    #[test]
    fn a_dialect_threshold_with_an_out_of_range_percentage_is_rejected() {
        assert!(parse_dialect_threshold("common-lisp=150").is_err());
    }
}
