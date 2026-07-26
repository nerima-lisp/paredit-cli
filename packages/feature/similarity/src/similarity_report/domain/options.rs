use anyhow::Result;
use std::str::FromStr;

use thiserror::Error;

pub const MAX_STORED_RESULTS: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimilarityOverlapPolicy {
    Maximal,
    All,
}

impl SimilarityOverlapPolicy {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Maximal => "maximal",
            Self::All => "all",
        }
    }
}

impl FromStr for SimilarityOverlapPolicy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "maximal" => Ok(Self::Maximal),
            "all" => Ok(Self::All),
            _ => Err(format!("unknown overlap policy: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimilarityComparisonScope {
    All,
    SameFile,
    CrossFile,
}

impl SimilarityComparisonScope {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::SameFile => "same-file",
            Self::CrossFile => "cross-file",
        }
    }
}

impl FromStr for SimilarityComparisonScope {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "all" => Ok(Self::All),
            "same-file" => Ok(Self::SameFile),
            "cross-file" => Ok(Self::CrossFile),
            _ => Err(format!("unknown comparison scope: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimilarityFormScope {
    All,
    TopLevel,
}

impl SimilarityFormScope {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::TopLevel => "top-level",
        }
    }
}

impl FromStr for SimilarityFormScope {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "all" => Ok(Self::All),
            "top-level" => Ok(Self::TopLevel),
            _ => Err(format!("unknown form scope: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityReportOptions {
    threshold: f64,
    min_node_count: usize,
    min_line_span: usize,
    comparison_scope: SimilarityComparisonScope,
    form_scope: SimilarityFormScope,
    overlap_policy: SimilarityOverlapPolicy,
    max_candidates: Option<usize>,
    max_comparisons: Option<usize>,
    max_results: Option<usize>,
}

impl Default for SimilarityReportOptions {
    fn default() -> Self {
        Self {
            threshold: 0.87,
            min_node_count: 4,
            min_line_span: 1,
            comparison_scope: SimilarityComparisonScope::All,
            form_scope: SimilarityFormScope::All,
            overlap_policy: SimilarityOverlapPolicy::Maximal,
            max_candidates: None,
            max_comparisons: None,
            max_results: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SimilarityReportOptionsError {
    #[error("--threshold must be between 0.0 and 1.0")]
    ThresholdOutOfRange,
    #[error("--min-node-count must be at least 2")]
    MinNodeCountTooSmall,
    #[error("--min-line-span must be at least 1")]
    MinLineSpanTooSmall,
    #[error("--max-candidates must be at least 1")]
    MaxCandidatesTooSmall,
    #[error("--max-comparisons must be at least 1")]
    MaxComparisonsTooSmall,
    #[error("--max-results must be at least 1")]
    MaxResultsTooSmall,
    #[error("--max-results must not exceed 1000000")]
    MaxResultsTooLarge,
}

impl SimilarityReportOptions {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        threshold: f64,
        min_node_count: usize,
        min_line_span: usize,
        comparison_scope: SimilarityComparisonScope,
        form_scope: SimilarityFormScope,
        overlap_policy: SimilarityOverlapPolicy,
        max_candidates: Option<usize>,
        max_comparisons: Option<usize>,
        max_results: Option<usize>,
    ) -> Result<Self, SimilarityReportOptionsError> {
        let options = Self {
            threshold,
            min_node_count,
            min_line_span,
            comparison_scope,
            form_scope,
            overlap_policy,
            max_candidates,
            max_comparisons,
            max_results,
        };
        options.validate()?;
        Ok(options)
    }

    #[must_use]
    pub const fn threshold(&self) -> f64 {
        self.threshold
    }

    #[must_use]
    pub const fn min_node_count(&self) -> usize {
        self.min_node_count
    }

    #[must_use]
    pub const fn min_line_span(&self) -> usize {
        self.min_line_span
    }

    #[must_use]
    pub const fn comparison_scope(&self) -> SimilarityComparisonScope {
        self.comparison_scope
    }

    #[must_use]
    pub const fn form_scope(&self) -> SimilarityFormScope {
        self.form_scope
    }

    #[must_use]
    pub const fn overlap_policy(&self) -> SimilarityOverlapPolicy {
        self.overlap_policy
    }

    #[must_use]
    pub const fn max_candidates(&self) -> Option<usize> {
        self.max_candidates
    }

    #[must_use]
    pub const fn max_comparisons(&self) -> Option<usize> {
        self.max_comparisons
    }

    #[must_use]
    pub const fn max_results(&self) -> Option<usize> {
        self.max_results
    }

    pub fn validate(&self) -> Result<(), SimilarityReportOptionsError> {
        if !(0.0..=1.0).contains(&self.threshold) {
            return Err(SimilarityReportOptionsError::ThresholdOutOfRange);
        }
        if self.min_node_count < 2 {
            return Err(SimilarityReportOptionsError::MinNodeCountTooSmall);
        }
        if self.min_line_span == 0 {
            return Err(SimilarityReportOptionsError::MinLineSpanTooSmall);
        }
        if self.max_candidates == Some(0) {
            return Err(SimilarityReportOptionsError::MaxCandidatesTooSmall);
        }
        if self.max_comparisons == Some(0) {
            return Err(SimilarityReportOptionsError::MaxComparisonsTooSmall);
        }
        if self.max_results == Some(0) {
            return Err(SimilarityReportOptionsError::MaxResultsTooSmall);
        }
        if self
            .max_results
            .is_some_and(|limit| limit > MAX_STORED_RESULTS)
        {
            return Err(SimilarityReportOptionsError::MaxResultsTooLarge);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    use crate::similarity_report::domain::{
        SimilarityCandidateCollectionError, collect_similarity_candidates,
    };

    #[test]
    fn default_options_validate() {
        SimilarityReportOptions::default().validate().unwrap();
    }

    #[test]
    fn constructor_rejects_invalid_values() {
        assert_eq!(
            SimilarityReportOptions::new(
                1.1,
                4,
                1,
                SimilarityComparisonScope::All,
                SimilarityFormScope::All,
                SimilarityOverlapPolicy::Maximal,
                None,
                None,
                None,
            ),
            Err(SimilarityReportOptionsError::ThresholdOutOfRange)
        );
    }

    #[test]
    fn reject_invalid_threshold() {
        let options = SimilarityReportOptions {
            threshold: 1.1,
            ..SimilarityReportOptions::default()
        };

        assert_eq!(
            options.validate(),
            Err(SimilarityReportOptionsError::ThresholdOutOfRange)
        );
    }

    #[test]
    fn reject_zero_result_limit() {
        let options = SimilarityReportOptions {
            max_results: Some(0),
            ..SimilarityReportOptions::default()
        };

        assert_eq!(
            options.validate(),
            Err(SimilarityReportOptionsError::MaxResultsTooSmall)
        );
    }

    #[test]
    fn reject_result_limit_above_retained_result_budget() {
        let options = SimilarityReportOptions {
            max_results: Some(MAX_STORED_RESULTS + 1),
            ..SimilarityReportOptions::default()
        };

        assert_eq!(
            options.validate(),
            Err(SimilarityReportOptionsError::MaxResultsTooLarge)
        );
    }

    #[test]
    fn candidate_collection_preserves_invalid_options_error_variant() {
        let options = SimilarityReportOptions {
            threshold: 1.1,
            ..SimilarityReportOptions::default()
        };
        let input = "(list value)";
        let tree = SyntaxTree::parse(input).expect("test input parses");
        let mut candidates = Vec::new();

        let error = collect_similarity_candidates(
            &tree,
            input,
            Path::new("input.lisp"),
            Dialect::CommonLisp,
            &options,
            &mut candidates,
        )
        .expect_err("invalid options must prevent collection");

        assert!(matches!(
            error,
            SimilarityCandidateCollectionError::InvalidOptions(
                SimilarityReportOptionsError::ThresholdOutOfRange
            )
        ));
    }
}
