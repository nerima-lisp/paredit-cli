use std::num::NonZeroUsize;
use std::path::Path as FsPath;
use std::path::PathBuf;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteOffset, ByteSpan, Path as SExprPath, SyntaxTree};

use super::*;

fn similarity_pair_report(
    similarity: f64,
    score: f64,
    left: SimilarityFormReport,
    right: SimilarityFormReport,
) -> SimilarityPairReport {
    SimilarityPairReport::new(
        SimilarityRatio::try_from(similarity).unwrap(),
        SimilarityScore::try_from(score).unwrap(),
        left,
        right,
    )
}

#[test]
fn similarity_value_objects_reject_invalid_numbers() {
    assert!(SimilarityRatio::try_from(0.0).is_ok());
    assert!(SimilarityRatio::try_from(1.0).is_ok());
    assert!(SimilarityRatio::try_from(-0.01).is_err());
    assert!(SimilarityRatio::try_from(1.01).is_err());
    assert!(SimilarityRatio::try_from(f64::NAN).is_err());
    assert!(SimilarityRatio::try_from(f64::INFINITY).is_err());

    assert!(SimilarityScore::try_from(0.0).is_ok());
    assert!(SimilarityScore::try_from(-0.01).is_err());
    assert!(SimilarityScore::try_from(f64::NAN).is_err());
    assert!(SimilarityScore::try_from(f64::INFINITY).is_err());
}

#[test]
fn report_limits_encode_complete_and_limited_states_exclusively() {
    let complete = ReportLimit::from_omitted(0);
    assert_eq!(complete, ReportLimit::Complete);
    assert!(!complete.reached());
    assert_eq!(complete.omitted(), 0);

    let limited = ReportLimit::from_omitted(3);
    assert_eq!(limited, ReportLimit::Limited(NonZeroUsize::new(3).unwrap()));
    assert!(limited.reached());
    assert_eq!(limited.omitted(), 3);
}

#[test]
fn report_summary_derives_truncation_from_result_counts() {
    let complete = SimilarityReportSummary::new(
        ReportLimit::Complete,
        PairProcessingCounts::new(4, 4, 0, 0).unwrap(),
        ReportLimit::Complete,
        PairResultCounts::new(3, 1, 2).unwrap(),
    )
    .unwrap();
    assert!(!complete.truncated());

    let truncated = SimilarityReportSummary::new(
        ReportLimit::Complete,
        PairProcessingCounts::new(4, 4, 0, 0).unwrap(),
        ReportLimit::Complete,
        PairResultCounts::new(3, 1, 1).unwrap(),
    )
    .unwrap();
    assert!(truncated.truncated());
}

#[test]
fn report_count_types_reject_invalid_invariants() {
    assert_eq!(
        PairProcessingCounts::new(1, 1, 1, 0),
        Err(InvalidSimilarityReport::ProcessedPairsExceedPossible {
            possible: 1,
            processed: 2,
        })
    );
    assert_eq!(
        PairResultCounts::new(1, 2, 0),
        Err(InvalidSimilarityReport::SuppressedPairsExceedMatched {
            matched: 1,
            suppressed: 2,
        })
    );
    assert_eq!(
        PairResultCounts::new(2, 1, 2),
        Err(InvalidSimilarityReport::ReportedPairsExceedAvailable {
            available: 1,
            reported: 2,
        })
    );
}

#[test]
fn report_aggregates_reject_inconsistent_counts() {
    let processing = PairProcessingCounts::new(4, 1, 1, 0).unwrap();
    let results = PairResultCounts::new(0, 0, 0).unwrap();
    assert_eq!(
        SimilarityReportSummary::new(
            ReportLimit::Complete,
            processing,
            ReportLimit::from_omitted(1),
            results,
        ),
        Err(InvalidSimilarityReport::PairAccountingMismatch {
            possible: 4,
            accounted: 3,
        })
    );

    let summary = SimilarityReportSummary::new(
        ReportLimit::Complete,
        PairProcessingCounts::new(0, 0, 0, 0).unwrap(),
        ReportLimit::Complete,
        PairResultCounts::new(1, 0, 1).unwrap(),
    )
    .unwrap();
    assert_eq!(
        SimilarityReport::new(summary, Vec::new()),
        Err(InvalidSimilarityReport::ReportedPairCountMismatch {
            reported: 1,
            actual: 0,
        })
    );
}

fn candidates(file: &str, input: &str, min_node_count: usize) -> Vec<SimilarityCandidate> {
    let tree = SyntaxTree::parse(input).unwrap();
    let mut result = Vec::new();
    let options = report_options(
        0.87,
        min_node_count,
        1,
        SimilarityComparisonScope::All,
        SimilarityFormScope::All,
        SimilarityOverlapPolicy::Maximal,
        None,
        None,
        None,
    );
    collect_similarity_candidates(
        &tree,
        input,
        FsPath::new(file),
        Dialect::CommonLisp,
        &options,
        &mut result,
    )
    .unwrap();
    result
}

#[allow(clippy::too_many_arguments)]
fn report_options(
    threshold: f64,
    min_node_count: usize,
    min_line_span: usize,
    comparison_scope: SimilarityComparisonScope,
    form_scope: SimilarityFormScope,
    overlap_policy: SimilarityOverlapPolicy,
    max_candidates: Option<usize>,
    max_comparisons: Option<usize>,
    max_results: Option<usize>,
) -> SimilarityReportOptions {
    SimilarityReportOptions::new(
        threshold,
        min_node_count,
        min_line_span,
        comparison_scope,
        form_scope,
        overlap_policy,
        max_candidates,
        max_comparisons,
        max_results,
    )
    .unwrap()
}

fn build_similarity_pairs(
    candidates: Vec<SimilarityCandidate>,
    threshold: f64,
    overlap_policy: SimilarityOverlapPolicy,
    max_results: Option<usize>,
) -> SimilarityReport {
    super::reports::build_similarity_pairs(
        candidates,
        &report_options(
            threshold,
            4,
            1,
            SimilarityComparisonScope::All,
            SimilarityFormScope::All,
            overlap_policy,
            None,
            None,
            max_results,
        ),
    )
    .unwrap()
}

