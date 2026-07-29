//! Cross-corpus clone detection: this project against a reference corpus.
//!
//! The wheel-reinvention check. Every other report in this package compares a
//! project against itself, which cannot answer the question that matters most
//! before writing a helper: does the code we already depend on have this?
//!
//! Two things make this a separate pass rather than a flag on `similarity`.
//! First, only cross-corpus pairs are interesting — a reference library
//! duplicating itself is not this project's problem — so the comparison is a
//! rectangle rather than a triangle. Second, and more important, `similarity`
//! only ever compares forms whose head symbol agrees, which is a sound
//! optimisation within one project and exactly wrong here: a local
//! `join-strings` and a library `str:join` disagree on the head and are the
//! whole point. This pass compares across heads and pays for it with a
//! node-count window instead.

use std::sync::Arc;

use crate::error::{SimilarityAnalysisResult, SimilarityBudgetError};
use crate::form_similarity::{
    CloneType, MAX_REPORT_TREE_EDIT_OPERATIONS, TreeSimilarityOperationBudget,
    TreeSimilarityWorkspace, classify_clone, reserve_tree_similarity_workspaces,
    similarity_upper_bound, tree_similarity_with_workspace_and_budget,
};
use crate::similarity_report::domain::{
    SimilarityCandidate, SimilarityFormReport, SimilarityReportOptions,
};

