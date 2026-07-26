use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::dialect::Dialect;
use crate::domain::form_similarity::StructuralTree;
use crate::domain::sexpr::{ByteOffset, ByteSpan, Path};

#[allow(unused_imports)]
pub use crate::domain::similarity_report::{
    SimilarityComparisonScope, SimilarityFormScope, SimilarityOverlapPolicy,
    SimilarityReportOptions, SimilarityReportOptionsError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedFormText {
    source: Arc<str>,
    span: ByteSpan,
}

impl SharedFormText {
    fn owned(text: String) -> Self {
        let len = text.len();
        Self {
            source: Arc::from(text),
            span: ByteSpan::new(ByteOffset::new(0), ByteOffset::new(len)),
        }
    }

    pub(crate) fn from_source(source: Arc<str>, span: ByteSpan) -> Self {
        Self { source, span }
    }

    pub(crate) fn source_identity(&self) -> *const str {
        Arc::as_ptr(&self.source)
    }

    pub(crate) fn source_len(&self) -> usize {
        self.source.len()
    }

    #[cfg(test)]
    pub(crate) fn shares_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source)
    }
}

impl Deref for SharedFormText {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.span.slice(&self.source)
    }
}

impl AsRef<str> for SharedFormText {
    fn as_ref(&self) -> &str {
        self
    }
}

impl PartialEq<str> for SharedFormText {
    fn eq(&self, other: &str) -> bool {
        self.as_ref() == other
    }
}

impl PartialEq<&str> for SharedFormText {
    fn eq(&self, other: &&str) -> bool {
        self.as_ref() == *other
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimilarityFormReport {
    path: PathBuf,
    dialect: Dialect,
    form_path: Path,
    span: ByteSpan,
    node_count: usize,
    head: Option<FormHead>,
    text: SharedFormText,
}

impl SimilarityFormReport {
    pub fn new(
        path: PathBuf,
        dialect: Dialect,
        form_path: Path,
        span: ByteSpan,
        node_count: usize,
        head: Option<FormHead>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            path,
            dialect,
            form_path,
            span,
            node_count,
            head,
            text: SharedFormText::owned(text.into()),
        }
    }

    pub(crate) fn new_shared(
        path: PathBuf,
        dialect: Dialect,
        form_path: Path,
        span: ByteSpan,
        node_count: usize,
        head: Option<FormHead>,
        source: Arc<str>,
    ) -> Self {
        Self {
            path,
            dialect,
            form_path,
            span,
            node_count,
            head,
            text: SharedFormText::from_source(source, span),
        }
    }

    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        self.path.as_path()
    }

    #[must_use]
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }

    #[must_use]
    pub const fn form_path(&self) -> &Path {
        &self.form_path
    }

    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    #[must_use]
    pub const fn node_count(&self) -> usize {
        self.node_count
    }

    #[must_use]
    pub fn head(&self) -> Option<&FormHead> {
        self.head.as_ref()
    }

    #[must_use]
    pub const fn text(&self) -> &SharedFormText {
        &self.text
    }

    #[must_use]
    pub fn contains_span(&self, other: &Self) -> bool {
        self.span.contains_span(other.span)
    }

    #[must_use]
    pub fn strictly_contains_span(&self, other: &Self) -> bool {
        self.contains_span(other) && self.span != other.span
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimilarityCandidate {
    form: Arc<SimilarityFormReport>,
    tree: StructuralTree,
    comparison_head: Option<ComparisonHead>,
}

impl SimilarityCandidate {
    pub fn new(
        form: impl Into<Arc<SimilarityFormReport>>,
        tree: StructuralTree,
        comparison_head: Option<ComparisonHead>,
    ) -> Self {
        Self {
            form: form.into(),
            tree,
            comparison_head,
        }
    }

    #[must_use]
    pub fn form(&self) -> &Arc<SimilarityFormReport> {
        &self.form
    }

    #[must_use]
    pub const fn tree(&self) -> &StructuralTree {
        &self.tree
    }

    #[must_use]
    pub fn comparison_head(&self) -> Option<&ComparisonHead> {
        self.comparison_head.as_ref()
    }

    #[must_use]
    pub fn same_comparison_bucket(&self, other: &Self) -> bool {
        self.comparison_head == other.comparison_head
    }

    #[must_use]
    pub fn cmp_comparison_bucket(&self, other: &Self) -> Ordering {
        self.comparison_head.cmp(&other.comparison_head)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct SimilarityRatio(f64);

impl SimilarityRatio {
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && (0.0..=1.0).contains(&value)).then_some(Self(value))
    }

    #[must_use]
    pub const fn as_f64(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for SimilarityRatio {
    type Error = InvalidSimilarityRatio;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InvalidSimilarityRatio)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSimilarityRatio;

impl std::fmt::Display for InvalidSimilarityRatio {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("similarity ratio must be finite and between 0 and 1")
    }
}

impl std::error::Error for InvalidSimilarityRatio {}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct SimilarityScore(f64);

impl SimilarityScore {
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value >= 0.0).then_some(Self(value))
    }

    #[must_use]
    pub const fn as_f64(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for SimilarityScore {
    type Error = InvalidSimilarityScore;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InvalidSimilarityScore)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSimilarityScore;

impl std::fmt::Display for InvalidSimilarityScore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("similarity score must be finite and non-negative")
    }
}