fn report_form(path: &str, start: usize, end: usize) -> SimilarityFormReport {
    SimilarityFormReport::new(
        PathBuf::from(path),
        Dialect::CommonLisp,
        SExprPath::from_indexes(vec![start, end]),
        ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end)),
        1,
        None,
        String::new(),
    )
}

fn similarity_candidate(file: &str, node_count: usize) -> SimilarityCandidate {
    let input = "(foo a)";
    let tree = SyntaxTree::parse(input).unwrap();
    SimilarityCandidate::new(
        SimilarityFormReport::new(
            PathBuf::from(file),
            Dialect::CommonLisp,
            SExprPath::from_indexes(vec![0, 7]),
            ByteSpan::new(ByteOffset::new(0), ByteOffset::new(input.len())),
            node_count,
            Some("foo".into()),
            input,
        ),
        crate::domain::form_similarity::StructuralTree::from_view(
            &tree
                .select_path(&crate::domain::sexpr::Path::root_child(0))
                .unwrap()
                .view(),
        ),
        Some("foo".into()),
    )
}

fn structural_similarity_candidate(file: &str, input: &str, index: usize) -> SimilarityCandidate {
    let syntax_tree = SyntaxTree::parse(input).unwrap();
    let structural_tree = crate::domain::form_similarity::StructuralTree::from_view(
        &syntax_tree
            .select_path(&crate::domain::sexpr::Path::root_child(0))
            .unwrap()
            .view(),
    );
    SimilarityCandidate::new(
        SimilarityFormReport::new(
            PathBuf::from(file),
            Dialect::CommonLisp,
            SExprPath::root_child(index),
            ByteSpan::new(ByteOffset::new(0), ByteOffset::new(input.len())),
            structural_tree.node_count(),
            Some("foo".into()),
            input,
        ),
        structural_tree,
        Some("foo".into()),
    )
}

#[test]
fn report_tree_edit_budget_is_exact_and_shared_across_workers() {
    let left_shape = "(foo (bar a) b)";
    let right_shape = "(foo bar (a b))";
    let values = (0..96)
        .map(|index| {
            structural_similarity_candidate(
                "budget.lisp",
                if index % 2 == 0 {
                    left_shape
                } else {
                    right_shape
                },
                index,
            )
        })
        .collect::<Vec<_>>();

    let sequential = super::reports::tree_edit_operation_budget_execution_for_test(&values, 1, 1);
    let parallel = super::reports::tree_edit_operation_budget_execution_for_test(&values, 1, 2);

    assert_eq!(sequential.0, parallel.0);
    assert_eq!(
        sequential.0.as_deref(),
        Some("tree similarity operation budget exceeded (2 operations, limit 1)")
    );
    assert_eq!((sequential.1, parallel.1), (2, 2));
    assert!(sequential.2 && parallel.2);
    assert_eq!(sequential.3, 1);
    assert_eq!(parallel.3, 2);

    let repeated = super::reports::tree_edit_operation_budget_execution_for_test(&values, 1, 2);
    assert_eq!(repeated, parallel);
}

#[test]
fn report_leaf_count_upper_bound_prunes_before_tree_edit() {
    let values = [
        structural_similarity_candidate("left.lisp", "((a) (b))", 0),
        structural_similarity_candidate("right.lisp", "(() (a b))", 1),
    ];

    assert_eq!(
        super::reports::pruning_execution_for_test(&values, 0.9),
        (1, 0, 0)
    );
    assert_eq!(
        super::reports::pruning_execution_for_test(&values, 0.8),
        (1, 0, 1)
    );
}

#[test]
fn similarity_form_containment_is_span_based() {
    let outer = report_form("a.lisp", 0, 30);
    let inner = report_form("a.lisp", 10, 20);
    let same = report_form("a.lisp", 0, 30);

    assert!(outer.contains_span(&inner));
    assert!(outer.strictly_contains_span(&inner));
    assert!(outer.contains_span(&same));
    assert!(!outer.strictly_contains_span(&same));
}

#[test]
fn similarity_pair_containment_is_span_based_per_side() {
    let outer = similarity_pair_report(
        1.0,
        2.0,
        report_form("a.lisp", 0, 30),
        report_form("b.lisp", 10, 20),
    );
    let inner = similarity_pair_report(
        1.0,
        1.0,
        report_form("a.lisp", 5, 25),
        report_form("b.lisp", 12, 18),
    );
    let sibling = similarity_pair_report(
        1.0,
        1.0,
        report_form("a.lisp", 0, 30),
        report_form("b.lisp", 30, 40),
    );

    assert!(outer.strictly_contains_pair(&inner));
    assert!(!inner.strictly_contains_pair(&outer));
    assert!(!outer.strictly_contains_pair(&sibling));
}

#[test]
fn similarity_candidate_comparison_bucket_uses_head_identity() {
    let left = similarity_candidate("a.lisp", 3);
    let matching = left.clone();
    let missing_head = SimilarityCandidate::new(left.form().clone(), left.tree().clone(), None);

    assert!(left.same_comparison_bucket(&matching));
    assert!(!left.same_comparison_bucket(&missing_head));
    assert!(missing_head.cmp_comparison_bucket(&left).is_lt());
}

#[test]
fn form_scope_top_level_excludes_nested_forms() {
    let tree = SyntaxTree::parse("(outer (inner value))").unwrap();
    let mut values = Vec::new();
    collect_similarity_candidates(
        &tree,
        "(outer (inner value))",
        FsPath::new("a.lisp"),
        Dialect::CommonLisp,
        &report_options(
            0.87,
            2,
            1,
            SimilarityComparisonScope::All,
            SimilarityFormScope::TopLevel,
            SimilarityOverlapPolicy::Maximal,
            None,
            None,
            None,
        ),
        &mut values,
    )
    .unwrap();

    assert_eq!(values.len(), 1);
    assert_eq!(values[0].form().text(), "(outer (inner value))");
}