/// Matches retained when `--max-results` says nothing.
///
/// Mirrors the pair engine's own default. Without a cap, comparing a project
/// against a large corpus at `--form-scope all` retains a match for every
/// `(persist payload)` against every other, which is millions of rows nobody
/// reads and a report that no longer fits in memory.
pub const DEFAULT_EXTERNAL_MAX_RESULTS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExternalCorpusStats {
    pub files: usize,
    pub candidates: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloneExternalMatch {
    pub similarity: f64,
    pub clone_type: CloneType,
    pub consistent_renaming: bool,
    pub project: Arc<SimilarityFormReport>,
    pub reference: Arc<SimilarityFormReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloneExternalReport {
    pub matches: Vec<CloneExternalMatch>,
    pub project: ExternalCorpusStats,
    pub reference: ExternalCorpusStats,
    pub possible_pairs: usize,
    pub evaluated_pairs: usize,
    pub pruned_by_size: usize,
    pub pruned_by_bound: usize,
    pub matched_pairs: usize,
    pub truncated: bool,
    pub comparison_limit_reached: bool,
}

/// Compares every project candidate against every reference candidate.
///
/// Pruning happens in three widening steps, cheapest first: a node-count window
/// found by binary search over the reference candidates sorted by size, then
/// the label-multiset bound, then the tree edit distance itself. Only the last
/// one is expensive, and by then most pairs are gone.
pub fn build_clone_external_report(
    project: Vec<SimilarityCandidate>,
    reference: Vec<SimilarityCandidate>,
    project_files: usize,
    reference_files: usize,
    options: &SimilarityReportOptions,
) -> SimilarityAnalysisResult<CloneExternalReport> {
    options.validate()?;

    let mut reference = reference;
    reference.sort_by_key(|candidate| candidate.form().node_count());
    let reference_sizes = reference
        .iter()
        .map(|candidate| candidate.form().node_count())
        .collect::<Vec<_>>();

    // Reported rather than refused when it exceeds `--max-comparisons`: the
    // pass runs, stops at the limit, and says so in the summary.
    let possible_pairs = project.len().saturating_mul(reference.len());

    let _reservation = reserve_tree_similarity_workspaces(1);
    let mut workspace = TreeSimilarityWorkspace::default();
    let operation_budget = TreeSimilarityOperationBudget::new(MAX_REPORT_TREE_EDIT_OPERATIONS);

    let threshold = options.threshold();
    let result_limit = options
        .max_results()
        .unwrap_or(DEFAULT_EXTERNAL_MAX_RESULTS);
    let mut matches = Vec::new();
    let mut evaluated_pairs = 0usize;
    let mut matched_pairs = 0usize;
    let mut pruned_by_size = 0usize;
    let mut pruned_by_bound = 0usize;
    let mut comparison_limit_reached = false;

    'outer: for left in &project {
        let left_nodes = left.form().node_count();
        let window = size_window(&reference_sizes, left_nodes, threshold);
        pruned_by_size = pruned_by_size.saturating_add(reference.len() - window.len());

        for right in &reference[window] {
            if options
                .max_comparisons()
                .is_some_and(|limit| evaluated_pairs >= limit)
            {
                comparison_limit_reached = true;
                break 'outer;
            }
            if similarity_upper_bound(left.tree(), right.tree()) < threshold {
                pruned_by_bound += 1;
                continue;
            }
            evaluated_pairs += 1;
            let similarity = tree_similarity_with_workspace_and_budget(
                left.tree(),
                right.tree(),
                &mut workspace,
                Some(&operation_budget),
            )
            .map_err(SimilarityBudgetError::from)?;
            if similarity < threshold {
                continue;
            }
            let classification = classify_clone(left.tree(), right.tree());
            matched_pairs += 1;
            matches.push(CloneExternalMatch {
                similarity,
                clone_type: classification.clone_type,
                consistent_renaming: classification.consistent_renaming,
                project: Arc::clone(left.form()),
                reference: Arc::clone(right.form()),
            });
            // Sort and cut when the buffer reaches twice the limit rather than
            // on every push: the same bound on memory, amortised to one sort
            // per limit-many matches instead of a heap operation per match.
            if matches.len() >= result_limit.saturating_mul(2) {
                matches.sort_by(compare_matches);
                matches.truncate(result_limit);
            }
        }
    }

    matches.sort_by(compare_matches);
    let truncated = matched_pairs > result_limit;
    matches.truncate(result_limit);

    Ok(CloneExternalReport {
        matches,
        project: ExternalCorpusStats {
            files: project_files,
            candidates: project.len(),
        },
        reference: ExternalCorpusStats {
            files: reference_files,
            candidates: reference.len(),
        },
        possible_pairs,
        evaluated_pairs,
        pruned_by_size,
        pruned_by_bound,
        matched_pairs,
        truncated,
        comparison_limit_reached,
    })
}

fn compare_matches(left: &CloneExternalMatch, right: &CloneExternalMatch) -> std::cmp::Ordering {
    right
        .similarity
        .partial_cmp(&left.similarity)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| left.clone_type.cmp(&right.clone_type))
        .then_with(|| right.project.node_count().cmp(&left.project.node_count()))
        .then_with(|| left.project.path().cmp(right.project.path()))
        .then_with(|| {
            left.project
                .span()
                .start()
                .get()
                .cmp(&right.project.span().start().get())
        })
        .then_with(|| left.reference.path().cmp(right.reference.path()))
        .then_with(|| {
            left.reference
                .span()
                .start()
                .get()
                .cmp(&right.reference.span().start().get())
        })
}

/// The slice of size-sorted reference candidates that can still clear the
/// threshold.
///
/// A rename never changes the node count, and every insertion or deletion costs
/// one edit, so `1 - |l - r| / max(l, r)` is an upper bound on the similarity of
/// a pair with those two sizes. Solving it for `r` gives `t*l <= r <= l/t`, and
/// because the candidates are sorted, both ends are a binary search.
fn size_window(sizes: &[usize], left_nodes: usize, threshold: f64) -> std::ops::Range<usize> {
    if threshold <= 0.0 {
        return 0..sizes.len();
    }
    let lower = (threshold * left_nodes as f64).ceil();
    let upper = left_nodes as f64 / threshold;
    let start = sizes.partition_point(|&size| (size as f64) < lower);
    let end = sizes.partition_point(|&size| (size as f64) <= upper);
    start..end.max(start)
}

/// Exposed for the domain tests, which need to drive the window directly rather
/// than infer it from a whole report.
#[cfg(test)]
pub(super) fn size_window_for_test(
    sizes: &[usize],
    left_nodes: usize,
    threshold: f64,
) -> std::ops::Range<usize> {
    size_window(sizes, left_nodes, threshold)
}
