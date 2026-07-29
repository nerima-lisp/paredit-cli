//! Picking `--threshold` from the project instead of from a default.
//!
//! 0.87 is a number someone chose once. It is a reasonable number, and it is
//! also wrong for any codebase whose forms are unusually small (where one edit
//! moves the ratio a long way) or unusually formulaic (where nearly everything
//! scores high and the report drowns). Neither case is exotic and neither is
//! visible without measuring.
//!
//! So measure. Score every pair above a low floor, histogram the scores, and
//! find the split. The recommendation is Otsu's method — the threshold that
//! maximises the variance *between* the two groups it creates — because that is
//! precisely the question being asked: where does "these are the same shape"
//! stop and "these merely both exist" begin? A distribution with a real valley
//! gets a threshold in the valley. A distribution without one gets a
//! recommendation that the report labels as weakly supported, rather than a
//! confident number nothing supports.

use crate::error::SimilarityAnalysisResult;
use crate::similarity_report::domain::{
    SimilarityCandidate, SimilarityReportOptions, build_similarity_pairs_with_omissions,
};

/// Below this a "similar" pair is not evidence of anything, and including the
/// long tail of near-zero scores drags every statistic toward it.
pub const DEFAULT_CALIBRATION_FLOOR: f64 = 0.50;
pub const DEFAULT_BUCKET_WIDTH: f64 = 0.01;
/// Fewer scored pairs than this and the shape of the distribution is noise.
pub const DEFAULT_MIN_SAMPLE: usize = 20;
/// An empty stretch narrower than this is sampling noise, not a population
/// boundary. Three buckets at the default width is three percentage points of
/// similarity with nothing in it.
pub const DEFAULT_MIN_GAP_BUCKETS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloneThresholdOptions {
    pub floor: f64,
    pub bucket_width: f64,
    pub min_sample: usize,
    pub min_gap_buckets: usize,
    pub default_threshold: f64,
}