#[test]
fn minimum_line_span_uses_inclusive_one_based_length() {
    let input = "(multi\n value)\n(single value)";
    let tree = SyntaxTree::parse(input).unwrap();
    let mut values = Vec::new();
    collect_similarity_candidates(
        &tree,
        input,
        FsPath::new("a.lisp"),
        Dialect::CommonLisp,
        &report_options(
            0.87,
            2,
            2,
            SimilarityComparisonScope::All,
            SimilarityFormScope::All,
            SimilarityOverlapPolicy::Maximal,
            None,
            None,
            None,
        ),
        &mut values,
    )
    .unwrap();

    assert_eq!(values.len(), 1);
    assert_eq!(values[0].form().text(), "(multi\n value)");
}

#[test]
fn candidate_limit_counts_only_eligible_omissions() {
    let input = "(keep value) (too-small) (omit\n value) (single-line value)";
    let tree = SyntaxTree::parse(input).unwrap();
    let mut values = Vec::new();
    let omitted = collect_similarity_candidates(
        &tree,
        input,
        FsPath::new("a.lisp"),
        Dialect::CommonLisp,
        &report_options(
            0.87,
            3,
            2,
            SimilarityComparisonScope::All,
            SimilarityFormScope::All,
            SimilarityOverlapPolicy::Maximal,
            Some(1),
            None,
            None,
        ),
        &mut values,
    )
    .unwrap();

    assert_eq!(values.len(), 1);
    assert_eq!(values[0].form().text(), "(omit\n value)");
    assert_eq!(omitted, 0);

    let input = "(keep\n value) (too-small) (omit\n value) (single-line value)";
    let tree = SyntaxTree::parse(input).unwrap();
    values.clear();
    let omitted = collect_similarity_candidates(
        &tree,
        input,
        FsPath::new("a.lisp"),
        Dialect::CommonLisp,
        &report_options(
            0.87,
            3,
            2,
            SimilarityComparisonScope::All,
            SimilarityFormScope::All,
            SimilarityOverlapPolicy::Maximal,
            Some(1),
            None,
            None,
        ),
        &mut values,
    )
    .unwrap();

    assert_eq!(values.len(), 1);
    assert_eq!(values[0].form().text(), "(keep\n value)");
    assert_eq!(omitted, 1);
}

#[test]
fn comparison_scope_filters_pair_population() {
    let mut values = candidates("a.lisp", "(foo a) (foo b)", 2);
    values.extend(candidates("b.lisp", "(foo c)", 2));
    let report = |comparison_scope| {
        super::reports::build_similarity_pairs(
            values.clone(),
            &report_options(
                0.0,
                4,
                1,
                comparison_scope,
                SimilarityFormScope::All,
                SimilarityOverlapPolicy::All,
                None,
                None,
                None,
            ),
        )
        .unwrap()
    };

    let all = report(SimilarityComparisonScope::All);
    let same_file = report(SimilarityComparisonScope::SameFile);
    let cross_file = report(SimilarityComparisonScope::CrossFile);
    assert_eq!(all.summary.possible_pairs(), 3);
    assert_eq!(same_file.summary.possible_pairs(), 1);
    assert_eq!(cross_file.summary.possible_pairs(), 2);
    assert_eq!(same_file.summary.evaluated_pairs(), 1);
    assert_eq!(cross_file.summary.evaluated_pairs(), 2);
}

#[test]
fn small_or_single_chunk_comparisons_stay_on_the_calling_thread() {
    assert_eq!(
        super::reports::scheduling_policy_for_test(8, 2_048, 1),
        (1, false)
    );
    assert_eq!(
        super::reports::scheduling_policy_for_test(8, 2_049, 1),
        (4, false)
    );
    assert_eq!(
        super::reports::scheduling_policy_for_test(1, 2_049, 1),
        (1, false)
    );
    assert_eq!(
        super::reports::scheduling_policy_for_test(8, 2_049, 2),
        (4, true)
    );
    assert_eq!(
        super::reports::scheduling_policy_for_test(usize::MAX, usize::MAX, usize::MAX),
        (4, true)
    );
}

#[test]
fn large_result_limits_constrain_worker_local_buffers() {
    assert_eq!(
        super::reports::result_bounded_worker_count_for_test(4, Some(250_000)),
        4
    );
    assert_eq!(
        super::reports::result_bounded_worker_count_for_test(4, Some(250_001)),
        1
    );
    assert_eq!(
        super::reports::result_bounded_worker_count_for_test(4, Some(1_000_000)),
        1
    );
}

#[test]
fn large_scoped_groups_are_split_at_file_boundaries() {
    let mut values = Vec::new();
    for path in ["a.lisp", "b.lisp", "c.lisp"] {
        values.extend((0..40).map(|_| similarity_candidate(path, 3)));
    }
    values.sort_unstable_by(|left, right| {
        left.form()
            .node_count()
            .cmp(&right.form().node_count())
            .then_with(|| left.form().path().cmp(right.form().path()))
    });
    let groups = [values.as_slice()];

    let mut same_file_costs =
        super::reports::work_item_costs_for_test(&groups, SimilarityComparisonScope::SameFile, 4);
    same_file_costs.sort_unstable();
    assert_eq!(same_file_costs, vec![780, 780, 780]);

    let mut cross_file_costs =
        super::reports::work_item_costs_for_test(&groups, SimilarityComparisonScope::CrossFile, 4);
    cross_file_costs.sort_unstable();
    assert_eq!(cross_file_costs, vec![1_600, 3_200]);
    assert_eq!(cross_file_costs.iter().sum::<usize>(), 4_800);
}