impl std::error::Error for InvalidSimilarityScore {}

#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityPairReport {
    similarity: SimilarityRatio,
    score: SimilarityScore,
    left: Arc<SimilarityFormReport>,
    right: Arc<SimilarityFormReport>,
}

impl SimilarityPairReport {
    #[must_use]
    pub fn new(
        similarity: SimilarityRatio,
        score: SimilarityScore,
        left: SimilarityFormReport,
        right: SimilarityFormReport,
    ) -> Self {
        Self {
            similarity,
            score,
            left: Arc::new(left),
            right: Arc::new(right),
        }
    }

    pub(crate) fn from_shared(
        similarity: SimilarityRatio,
        score: SimilarityScore,
        left: Arc<SimilarityFormReport>,
        right: Arc<SimilarityFormReport>,
    ) -> Self {
        Self {
            similarity,
            score,
            left,
            right,
        }
    }

    #[must_use]
    pub const fn similarity(&self) -> SimilarityRatio {
        self.similarity
    }

    #[must_use]
    pub const fn score(&self) -> SimilarityScore {
        self.score
    }

    #[must_use]
    pub fn left(&self) -> &SimilarityFormReport {
        &self.left
    }

    #[must_use]
    pub fn right(&self) -> &SimilarityFormReport {
        &self.right
    }

    #[must_use]
    pub fn strictly_contains_pair(&self, other: &Self) -> bool {
        strictly_contains_pair_forms(&self.left, &self.right, &other.left, &other.right)
    }
}