impl Default for CloneThresholdOptions {
    fn default() -> Self {
        Self {
            floor: DEFAULT_CALIBRATION_FLOOR,
            bucket_width: DEFAULT_BUCKET_WIDTH,
            min_sample: DEFAULT_MIN_SAMPLE,
            min_gap_buckets: DEFAULT_MIN_GAP_BUCKETS,
            default_threshold: 0.87,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimilarityHistogramBucket {
    pub lower: f64,
    pub upper: f64,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityHistogram {
    pub floor: f64,
    pub bucket_width: f64,
    pub buckets: Vec<SimilarityHistogramBucket>,
    pub sampled_pairs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdMethod {
    /// Maximises between-class variance over the histogram.
    Otsu,
    /// The upper edge of the widest empty stretch.
    LargestGap,
    /// The score below which the given percentage of the sample falls.
    Percentile(u8),
    /// The built-in default, recommended when the sample cannot support better.
    Default,
}

impl ThresholdMethod {
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Otsu => "otsu".to_owned(),
            Self::LargestGap => "largest-gap".to_owned(),
            Self::Percentile(percent) => format!("p{percent}"),
            Self::Default => "default".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThresholdCandidate {
    pub method: ThresholdMethod,
    pub threshold: f64,
    pub pairs_at_or_above: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloneThresholdReport {
    pub histogram: SimilarityHistogram,
    pub recommended: ThresholdCandidate,
    /// True when the sample was large enough for the recommendation to mean
    /// something. False means `recommended` is the built-in default.
    pub well_supported: bool,
    pub candidates: Vec<ThresholdCandidate>,
    /// Buckets in the widest empty stretch between two occupied ones. The
    /// evidence behind a `largest-gap` recommendation, reported so a reader can
    /// judge it rather than take it.
    pub widest_gap_buckets: usize,
    pub candidate_forms: usize,
    pub possible_pairs: usize,
    pub evaluated_pairs: usize,
    pub scored_pairs: usize,
    pub default_threshold: f64,
    pub pairs_at_default: usize,
}

pub fn build_clone_threshold_report(
    candidates: Vec<SimilarityCandidate>,
    options: &SimilarityReportOptions,
    calibration: &CloneThresholdOptions,
) -> SimilarityAnalysisResult<CloneThresholdReport> {
    let candidate_forms = candidates.len();
    // Score against the floor, not against whatever `--threshold` says: the
    // point is to see the part of the distribution the current threshold hides.
    let sampling_options = SimilarityReportOptions::new(
        calibration.floor,
        options.min_node_count(),
        options.min_line_span(),
        options.comparison_scope(),
        options.form_scope(),
        crate::similarity_report::domain::SimilarityOverlapPolicy::All,
        options.max_candidates(),
        options.max_comparisons(),
        options.max_results(),
    )?;
    let report = build_similarity_pairs_with_omissions(candidates, 0, &sampling_options)?;

    let mut scores = report
        .pairs
        .iter()
        .map(|pair| pair.similarity().as_f64())
        .collect::<Vec<_>>();
    scores.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    let histogram = build_histogram(&scores, calibration);
    let at_or_above = |threshold: f64| scores.iter().filter(|&&score| score >= threshold).count();

    let mut candidates_out = Vec::new();
    if let Some(threshold) = otsu_threshold(&histogram) {
        candidates_out.push(ThresholdCandidate {
            method: ThresholdMethod::Otsu,
            threshold,
            pairs_at_or_above: at_or_above(threshold),
        });
    }
    let gap = largest_gap_threshold(&histogram);
    if let Some((_, threshold)) = gap {
        candidates_out.push(ThresholdCandidate {
            method: ThresholdMethod::LargestGap,
            threshold,
            pairs_at_or_above: at_or_above(threshold),
        });
    }
    let widest_gap_buckets = gap.map_or(0, |(width, _)| width);
    for percent in [50u8, 75, 90, 95, 99] {
        if let Some(threshold) = percentile(&scores, percent) {
            candidates_out.push(ThresholdCandidate {
                method: ThresholdMethod::Percentile(percent),
                threshold,
                pairs_at_or_above: at_or_above(threshold),
            });
        }
    }

    let pairs_at_default = at_or_above(calibration.default_threshold);
    let default_candidate = ThresholdCandidate {
        method: ThresholdMethod::Default,
        threshold: calibration.default_threshold,
        pairs_at_or_above: pairs_at_default,
    };

    // A wide empty stretch between two populated regions is the strongest
    // evidence available that there really are two populations, so it wins when
    // it exists. Otsu always returns *something*, and on the usual shape — a
    // large low mass of coincidentally-similar pairs and a small high mass of
    // real clones — what it returns is pulled toward the mass rather than
    // toward the boundary. It is the fallback, not the first choice.
    let preferred = if widest_gap_buckets >= calibration.min_gap_buckets {
        ThresholdMethod::LargestGap
    } else {
        ThresholdMethod::Otsu
    };
    let chosen = candidates_out
        .iter()
        .find(|candidate| candidate.method == preferred)
        .cloned();
    let well_supported = scores.len() >= calibration.min_sample && chosen.is_some();
    let recommended = match chosen {
        Some(candidate) if well_supported => candidate,
        _ => default_candidate,
    };

    Ok(CloneThresholdReport {
        histogram,
        recommended,
        well_supported,
        candidates: candidates_out,
        widest_gap_buckets,
        candidate_forms,
        possible_pairs: report.summary.possible_pairs(),
        evaluated_pairs: report.summary.evaluated_pairs(),
        scored_pairs: scores.len(),
        default_threshold: calibration.default_threshold,
        pairs_at_default,
    })
}

pub(super) fn build_histogram(
    scores: &[f64],
    calibration: &CloneThresholdOptions,
) -> SimilarityHistogram {
    let width = if calibration.bucket_width > 0.0 {
        calibration.bucket_width
    } else {
        DEFAULT_BUCKET_WIDTH
    };
    let floor = calibration.floor.clamp(0.0, 1.0);
    let bucket_count = (((1.0 - floor) / width).ceil() as usize).max(1);
    let mut counts = vec![0usize; bucket_count];
    for &score in scores {
        if score < floor {
            continue;
        }
        // The closed top end goes in the last bucket rather than off the end,
        // and a score of exactly 1.0 is the most common score there is.
        let index = (((score - floor) / width) as usize).min(bucket_count - 1);
        counts[index] += 1;
    }

    SimilarityHistogram {
        floor,
        bucket_width: width,
        buckets: counts
            .into_iter()
            .enumerate()
            .map(|(index, count)| SimilarityHistogramBucket {
                lower: floor + index as f64 * width,
                upper: (floor + (index + 1) as f64 * width).min(1.0),
                count,
            })
            .collect(),
        sampled_pairs: scores.iter().filter(|&&score| score >= floor).count(),
    }
}

/// Otsu's method over the histogram.
///
/// Walks every split point, treating bucket midpoints as the values and bucket
/// counts as the weights, and returns the split maximising
/// `w0 * w1 * (mu0 - mu1)^2` — the between-class variance, up to a constant
/// factor that does not move the argmax. Returns `None` when there is nothing
/// to split: an empty sample, or one where every score is in one bucket.
pub(super) fn otsu_threshold(histogram: &SimilarityHistogram) -> Option<f64> {
    let total: usize = histogram.buckets.iter().map(|bucket| bucket.count).sum();
    if total == 0 {
        return None;
    }
    let occupied = histogram
        .buckets
        .iter()
        .filter(|bucket| bucket.count > 0)
        .count();
    if occupied < 2 {
        return None;
    }

    let midpoint = |bucket: &SimilarityHistogramBucket| (bucket.lower + bucket.upper) / 2.0;
    let total_weight = total as f64;
    let total_sum: f64 = histogram
        .buckets
        .iter()
        .map(|bucket| bucket.count as f64 * midpoint(bucket))
        .sum();

    let mut best: Option<(f64, f64)> = None;
    let mut low_weight = 0.0;
    let mut low_sum = 0.0;
    for bucket in &histogram.buckets {
        low_weight += bucket.count as f64;
        low_sum += bucket.count as f64 * midpoint(bucket);
        let high_weight = total_weight - low_weight;
        if low_weight == 0.0 || high_weight == 0.0 {
            continue;
        }
        let low_mean = low_sum / low_weight;
        let high_mean = (total_sum - low_sum) / high_weight;
        let variance = low_weight * high_weight * (low_mean - high_mean).powi(2);
        // The threshold is the boundary above this bucket: everything in the
        // high class scores at least that much.
        let threshold = bucket.upper.min(1.0);
        if best.is_none_or(|(best_variance, _)| variance > best_variance) {
            best = Some((variance, threshold));
        }
    }

    best.map(|(_, threshold)| threshold)
}

/// The upper edge of the widest run of empty buckets that has occupied buckets
/// on both sides.
///
/// Where Otsu answers "which split separates the mass best", this answers
/// "where is the actual hole". They usually agree; when they do not, the
/// disagreement is worth seeing.
pub(super) fn largest_gap_threshold(histogram: &SimilarityHistogram) -> Option<(usize, f64)> {
    let first = histogram
        .buckets
        .iter()
        .position(|bucket| bucket.count > 0)?;
    let last = histogram
        .buckets
        .iter()
        .rposition(|bucket| bucket.count > 0)?;
    if last <= first {
        return None;
    }

    let mut best: Option<(usize, f64)> = None;
    let mut run_start: Option<usize> = None;
    for index in first..=last {
        if histogram.buckets[index].count == 0 {
            run_start.get_or_insert(index);
            continue;
        }
        if let Some(start) = run_start.take() {
            let length = index - start;
            let threshold = histogram.buckets[index - 1].upper.min(1.0);
            if best.is_none_or(|(best_length, _)| length > best_length) {
                best = Some((length, threshold));
            }
        }
    }

    best
}

/// The nearest-rank percentile of an ascending sample.
pub(super) fn percentile(sorted_scores: &[f64], percent: u8) -> Option<f64> {
    if sorted_scores.is_empty() {
        return None;
    }
    let rank = (f64::from(percent) / 100.0 * sorted_scores.len() as f64).ceil() as usize;
    let index = rank.max(1) - 1;
    sorted_scores
        .get(index.min(sorted_scores.len() - 1))
        .copied()
}