#[test]
fn split_scoped_comparisons_match_the_sequential_path() {
    let input = std::iter::repeat_n("(foo a)", 40)
        .collect::<Vec<_>>()
        .join(" ");
    let mut values = Vec::new();
    for path in ["a.lisp", "b.lisp", "c.lisp"] {
        values.extend(candidates(path, &input, 2));
    }

    let options = |comparison_scope, max_comparisons| {
        report_options(
            1.0,
            2,
            1,
            comparison_scope,
            SimilarityFormScope::All,
            SimilarityOverlapPolicy::Maximal,
            None,
            max_comparisons,
            None,
        )
    };
    let scheduled = super::reports::build_similarity_pairs(
        values.clone(),
        &options(SimilarityComparisonScope::SameFile, None),
    )
    .unwrap();
    let sequential = super::reports::build_similarity_pairs(
        values.clone(),
        &options(SimilarityComparisonScope::SameFile, Some(usize::MAX)),
    )
    .unwrap();
    assert_eq!(scheduled, sequential);

    let cross_file_options = options(SimilarityComparisonScope::CrossFile, None);
    let scheduled =
        super::reports::build_similarity_pairs(values.clone(), &cross_file_options).unwrap();
    let sequential = super::reports::build_similarity_pairs(
        values.clone(),
        &options(SimilarityComparisonScope::CrossFile, Some(usize::MAX)),
    )
    .unwrap();
    assert_eq!(scheduled, sequential);
    assert_eq!(sequential.summary.evaluated_pairs(), 4_800);
    assert_eq!(sequential.summary.unprocessed_pairs(), 0);

    let mut reversed = values;
    reversed.reverse();
    let reversed = super::reports::build_similarity_pairs(reversed, &cross_file_options).unwrap();
    assert_eq!(scheduled, reversed);
    assert_eq!(scheduled.summary.possible_pairs(), 4_800);
    assert_eq!(scheduled.summary.evaluated_pairs(), 4_800);
    assert_eq!(scheduled.summary.matched_pairs(), 4_800);
}

#[test]
fn threshold_is_inclusive() {
    let values = candidates("a.lisp", "(foo a b) (foo x y)", 2);
    let similarity =
        crate::domain::form_similarity::tree_similarity(values[0].tree(), values[1].tree())
            .unwrap();
    let report = build_similarity_pairs(values, similarity, SimilarityOverlapPolicy::All, None);
    assert_eq!(report.pairs.len(), 1);
    assert_eq!(report.summary.evaluated_pairs(), 1);
}

#[test]
fn empty_and_single_candidates_produce_no_pairs() {
    assert!(
        build_similarity_pairs(Vec::new(), 0.0, SimilarityOverlapPolicy::All, None)
            .pairs
            .is_empty()
    );
    assert!(
        build_similarity_pairs(
            candidates("a.lisp", "(foo a)", 2),
            0.0,
            SimilarityOverlapPolicy::All,
            None,
        )
        .pairs
        .is_empty()
    );
}

#[test]
fn reversed_input_has_deterministic_output() {
    let mut forward = candidates("a.lisp", "(foo a b)", 2);
    forward.extend(candidates("b.lisp", "(foo x y)", 2));
    let mut reverse = forward.clone();
    reverse.reverse();
    assert_eq!(
        build_similarity_pairs(forward, 0.0, SimilarityOverlapPolicy::All, None),
        build_similarity_pairs(reverse, 0.0, SimilarityOverlapPolicy::All, None)
    );
}

#[test]
fn size_lower_bound_prunes_without_changing_inclusive_boundary() {
    let mut values = candidates("a.lisp", "(foo a)", 2);
    values.extend(candidates("b.lisp", "(foo a b c)", 2));

    let pruned = build_similarity_pairs(values.clone(), 0.75, SimilarityOverlapPolicy::All, None);
    assert_eq!(pruned.summary.possible_pairs(), 1);
    assert_eq!(pruned.summary.pruned_by_size(), 1);
    assert_eq!(pruned.summary.evaluated_pairs(), 0);

    let boundary = build_similarity_pairs(values, 0.6, SimilarityOverlapPolicy::All, None);
    assert_eq!(boundary.summary.pruned_by_size(), 0);
    assert_eq!(boundary.summary.evaluated_pairs(), 1);

    let disabled = build_similarity_pairs(
        candidates("a.lisp", "(foo a) (foo a b c)", 2),
        0.0,
        SimilarityOverlapPolicy::All,
        None,
    );
    assert_eq!(disabled.summary.possible_pairs(), 1);
    assert_eq!(disabled.summary.pruned_by_size(), 0);
    assert_eq!(disabled.summary.evaluated_pairs(), 1);
    assert_eq!(
        disabled.summary.evaluated_pairs() + disabled.summary.pruned_by_size(),
        disabled.summary.possible_pairs()
    );
}

#[test]
fn sequential_size_pruning_keeps_later_valid_pairs() {
    let values = candidates("a.lisp", "(foo a b c d e) (foo a) (foo a b c d e)", 2);
    let report = super::reports::build_similarity_pairs(
        values,
        &report_options(
            0.75,
            4,
            1,
            SimilarityComparisonScope::All,
            SimilarityFormScope::All,
            SimilarityOverlapPolicy::All,
            None,
            Some(usize::MAX),
            None,
        ),
    )
    .unwrap();

    assert_eq!(report.summary.possible_pairs(), 3);
    assert_eq!(report.summary.pruned_by_size(), 2);
    assert_eq!(report.summary.evaluated_pairs(), 1);
    assert_eq!(report.pairs.len(), 1);
    assert_eq!(report.pairs[0].similarity().as_f64(), 1.0);
}

#[test]
fn cross_file_size_pruning_keeps_later_valid_pairs() {
    let left_group = [similarity_candidate("a.lisp", 5)];
    let right_group = [
        similarity_candidate("b.lisp", 1),
        similarity_candidate("b.lisp", 5),
    ];
    let left_refs: Vec<_> = left_group.iter().collect();
    let right_refs: Vec<_> = right_group.iter().collect();
    let mut workspace = crate::domain::form_similarity::TreeSimilarityWorkspace::default();
    let mut evaluated_pairs = 0;

    let (output, limit_reached) = super::reports::compare_cross_file_group_pair(
        &left_refs,
        &right_refs,
        0.75,
        None,
        None,
        &mut evaluated_pairs,
        &mut workspace,
    );

    assert!(!limit_reached);
    assert_eq!(output.pruned_by_size, 1);
    assert_eq!(output.evaluated_pairs, 1);
    assert_eq!(output.pair_count(), 1);
}