pub(super) fn strictly_contains_pair_forms(
    left_outer: &SimilarityFormReport,
    right_outer: &SimilarityFormReport,
    left_inner: &SimilarityFormReport,
    right_inner: &SimilarityFormReport,
) -> bool {
    left_outer.strictly_contains_span(left_inner) && right_outer.contains_span(right_inner)
        || left_outer.contains_span(left_inner) && right_outer.strictly_contains_span(right_inner)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComparisonHead(String);

impl ComparisonHead {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ComparisonHead {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for ComparisonHead {
    fn from(value: &str) -> Self {
        Self::new(value.to_owned())
    }
}

impl From<ComparisonHead> for String {
    fn from(value: ComparisonHead) -> Self {
        value.0
    }
}

impl AsRef<str> for ComparisonHead {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormHead(String);

impl FormHead {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for FormHead {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for FormHead {
    fn from(value: &str) -> Self {
        Self::new(value.to_owned())
    }
}

impl From<FormHead> for String {
    fn from(value: FormHead) -> Self {
        value.0
    }
}

impl AsRef<str> for FormHead {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for FormHead {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Display for FormHead {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportLimit {
    Complete,
    Limited(NonZeroUsize),
}

impl ReportLimit {
    pub fn from_omitted(count: usize) -> Self {
        NonZeroUsize::new(count).map_or(Self::Complete, Self::Limited)
    }

    #[must_use]
    pub fn reached(self) -> bool {
        matches!(self, Self::Limited(_))
    }

    #[must_use]
    pub fn omitted(self) -> usize {
        match self {
            Self::Complete => 0,
            Self::Limited(count) => count.get(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairProcessingCounts {
    possible: usize,
    evaluated: usize,
    pruned_by_size: usize,
    resource_skipped: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidSimilarityReport {
    ProcessingCountOverflow,
    ProcessedPairsExceedPossible { possible: usize, processed: usize },
    PairAccountingMismatch { possible: usize, accounted: usize },
    SuppressedPairsExceedMatched { matched: usize, suppressed: usize },
    ReportedPairsExceedAvailable { available: usize, reported: usize },
    ReportedPairCountMismatch { reported: usize, actual: usize },
}

impl Display for InvalidSimilarityReport {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProcessingCountOverflow => {
                formatter.write_str("similarity pair processing counts overflow")
            }
            Self::ProcessedPairsExceedPossible {
                possible,
                processed,
            } => write!(
                formatter,
                "processed similarity pairs ({processed}) exceed possible pairs ({possible})"
            ),
            Self::PairAccountingMismatch {
                possible,
                accounted,
            } => write!(
                formatter,
                "similarity pair accounting mismatch: possible={possible}, accounted={accounted}"
            ),
            Self::SuppressedPairsExceedMatched {
                matched,
                suppressed,
            } => write!(
                formatter,
                "suppressed similarity pairs ({suppressed}) exceed matched pairs ({matched})"
            ),
            Self::ReportedPairsExceedAvailable {
                available,
                reported,
            } => write!(
                formatter,
                "reported similarity pairs ({reported}) exceed available pairs ({available})"
            ),
            Self::ReportedPairCountMismatch { reported, actual } => write!(
                formatter,
                "reported similarity pair count ({reported}) does not match actual pairs ({actual})"
            ),
        }
    }
}

impl std::error::Error for InvalidSimilarityReport {}

impl PairProcessingCounts {
    pub fn new(
        possible: usize,
        evaluated: usize,
        pruned_by_size: usize,
        resource_skipped: usize,
    ) -> Result<Self, InvalidSimilarityReport> {
        let processed = evaluated
            .checked_add(pruned_by_size)
            .and_then(|count| count.checked_add(resource_skipped))
            .ok_or(InvalidSimilarityReport::ProcessingCountOverflow)?;
        if processed > possible {
            return Err(InvalidSimilarityReport::ProcessedPairsExceedPossible {
                possible,
                processed,
            });
        }
        Ok(Self {
            possible,
            evaluated,
            pruned_by_size,
            resource_skipped,
        })
    }

    fn validate_accounting(self, unprocessed: usize) -> Result<(), InvalidSimilarityReport> {
        let accounted = self
            .evaluated
            .checked_add(self.pruned_by_size)
            .and_then(|count| count.checked_add(self.resource_skipped))
            .and_then(|count| count.checked_add(unprocessed))
            .ok_or(InvalidSimilarityReport::ProcessingCountOverflow)?;
        if accounted != self.possible {
            return Err(InvalidSimilarityReport::PairAccountingMismatch {
                possible: self.possible,
                accounted,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairResultCounts {
    matched: usize,
    suppressed: usize,
    reported: usize,
}

impl PairResultCounts {
    pub fn new(
        matched: usize,
        suppressed: usize,
        reported: usize,
    ) -> Result<Self, InvalidSimilarityReport> {
        if suppressed > matched {
            return Err(InvalidSimilarityReport::SuppressedPairsExceedMatched {
                matched,
                suppressed,
            });
        }
        let available = matched - suppressed;
        if reported > available {
            return Err(InvalidSimilarityReport::ReportedPairsExceedAvailable {
                available,
                reported,
            });
        }
        Ok(Self {
            matched,
            suppressed,
            reported,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimilarityReportSummary {
    candidate_limit: ReportLimit,
    pair_processing: PairProcessingCounts,
    comparison_limit: ReportLimit,
    pair_results: PairResultCounts,
}

impl SimilarityReportSummary {
    pub fn new(
        candidate_limit: ReportLimit,
        pair_processing: PairProcessingCounts,
        comparison_limit: ReportLimit,
        pair_results: PairResultCounts,
    ) -> Result<Self, InvalidSimilarityReport> {
        pair_processing.validate_accounting(comparison_limit.omitted())?;
        Ok(Self {
            candidate_limit,
            pair_processing,
            comparison_limit,
            pair_results,
        })
    }

    #[must_use]
    pub fn candidate_limit_reached(&self) -> bool {
        self.candidate_limit.reached()
    }

    #[must_use]
    pub fn omitted_candidates(&self) -> usize {
        self.candidate_limit.omitted()
    }

    #[must_use]
    pub fn possible_pairs(&self) -> usize {
        self.pair_processing.possible
    }

    #[must_use]
    pub fn evaluated_pairs(&self) -> usize {
        self.pair_processing.evaluated
    }

    #[must_use]
    pub fn pruned_by_size(&self) -> usize {
        self.pair_processing.pruned_by_size
    }

    #[must_use]
    pub fn resource_skipped_pairs(&self) -> usize {
        self.pair_processing.resource_skipped
    }

    #[must_use]
    pub fn comparison_limit_reached(&self) -> bool {
        self.comparison_limit.reached()
    }

    #[must_use]
    pub fn unprocessed_pairs(&self) -> usize {
        self.comparison_limit.omitted()
    }

    #[must_use]
    pub fn matched_pairs(&self) -> usize {
        self.pair_results.matched
    }

    #[must_use]
    pub fn suppressed_pairs(&self) -> usize {
        self.pair_results.suppressed
    }

    #[must_use]
    pub fn reported_pairs(&self) -> usize {
        self.pair_results.reported
    }

    #[must_use]
    pub fn truncated(&self) -> bool {
        self.reported_pairs() < self.matched_pairs().saturating_sub(self.suppressed_pairs())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityReport {
    pub(crate) summary: SimilarityReportSummary,
    pub(crate) pairs: Vec<SimilarityPairReport>,
}

impl SimilarityReport {
    pub fn new(
        summary: SimilarityReportSummary,
        pairs: Vec<SimilarityPairReport>,
    ) -> Result<Self, InvalidSimilarityReport> {
        if summary.reported_pairs() != pairs.len() {
            return Err(InvalidSimilarityReport::ReportedPairCountMismatch {
                reported: summary.reported_pairs(),
                actual: pairs.len(),
            });
        }
        Ok(Self { summary, pairs })
    }

    #[must_use]
    pub const fn summary(&self) -> &SimilarityReportSummary {
        &self.summary
    }

    #[must_use]
    pub fn pairs(&self) -> &[SimilarityPairReport] {
        &self.pairs
    }
}