#[test]
fn maximal_overlap_suppresses_only_strictly_contained_pairs() {
    let mut values = candidates("a.lisp", "(outer (same x))", 2);
    values.extend(candidates("b.lisp", "(outer (same y))", 2));

    let all = build_similarity_pairs(values.clone(), 0.0, SimilarityOverlapPolicy::All, None);
    let maximal = build_similarity_pairs(values, 0.0, SimilarityOverlapPolicy::Maximal, None);
    assert_eq!(all.summary.suppressed_pairs(), 0);
    assert!(maximal.summary.suppressed_pairs() > 0);
    assert_eq!(maximal.summary.matched_pairs(), all.summary.matched_pairs());

    let nested_pair = |report: &SimilarityReport| {
        report
            .pairs
            .iter()
            .any(|pair| pair.left().text() == "(same x)" && pair.right().text() == "(same y)")
    };
    assert!(nested_pair(&all));
    assert!(!nested_pair(&maximal));
}

#[test]
fn maximal_overlap_normalizes_reversed_cross_file_pair_orientation() {
    let mut values = candidates("a.lisp", "(outer (same x extra))", 2);
    values.extend(candidates("b.lisp", "(outer padding padding (same y))", 2));
    let node_count = |path: &str, text: &str| {
        values
            .iter()
            .find(|candidate| {
                candidate.form().path() == FsPath::new(path) && candidate.form().text() == text
            })
            .unwrap()
            .form()
            .node_count()
    };

    assert!(
        node_count("a.lisp", "(outer (same x extra))")
            < node_count("b.lisp", "(outer padding padding (same y))")
    );
    assert!(node_count("a.lisp", "(same x extra)") > node_count("b.lisp", "(same y)"));

    let all = build_similarity_pairs(values.clone(), 0.0, SimilarityOverlapPolicy::All, None);
    let maximal = build_similarity_pairs(values, 0.0, SimilarityOverlapPolicy::Maximal, None);
    let nested_pair = |report: &SimilarityReport| {
        report.pairs.iter().any(|pair| {
            (pair.left().text() == "(same x extra)" && pair.right().text() == "(same y)")
                || (pair.left().text() == "(same y)" && pair.right().text() == "(same x extra)")
        })
    };

    assert!(nested_pair(&all));
    assert!(!nested_pair(&maximal));
    assert!(maximal.summary.suppressed_pairs() > 0);
}

#[test]
fn maximal_overlap_normalizes_reversed_same_file_pair_orientation() {
    let mut pairs = vec![
        similarity_pair_report(
            1.0,
            2.0,
            report_form("same.lisp", 0, 20),
            report_form("same.lisp", 30, 60),
        ),
        similarity_pair_report(
            1.0,
            1.0,
            report_form("same.lisp", 40, 50),
            report_form("same.lisp", 5, 10),
        ),
    ];

    assert_eq!(super::reports::suppress_contained_pairs(&mut pairs), 1);
    assert_eq!(pairs.len(), 1);
    assert_eq!(
        pairs[0].left().span(),
        ByteSpan::new(ByteOffset::new(0), ByteOffset::new(20))
    );
    assert_eq!(
        pairs[0].right().span(),
        ByteSpan::new(ByteOffset::new(30), ByteOffset::new(60))
    );
}

#[test]
fn maximal_overlap_retains_sibling_pairs() {
    let mut pairs = vec![
        similarity_pair_report(
            1.0,
            2.0,
            report_form("a.lisp", 0, 10),
            report_form("b.lisp", 0, 10),
        ),
        similarity_pair_report(
            1.0,
            1.0,
            report_form("a.lisp", 20, 30),
            report_form("b.lisp", 20, 30),
        ),
    ];

    assert_eq!(super::reports::suppress_contained_pairs(&mut pairs), 0);
    assert_eq!(pairs.len(), 2);
}

#[test]
fn maximal_overlap_suppresses_when_only_one_side_is_strictly_contained() {
    let mut pairs = vec![
        similarity_pair_report(
            1.0,
            2.0,
            report_form("a.lisp", 10, 20),
            report_form("b.lisp", 0, 30),
        ),
        similarity_pair_report(
            1.0,
            1.0,
            report_form("a.lisp", 0, 30),
            report_form("b.lisp", 0, 30),
        ),
    ];

    assert_eq!(super::reports::suppress_contained_pairs(&mut pairs), 1);
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].score().as_f64(), 1.0);
}

#[test]
fn maximal_overlap_suppression_is_independent_of_score_order() {
    let mut pairs = vec![
        similarity_pair_report(
            1.0,
            10.0,
            report_form("a.lisp", 10, 20),
            report_form("b.lisp", 10, 20),
        ),
        similarity_pair_report(
            1.0,
            1.0,
            report_form("a.lisp", 0, 30),
            report_form("b.lisp", 0, 30),
        ),
    ];

    assert_eq!(super::reports::suppress_contained_pairs(&mut pairs), 1);
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].score().as_f64(), 1.0);
}

#[test]
fn online_maximal_frontier_matches_offline_suppression() {
    fn pairs() -> Vec<SimilarityPairReport> {
        vec![
            similarity_pair_report(
                1.0,
                10.0,
                report_form("a.lisp", 10, 20),
                report_form("b.lisp", 10, 20),
            ),
            similarity_pair_report(
                0.9,
                9.0,
                report_form("a.lisp", 40, 50),
                report_form("b.lisp", 40, 50),
            ),
            similarity_pair_report(
                0.8,
                8.0,
                report_form("a.lisp", 0, 30),
                report_form("b.lisp", 0, 30),
            ),
            similarity_pair_report(
                0.7,
                7.0,
                report_form("a.lisp", 0, 60),
                report_form("b.lisp", 0, 60),
            ),
        ]
    }

    for reverse in [false, true] {
        let mut offline = pairs();
        if reverse {
            offline.reverse();
        }
        let mut online = pairs();
        if reverse {
            online.reverse();
        }

        let offline_suppressed = super::reports::suppress_contained_pairs(&mut offline);
        let online_suppressed = super::reports::retain_maximal_frontier_for_test(&mut online);
        offline.sort_by_key(|pair| pair.left().span().start());
        online.sort_by_key(|pair| pair.left().span().start());

        assert_eq!(online_suppressed, offline_suppressed);
        assert_eq!(online, offline);
    }
}

#[test]
fn maximal_overlap_retains_duplicate_span_pairs() {
    let pair = similarity_pair_report(
        1.0,
        1.0,
        report_form("a.lisp", 0, 10),
        report_form("b.lisp", 0, 10),
    );
    let mut pairs = vec![pair.clone(), pair];

    assert_eq!(super::reports::suppress_contained_pairs(&mut pairs), 0);
    assert_eq!(pairs.len(), 2);
}

#[test]
fn maximal_overlap_handles_large_non_overlapping_group() {
    const PAIR_COUNT: usize = 10_000;
    let mut pairs = (0..PAIR_COUNT)
        .map(|index| {
            let start = index * 2;
            similarity_pair_report(
                1.0,
                1.0,
                report_form("a.lisp", start, start + 1),
                report_form("b.lisp", start, start + 1),
            )
        })
        .collect();

    assert_eq!(super::reports::suppress_contained_pairs(&mut pairs), 0);
    assert_eq!(pairs.len(), PAIR_COUNT);
}

#[test]
fn max_results_truncates_only_reported_pairs() {
    let values = candidates("a.lisp", "(foo a) (foo b) (foo c)", 2);
    let report = build_similarity_pairs(values, 0.0, SimilarityOverlapPolicy::All, Some(1));
    assert_eq!(report.summary.matched_pairs(), 3);
    assert_eq!(report.summary.reported_pairs(), 1);
    assert!(report.summary.truncated());
    assert_eq!(report.pairs.len(), 1);

    let unlimited = build_similarity_pairs(
        candidates("a.lisp", "(foo a) (foo b) (foo c)", 2),
        0.0,
        SimilarityOverlapPolicy::All,
        None,
    );
    assert_eq!(
        report.summary.matched_pairs(),
        unlimited.summary.matched_pairs()
    );
    assert_eq!(unlimited.summary.reported_pairs(), 3);
    assert!(!unlimited.summary.truncated());
    assert_eq!(
        report.pairs[0].score().as_f64(),
        unlimited.pairs[0].score().as_f64()
    );
}

#[test]
fn bounded_results_count_matches_beyond_the_retained_capacity() {
    let values = vec![
        similarity_candidate("a.lisp", 3),
        similarity_candidate("a.lisp", 3),
    ];
    let (matched, exceeded, cancelled, retained_scores) =
        super::reports::bounded_result_scores_for_test(&values, &[1.0, 5.0, 2.0, 4.0, 3.0], 2);

    assert_eq!(matched, 5);
    assert!(!exceeded);
    assert!(!cancelled);
    assert_eq!(retained_scores, vec![5.0, 4.0]);
}

#[test]
fn zero_result_limit_counts_matches_without_reserving_storage() {
    let values = vec![
        similarity_candidate("a.lisp", 3),
        similarity_candidate("b.lisp", 3),
    ];
    let (matched, exceeded, cancelled, retained_scores) =
        super::reports::bounded_result_scores_for_test(&values, &[1.0, 5.0, 2.0], 0);

    assert_eq!(matched, 3);
    assert!(!exceeded);
    assert!(!cancelled);
    assert!(retained_scores.is_empty());
}

#[test]
fn cross_file_groups_keep_comparing_after_the_single_result_slot_is_full() {
    let values = ["a.lisp", "b.lisp", "c.lisp", "d.lisp"]
        .map(|path| similarity_candidate(path, 4))
        .into_iter()
        .collect();
    let report = super::reports::build_similarity_pairs(
        values,
        &report_options(
            0.0,
            4,
            1,
            SimilarityComparisonScope::CrossFile,
            SimilarityFormScope::All,
            SimilarityOverlapPolicy::All,
            None,
            None,
            Some(1),
        ),
    )
    .unwrap();

    assert_eq!(report.summary.matched_pairs(), 6);
    assert_eq!(report.summary.reported_pairs(), 1);
    assert!(report.summary.truncated());
}

#[test]
fn bounded_results_never_retain_more_than_the_requested_limit() {
    let values = vec![
        similarity_candidate("a.lisp", 3),
        similarity_candidate("b.lisp", 3),
    ];
    let scores = (0..200_000)
        .map(|index| ((index * 7919) % 100_003) as f64)
        .collect::<Vec<_>>();
    let (matched, peak_retained, budget_retained, cancelled) =
        super::reports::bounded_result_retention_for_test(&values, &scores, 127);

    assert_eq!(matched, scores.len());
    assert_eq!(peak_retained, 127);
    assert_eq!(budget_retained, 127);
    assert!(!cancelled);
}

#[test]
fn bounded_result_ties_are_deterministic_across_insertion_order() {
    let values = ["a.lisp", "b.lisp", "c.lisp", "d.lisp"].map(|path| similarity_candidate(path, 3));
    let forward = [(2, 3), (1, 3), (0, 3), (0, 2), (0, 1)];
    let reverse = forward.into_iter().rev().collect::<Vec<_>>();
    let forward_paths = super::reports::bounded_result_paths_for_test(&values, &forward, 3);
    let reverse_paths = super::reports::bounded_result_paths_for_test(&values, &reverse, 3);

    assert_eq!(forward_paths, reverse_paths);
    assert_eq!(
        forward_paths,
        vec![
            ("a.lisp".into(), "b.lisp".into()),
            ("a.lisp".into(), "c.lisp".into()),
            ("a.lisp".into(), "d.lisp".into()),
        ]
    );
}

#[test]
fn bounded_parallel_merge_is_independent_of_worker_merge_order() {
    let values = ["a.lisp", "b.lisp", "c.lisp", "d.lisp"].map(|path| similarity_candidate(path, 3));
    let first_worker = [(2, 3), (1, 3)];
    let second_worker = [(0, 3), (0, 2), (0, 1)];
    let worker_pairs = [first_worker.as_slice(), second_worker.as_slice()];
    let (forward_cancelled, forward_heap_len, forward_budget_retained, forward_paths) =
        super::reports::bounded_parallel_result_paths_for_test(&values, &worker_pairs, &[0, 1], 3);
    let (reverse_cancelled, reverse_heap_len, reverse_budget_retained, reverse_paths) =
        super::reports::bounded_parallel_result_paths_for_test(&values, &worker_pairs, &[1, 0], 3);

    assert!(!forward_cancelled);
    assert!(!reverse_cancelled);
    assert_eq!(forward_budget_retained, forward_heap_len);
    assert_eq!(reverse_budget_retained, reverse_heap_len);
    assert_eq!(forward_heap_len, 3);
    assert_eq!(reverse_heap_len, 3);
    assert_eq!(forward_paths, reverse_paths);
    assert_eq!(
        forward_paths,
        vec![
            ("a.lisp".into(), "b.lisp".into()),
            ("a.lisp".into(), "c.lisp".into()),
            ("a.lisp".into(), "d.lisp".into()),
        ]
    );
}

#[test]
fn max_comparisons_stops_ted_evaluation_and_tracks_unprocessed_pairs() {
    let report = super::reports::build_similarity_pairs(
        candidates("a.lisp", "(foo a) (foo b) (foo c)", 2),
        &report_options(
            0.0,
            4,
            1,
            SimilarityComparisonScope::All,
            SimilarityFormScope::All,
            SimilarityOverlapPolicy::All,
            None,
            Some(1),
            None,
        ),
    )
    .unwrap();

    assert_eq!(report.summary.possible_pairs(), 3);
    assert_eq!(report.summary.evaluated_pairs(), 1);
    assert_eq!(report.summary.pruned_by_size(), 0);
    assert_eq!(report.summary.unprocessed_pairs(), 2);
    assert!(report.summary.comparison_limit_reached());
    assert_eq!(report.summary.matched_pairs(), 1);
    assert_eq!(
        report.summary.possible_pairs(),
        report.summary.evaluated_pairs()
            + report.summary.pruned_by_size()
            + report.summary.unprocessed_pairs()
    );
}

#[test]
fn same_file_comparison_limit_uses_stable_path_order() {
    let mut values = candidates("z.lisp", "(foo a) (foo a)", 2);
    values.extend(candidates("a.lisp", "(foo a) (foo a)", 2));
    let options = report_options(
        1.0,
        2,
        1,
        SimilarityComparisonScope::SameFile,
        SimilarityFormScope::All,
        SimilarityOverlapPolicy::All,
        None,
        Some(1),
        None,
    );

    // Repeated construction catches randomized HashMap iteration deciding which
    // file consumes the single-comparison budget.
    for _ in 0..64 {
        let report = super::reports::build_similarity_pairs(values.clone(), &options).unwrap();

        assert_eq!(report.summary.evaluated_pairs(), 1);
        assert_eq!(report.pairs.len(), 1);
        assert_eq!(report.pairs[0].left().path(), PathBuf::from("a.lisp"));
        assert_eq!(report.pairs[0].right().path(), PathBuf::from("a.lisp"));
    }
}

#[test]
fn sufficient_max_comparisons_does_not_report_limit_reached() {
    let report = super::reports::build_similarity_pairs(
        candidates("a.lisp", "(foo a) (foo b) (foo c)", 2),
        &report_options(
            0.0,
            4,
            1,
            SimilarityComparisonScope::All,
            SimilarityFormScope::All,
            SimilarityOverlapPolicy::All,
            None,
            Some(3),
            Some(1),
        ),
    )
    .unwrap();

    assert_eq!(report.summary.evaluated_pairs(), 3);
    assert_eq!(report.summary.unprocessed_pairs(), 0);
    assert!(!report.summary.comparison_limit_reached());
    assert_eq!(report.summary.matched_pairs(), 3);
    assert_eq!(report.summary.reported_pairs(), 1);
}

#[test]
fn resource_budgets_enforce_candidate_and_planned_comparison_limits() {
    assert!(super::reports::validate_resource_budgets_for_test(100_000, 100_000_000, None).is_ok());
    assert!(super::reports::validate_resource_budgets_for_test(100_001, 0, None).is_err());
    assert!(super::reports::validate_resource_budgets_for_test(1, 100_000_001, None).is_err());
    assert!(super::reports::validate_resource_budgets_for_test(1, 100_000_001, Some(10)).is_ok());
}

#[test]
fn max_comparisons_counts_size_pruned_pairs_as_inspected() {
    for scope in [
        SimilarityComparisonScope::All,
        SimilarityComparisonScope::SameFile,
    ] {
        let values = candidates(
            "a.lisp",
            "(foo a) (foo a b c d) (foo x y z w) (foo p q r s)",
            2,
        );
        let report = super::reports::build_similarity_pairs(
            values,
            &report_options(
                0.6,
                4,
                1,
                scope,
                SimilarityFormScope::All,
                SimilarityOverlapPolicy::All,
                None,
                Some(1),
                None,
            ),
        )
        .unwrap();

        assert_eq!(report.summary.possible_pairs(), 6);
        assert_eq!(report.summary.evaluated_pairs(), 0);
        assert_eq!(report.summary.pruned_by_size(), 1);
        assert_eq!(report.summary.unprocessed_pairs(), 5);
        assert!(report.summary.comparison_limit_reached());
    }

    let mut cross_file_values = candidates("a.lisp", "(foo a)", 2);
    cross_file_values.extend(candidates("b.lisp", "(foo a b c d)", 2));
    cross_file_values.extend(candidates("c.lisp", "(foo x y z w)", 2));
    let report = super::reports::build_similarity_pairs(
        cross_file_values,
        &report_options(
            0.6,
            4,
            1,
            SimilarityComparisonScope::CrossFile,
            SimilarityFormScope::All,
            SimilarityOverlapPolicy::All,
            None,
            Some(1),
            None,
        ),
    )
    .unwrap();

    assert_eq!(report.summary.possible_pairs(), 3);
    assert_eq!(report.summary.evaluated_pairs(), 0);
    assert_eq!(report.summary.pruned_by_size(), 1);
    assert_eq!(report.summary.unprocessed_pairs(), 2);
    assert!(report.summary.comparison_limit_reached());
}

#[test]
fn candidate_limit_is_preserved_when_building_the_report() {
    let report = super::reports::build_similarity_pairs_with_omissions(
        candidates("a.lisp", "(foo a) (bar b) (baz c)", 2),
        1,
        &report_options(
            1.0,
            4,
            1,
            SimilarityComparisonScope::All,
            SimilarityFormScope::TopLevel,
            SimilarityOverlapPolicy::Maximal,
            None,
            None,
            None,
        ),
    )
    .unwrap();

    assert!(report.summary.candidate_limit_reached());
    assert_eq!(report.summary.omitted_candidates(), 1);
}

#[test]
fn different_heads_do_not_share_a_comparison_bucket() {
    let values = candidates("a.lisp", "(foo a b) (bar a b)", 2);
    let report = super::reports::build_similarity_pairs(
        values,
        &report_options(
            0.0,
            4,
            1,
            SimilarityComparisonScope::All,
            SimilarityFormScope::All,
            SimilarityOverlapPolicy::All,
            None,
            None,
            None,
        ),
    )
    .unwrap();

    assert_eq!(report.summary.possible_pairs(), 0);
    assert_eq!(report.summary.evaluated_pairs(), 0);
    assert!(report.pairs.is_empty());
}

#[test]
fn deeply_nested_candidate_collection_does_not_overflow() {
    const DEPTH: usize = 10_001;

    let input = format!("{}target{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
    let tree = SyntaxTree::parse(&input).expect("valid deeply nested input");
    let options = report_options(
        0.87,
        2,
        1,
        SimilarityComparisonScope::All,
        SimilarityFormScope::All,
        SimilarityOverlapPolicy::Maximal,
        Some(1),
        None,
        None,
    );
    let mut values = Vec::new();
    let omitted = collect_similarity_candidates(
        &tree,
        &input,
        FsPath::new("deep.lisp"),
        Dialect::CommonLisp,
        &options,
        &mut values,
    )
    .expect("collect deeply nested candidates");

    assert_eq!(values.len(), 1);
    assert_eq!(omitted, DEPTH - 1);
}

#[test]
fn candidate_collection_shares_input_and_applies_cumulative_budgets() {
    let input = "(outer (inner value)) (other value)";
    let tree = SyntaxTree::parse(input).unwrap();
    let options = report_options(
        0.87,
        2,
        1,
        SimilarityComparisonScope::All,
        SimilarityFormScope::All,
        SimilarityOverlapPolicy::Maximal,
        None,
        None,
        None,
    );
    let mut values = Vec::new();
    let omitted = super::collect::collect_similarity_candidates_with_budgets_for_test(
        &tree,
        input,
        FsPath::new("budget.lisp"),
        Dialect::CommonLisp,
        &options,
        &mut values,
        6,
        input.len(),
    )
    .unwrap();

    assert_eq!(values.len(), 1);
    assert_eq!(values[0].form().text(), "(outer (inner value))");
    assert!(omitted >= 2);

    let mut all_values = Vec::new();
    collect_similarity_candidates(
        &tree,
        input,
        FsPath::new("shared.lisp"),
        Dialect::CommonLisp,
        &options,
        &mut all_values,
    )
    .unwrap();
    assert!(
        all_values[0]
            .form()
            .text()
            .shares_source(all_values[1].form().text())
    );
}

#[test]
fn rejected_large_source_is_not_materialized_for_candidates() {
    let input = format!("{}(outer (inner value))", " ".repeat(1024 * 1024));
    let tree = SyntaxTree::parse(&input).unwrap();
    let options = report_options(
        0.87,
        2,
        1,
        SimilarityComparisonScope::All,
        SimilarityFormScope::All,
        SimilarityOverlapPolicy::Maximal,
        None,
        None,
        None,
    );
    let mut values = Vec::new();
    let (omitted, source_materialized) =
        super::collect::collect_similarity_candidates_materialization_for_test(
            &tree,
            &input,
            FsPath::new("large.lisp"),
            Dialect::CommonLisp,
            &options,
            &mut values,
            usize::MAX,
            input.len() - 1,
        )
        .unwrap();

    assert_eq!(values.len(), 0);
    assert_eq!(omitted, 2);
    assert!(!source_materialized);
}

#[test]
fn candidate_text_budget_counts_each_retained_source_once() {
    let first_input = format!("{}(outer (inner value))", " ".repeat(128));
    let second_input = format!("{}(other (nested value))", " ".repeat(128));
    let first_tree = SyntaxTree::parse(&first_input).unwrap();
    let second_tree = SyntaxTree::parse(&second_input).unwrap();
    let options = report_options(
        0.87,
        2,
        1,
        SimilarityComparisonScope::All,
        SimilarityFormScope::All,
        SimilarityOverlapPolicy::Maximal,
        None,
        None,
        None,
    );
    let mut values = Vec::new();
    let text_budget = first_input.len() + second_input.len() - 1;

    let first_omitted = super::collect::collect_similarity_candidates_with_budgets_for_test(
        &first_tree,
        &first_input,
        FsPath::new("first.lisp"),
        Dialect::CommonLisp,
        &options,
        &mut values,
        usize::MAX,
        text_budget,
    )
    .unwrap();
    let retained_after_first = values.len();
    let second_omitted = super::collect::collect_similarity_candidates_with_budgets_for_test(
        &second_tree,
        &second_input,
        FsPath::new("second.lisp"),
        Dialect::CommonLisp,
        &options,
        &mut values,
        usize::MAX,
        text_budget,
    )
    .unwrap();

    assert_eq!(first_omitted, 0);
    assert_eq!(retained_after_first, 2);
    assert_eq!(values.len(), retained_after_first);
    assert_eq!(second_omitted, 2);
    assert!(
        values[0]
            .form()
            .text()
            .shares_source(values[1].form().text())
    );
}
