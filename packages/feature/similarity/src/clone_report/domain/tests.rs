use std::path::{Path as FsPath, PathBuf};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::form_similarity::CloneType;
use crate::similarity_report::domain::{
    SimilarityCandidate, SimilarityComparisonScope, SimilarityFormScope, SimilarityOverlapPolicy,
    SimilarityReportOptions, collect_similarity_candidates,
};

use super::calibration::{build_histogram, largest_gap_threshold, otsu_threshold, percentile};
use super::external::size_window_for_test;
use super::*;

/// Every clone report starts from the same candidate collection the
/// `similarity` command uses, so the fixtures do too.
fn candidates(
    files: &[(&str, &str)],
    options: &SimilarityReportOptions,
) -> Vec<SimilarityCandidate> {
    let mut collected = Vec::new();
    for (name, source) in files {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        collect_similarity_candidates(
            &tree,
            source,
            FsPath::new(name),
            Dialect::CommonLisp,
            options,
            &mut collected,
        )
        .expect("collect candidates");
    }
    collected
}

fn top_level_options(threshold: f64) -> SimilarityReportOptions {
    SimilarityReportOptions::new(
        threshold,
        4,
        1,
        SimilarityComparisonScope::All,
        SimilarityFormScope::TopLevel,
        SimilarityOverlapPolicy::All,
        None,
        None,
        None,
    )
    .expect("valid options")
}

// ---------------------------------------------------------------- clone classes

#[test]
fn a_chain_of_similar_forms_becomes_one_class() {
    let options = top_level_options(0.8);
    let source = "\
(defun alpha (x) (+ x 1))
(defun beta (y) (+ y 2))
(defun gamma (z) (+ z 3))
";
    let report = build_clone_class_report(
        candidates(&[("a.lisp", source)], &options),
        0,
        &options,
        &CloneClassOptions::default(),
    )
    .expect("build classes");

    assert_eq!(report.classes.len(), 1);
    let class = &report.classes[0];
    assert_eq!(class.members.len(), 3);
    assert_eq!(class.clone_type, CloneType::Type2);
    assert!(class.consistent_renaming);
    assert_eq!(class.rank, 1);
    assert_eq!(report.unclassified_edges, 0);
}

#[test]
fn identical_forms_classify_the_class_as_type_1() {
    let options = top_level_options(0.9);
    let source = "(defun alpha (x) (+ x 1))\n(defun alpha (x) (+ x 1))\n";
    let report = build_clone_class_report(
        candidates(&[("a.lisp", source)], &options),
        0,
        &options,
        &CloneClassOptions::default(),
    )
    .expect("build classes");

    assert_eq!(report.classes.len(), 1);
    assert_eq!(report.classes[0].clone_type, CloneType::Type1);
    assert_eq!(report.classes[0].renamed_atoms, 0);
}

#[test]
fn the_class_type_is_the_loosest_relation_in_it() {
    // Two exact copies plus one reshaped body. The class as a whole is only as
    // tight as its loosest edge.
    let options = top_level_options(0.7);
    let source = "\
(defun alpha (x) (+ x 1))
(defun alpha (x) (+ x 1))
(defun alpha (x) (+ x 1 1))
";
    let report = build_clone_class_report(
        candidates(&[("a.lisp", source)], &options),
        0,
        &options,
        &CloneClassOptions::default(),
    )
    .expect("build classes");

    assert_eq!(report.classes.len(), 1);
    assert_eq!(report.classes[0].members.len(), 3);
    assert_eq!(report.classes[0].clone_type, CloneType::Type3);
    assert!(!report.classes[0].consistent_renaming);
}

#[test]
fn classes_rank_by_the_lines_extraction_would_save() {
    let options = top_level_options(0.8);
    // The long pair spans more lines than the short pair, so it must rank
    // first however the hash map happened to order the components.
    let source = "\
(defun short-a (x) (+ x 1))
(defun short-b (y) (+ y 2))
(defun long-a (x)
  (frobnicate x)
  (frobnicate x)
  (frobnicate x)
  (frobnicate x))
(defun long-b (y)
  (frobnicate y)
  (frobnicate y)
  (frobnicate y)
  (frobnicate y))
";
    let report = build_clone_class_report(
        candidates(&[("a.lisp", source)], &options),
        0,
        &options,
        &CloneClassOptions::default(),
    )
    .expect("build classes");

    assert_eq!(report.classes.len(), 2);
    assert!(report.classes[0].extraction.saved_lines > report.classes[1].extraction.saved_lines);
    assert_eq!(report.classes[0].rank, 1);
    assert_eq!(report.classes[1].rank, 2);
    assert_eq!(
        report.saved_lines(),
        report
            .classes
            .iter()
            .map(|class| class.extraction.saved_lines)
            .sum::<usize>()
    );
}

#[test]
fn extraction_estimate_accounts_for_the_helper_and_its_call_sites() {
    let members = [
        MemberSize {
            path: FsPath::new("a.lisp"),
            lines: 5,
            nodes: 20,
        },
        MemberSize {
            path: FsPath::new("b.lisp"),
            lines: 4,
            nodes: 18,
        },
    ];
    let estimate = ExtractionEstimate::from_sizes(&members, 1);

    assert_eq!(estimate.member_count, 2);
    assert_eq!(estimate.file_count, 2);
    assert_eq!(estimate.total_lines, 9);
    assert_eq!(estimate.representative_lines, 5);
    // 5 lines of helper body + 1 line of helper header + 2 call sites.
    assert_eq!(estimate.retained_lines, 8);
    assert_eq!(estimate.saved_lines, 1);

    // Extracting something that is nearly all overhead saves nothing, and the
    // estimate must say zero rather than underflow.
    let tiny = [
        MemberSize {
            path: FsPath::new("a.lisp"),
            lines: 1,
            nodes: 4,
        },
        MemberSize {
            path: FsPath::new("a.lisp"),
            lines: 1,
            nodes: 4,
        },
    ];
    let tiny_estimate = ExtractionEstimate::from_sizes(&tiny, 1);
    assert_eq!(tiny_estimate.saved_lines, 0);
    assert_eq!(tiny_estimate.file_count, 1);
}

#[test]
fn the_clone_type_filter_and_the_class_limit_are_reported_not_hidden() {
    let options = top_level_options(0.8);
    let source = "\
(defun alpha (x) (+ x 1))
(defun beta (y) (+ y 2))
(defun unrelated-longer (a b c) (list a b c))
(defun unrelated-longer (a b c) (list a b c))
";
    let all = build_clone_class_report(
        candidates(&[("a.lisp", source)], &options),
        0,
        &options,
        &CloneClassOptions::default(),
    )
    .expect("build classes");
    assert_eq!(all.classes.len(), 2);

    let only_type_1 = build_clone_class_report(
        candidates(&[("a.lisp", source)], &options),
        0,
        &options,
        &CloneClassOptions {
            clone_type: Some(CloneType::Type1),
            ..CloneClassOptions::default()
        },
    )
    .expect("build classes");
    assert_eq!(only_type_1.classes.len(), 1);
    assert_eq!(only_type_1.classes[0].clone_type, CloneType::Type1);
    assert_eq!(only_type_1.filtered_classes, 1);
    assert_eq!(only_type_1.total_classes, 2);

    let limited = build_clone_class_report(
        candidates(&[("a.lisp", source)], &options),
        0,
        &options,
        &CloneClassOptions {
            max_classes: Some(1),
            ..CloneClassOptions::default()
        },
    )
    .expect("build classes");
    assert_eq!(limited.classes.len(), 1);
    assert_eq!(limited.truncated_classes, 1);
}

#[test]
fn classes_span_files() {
    let options = top_level_options(0.8);
    let report = build_clone_class_report(
        candidates(
            &[
                ("a.lisp", "(defun alpha (x) (+ x 1))\n"),
                ("b.lisp", "(defun beta (y) (+ y 2))\n"),
            ],
            &options,
        ),
        0,
        &options,
        &CloneClassOptions::default(),
    )
    .expect("build classes");

    assert_eq!(report.classes.len(), 1);
    assert_eq!(report.classes[0].extraction.file_count, 2);
}

#[test]
fn nested_echo_classes_are_suppressed_and_counted() {
    // The duplicated `defun` contains a duplicated `let` containing a
    // duplicated `dolist`. One thing to extract, not three.
    let options = SimilarityReportOptions::new(
        0.8,
        4,
        1,
        SimilarityComparisonScope::All,
        SimilarityFormScope::All,
        SimilarityOverlapPolicy::All,
        None,
        None,
        None,
    )
    .expect("valid options");
    let source = "\
(defun alpha (x)
  (let ((total 0))
    (dolist (item x) (incf total item))
    total))
(defun beta (y)
  (let ((sum 0))
    (dolist (elem y) (incf sum elem))
    sum))
";

    let maximal = build_clone_class_report(
        candidates(&[("a.lisp", source)], &options),
        0,
        &options,
        &CloneClassOptions::default(),
    )
    .expect("build classes");
    assert_eq!(maximal.classes.len(), 1);
    assert_eq!(
        maximal.classes[0].members[0].head().map(AsRef::as_ref),
        Some("defun")
    );
    assert!(maximal.nested_classes >= 2);

    let all = build_clone_class_report(
        candidates(&[("a.lisp", source)], &options),
        0,
        &options,
        &CloneClassOptions {
            overlap_policy: ClassOverlapPolicy::All,
            ..CloneClassOptions::default()
        },
    )
    .expect("build classes");
    assert!(all.classes.len() > maximal.classes.len());
    assert_eq!(all.nested_classes, 0);
}

// ------------------------------------------------------------- clone sequences

fn sequence_report(
    files: &[(&str, &str)],
    options: &CloneSequenceOptions,
) -> Result<CloneSequenceReport, CloneSequenceOptionsError> {
    let parsed = files
        .iter()
        .map(|(name, source)| {
            (
                PathBuf::from(name),
                SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse"),
                *source,
            )
        })
        .collect::<Vec<_>>();
    let sources = parsed
        .iter()
        .map(|(path, tree, text)| SequenceSource {
            path: path.as_path(),
            dialect: Dialect::CommonLisp,
            tree,
            text,
        })
        .collect::<Vec<_>>();
    build_clone_sequence_report(&sources, options)
}

#[test]
fn a_run_shared_by_two_bodies_is_found_where_no_whole_form_matches() {
    let source = "\
(defun create (request)
  (validate request)
  (normalize request)
  (audit request)
  (persist request))
(defun update (request id)
  (assert-exists id)
  (validate request)
  (normalize request)
  (audit request)
  (merge-into id request))
";
    let report = sequence_report(&[("svc.lisp", source)], &CloneSequenceOptions::default())
        .expect("build sequences");

    assert_eq!(report.groups.len(), 1);
    let group = &report.groups[0];
    assert_eq!(group.run_length, 3);
    assert_eq!(group.occurrences.len(), 2);
    assert_eq!(group.clone_type, CloneType::Type1);
    // The run starts at a different child index in each body, which is exactly
    // why no whole-form report can see it.
    assert_eq!(group.occurrences[0].first_child_index, 3);
    assert_eq!(group.occurrences[1].first_child_index, 4);
}

#[test]
fn runs_inside_forms_that_are_themselves_clones_are_left_to_the_class_report() {
    // alpha and beta are whole-form clones. The shared run inside them is not
    // independent news, and `clone-classes` reports the forms better.
    let source = "\
(defun alpha (x)
  (validate x)
  (normalize x)
  (persist x))
(defun beta (y)
  (validate y)
  (normalize y)
  (persist y))
";
    let default = sequence_report(&[("a.lisp", source)], &CloneSequenceOptions::default())
        .expect("build sequences");
    assert!(default.groups.is_empty(), "{:?}", default.groups);
    // Counted, not silently dropped: "no partial duplication" and "all of it
    // was whole-form duplication" are different answers.
    assert!(default.suppressed_parent_clone_groups > 0);

    let included = sequence_report(
        &[("a.lisp", source)],
        &CloneSequenceOptions {
            include_parent_clones: true,
            ..CloneSequenceOptions::default()
        },
    )
    .expect("build sequences");
    assert!(!included.groups.is_empty());
}

#[test]
fn repetition_inside_one_form_is_never_suppressed_as_a_parent_clone() {
    // There is only one enclosing form here, so it cannot be a clone of
    // anything, and the run repeating inside it is exactly what to report.
    let source = "\
(defun alpha (x)
  (validate x)
  (normalize x)
  (persist x)
  (reset x)
  (validate x)
  (normalize x)
  (persist x))
";
    let report = sequence_report(&[("a.lisp", source)], &CloneSequenceOptions::default())
        .expect("build sequences");

    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].run_length, 3);
    assert_eq!(report.groups[0].occurrences.len(), 2);
}

#[test]
fn a_run_of_same_shaped_calls_with_different_heads_is_not_a_clone() {
    // Every form here is a two-element call, so an atom-erased fingerprint
    // matches them all. Only the head check separates them.
    let source = "\
(defun alpha (x)
  (first-thing x)
  (second-thing x)
  (third-thing x)
  (done x))
(defun beta (y)
  (fourth-thing y)
  (fifth-thing y)
  (sixth-thing y)
  (finished y))
";
    let report = sequence_report(&[("a.lisp", source)], &CloneSequenceOptions::default())
        .expect("build sequences");

    assert!(report.groups.is_empty(), "{:?}", report.groups);
}

#[test]
fn exact_mode_refuses_a_renamed_run_that_renamed_mode_accepts() {
    let source = "\
(defun alpha (x)
  (guard x)
  (validate x)
  (normalize x)
  (audit x)
  (persist x))
(defun beta (y)
  (validate y)
  (normalize y)
  (audit y)
  (finish y))
";
    let renamed = sequence_report(&[("a.lisp", source)], &CloneSequenceOptions::default())
        .expect("build sequences");
    assert_eq!(renamed.groups.len(), 1);
    assert_eq!(renamed.groups[0].run_length, 3);
    assert_eq!(renamed.groups[0].clone_type, CloneType::Type2);

    let exact = sequence_report(
        &[("a.lisp", source)],
        &CloneSequenceOptions {
            match_mode: SequenceMatchMode::Exact,
            ..CloneSequenceOptions::default()
        },
    )
    .expect("build sequences");
    assert!(exact.groups.is_empty());
}

#[test]
fn overlapping_occurrences_of_one_run_are_counted_once() {
    // Six identical statements contain four overlapping runs of three. There
    // are not four independent copies there, and counting them would overstate
    // both the duplication and what removing it saves.
    let source = "\
(defun alpha (x)
  (step x)
  (step x)
  (step x)
  (step x)
  (step x)
  (step x)
  (finish x))
";
    let report = sequence_report(
        &[("a.lisp", source)],
        &CloneSequenceOptions {
            match_mode: SequenceMatchMode::Exact,
            ..CloneSequenceOptions::default()
        },
    )
    .expect("build sequences");

    for group in &report.groups {
        let mut spans = group
            .occurrences
            .iter()
            .map(|occurrence| {
                (
                    occurrence.first_child_index,
                    occurrence.first_child_index + occurrence.run_length,
                )
            })
            .collect::<Vec<_>>();
        spans.sort_unstable();
        for window in spans.windows(2) {
            assert!(
                window[0].1 <= window[1].0,
                "overlapping occurrences reported: {spans:?}"
            );
        }
    }
}

#[test]
fn sequence_options_reject_impossible_bounds() {
    let base = CloneSequenceOptions::default();
    for (options, expected) in [
        (
            CloneSequenceOptions {
                min_run_length: 1,
                ..base
            },
            CloneSequenceOptionsError::MinRunLengthTooSmall,
        ),
        (
            CloneSequenceOptions {
                min_run_length: 5,
                max_run_length: 4,
                ..base
            },
            CloneSequenceOptionsError::MaxRunLengthBelowMin,
        ),
        (
            CloneSequenceOptions {
                max_run_length: MAX_SUPPORTED_RUN_LENGTH + 1,
                ..base
            },
            CloneSequenceOptionsError::MaxRunLengthTooLarge,
        ),
        (
            CloneSequenceOptions {
                min_occurrences: 1,
                ..base
            },
            CloneSequenceOptionsError::MinOccurrencesTooSmall,
        ),
        (
            CloneSequenceOptions {
                max_groups: Some(0),
                ..base
            },
            CloneSequenceOptionsError::MaxGroupsTooSmall,
        ),
    ] {
        assert_eq!(sequence_report(&[], &options).unwrap_err(), expected);
    }
}

// -------------------------------------------------------------- clone external

#[test]
fn external_matching_crosses_head_symbols() {
    // `similarity` buckets by head and would never compare these two. Finding
    // them anyway is the whole point of the cross-corpus pass.
    let options = top_level_options(0.8);
    let project = candidates(
        &[("src/util.lisp", "(defun my-join (a b) (fold a b))\n")],
        &options,
    );
    let reference = candidates(
        &[("vendor/lib.lisp", "(defun str-join (x y) (fold x y))\n")],
        &options,
    );

    let report =
        build_clone_external_report(project, reference, 1, 1, &options).expect("build external");

    assert_eq!(report.matches.len(), 1);
    assert_eq!(report.matches[0].clone_type, CloneType::Type2);
    assert_eq!(
        report.matches[0].project.path(),
        FsPath::new("src/util.lisp")
    );
    assert_eq!(
        report.matches[0].reference.path(),
        FsPath::new("vendor/lib.lisp")
    );
}

#[test]
fn external_matching_never_pairs_a_corpus_with_itself() {
    let options = top_level_options(0.8);
    let project = candidates(
        &[(
            "src/util.lisp",
            "(defun alpha (x) (+ x 1))\n(defun beta (y) (+ y 2))\n",
        )],
        &options,
    );

    let report =
        build_clone_external_report(project, Vec::new(), 1, 0, &options).expect("build external");

    assert!(report.matches.is_empty());
    assert_eq!(report.possible_pairs, 0);
    assert_eq!(report.evaluated_pairs, 0);
}

#[test]
fn external_matches_are_bounded_and_report_what_was_cut() {
    // Every project form matches every reference form here, so an unbounded
    // report would be the product of the two corpora.
    let options = SimilarityReportOptions::new(
        0.8,
        4,
        1,
        SimilarityComparisonScope::All,
        SimilarityFormScope::TopLevel,
        SimilarityOverlapPolicy::All,
        None,
        None,
        Some(3),
    )
    .expect("valid options");
    let mut project_source = String::new();
    let mut reference_source = String::new();
    for index in 0..6 {
        project_source.push_str(&format!("(defun mine{index} (a b) (fold a b))\n"));
        reference_source.push_str(&format!("(defun theirs{index} (x y) (fold x y))\n"));
    }

    let report = build_clone_external_report(
        candidates(&[("src/util.lisp", &project_source)], &options),
        candidates(&[("refs/lib.lisp", &reference_source)], &options),
        1,
        1,
        &options,
    )
    .expect("build external");

    assert_eq!(report.matches.len(), 3);
    assert_eq!(report.matched_pairs, 36);
    assert!(report.truncated);
    // What survives the cut is the best of what was found, in rank order.
    for window in report.matches.windows(2) {
        assert!(window[0].similarity >= window[1].similarity);
    }
}

#[test]
fn the_size_window_is_the_widest_one_the_threshold_allows() {
    let sizes = [2usize, 4, 8, 10, 12, 20, 40];

    // At t = 0.8 a 10-node form can only reach 8..=12 nodes.
    let window = size_window_for_test(&sizes, 10, 0.8);
    assert_eq!(&sizes[window], &[8, 10, 12]);

    // A threshold of 1.0 admits only the exact size.
    let exact = size_window_for_test(&sizes, 10, 1.0);
    assert_eq!(&sizes[exact], &[10]);

    // A threshold of 0 disables the window rather than emptying it.
    let everything = size_window_for_test(&sizes, 10, 0.0);
    assert_eq!(&sizes[everything], &sizes);

    // Nothing in range yields an empty, non-inverted window.
    let empty = size_window_for_test(&sizes, 1000, 0.95);
    assert!(empty.is_empty());
    assert!(empty.start <= empty.end);
}

// -------------------------------------------------------- threshold calibration

fn histogram_from(counts: &[usize], floor: f64, width: f64) -> SimilarityHistogram {
    let calibration = CloneThresholdOptions {
        floor,
        bucket_width: width,
        ..CloneThresholdOptions::default()
    };
    let scores = counts
        .iter()
        .enumerate()
        .flat_map(|(index, &count)| {
            std::iter::repeat_n(floor + (index as f64 + 0.5) * width, count)
        })
        .collect::<Vec<_>>();
    build_histogram(&scores, &calibration)
}

#[test]
fn otsu_splits_a_two_population_distribution_between_them() {
    // A heavy low cluster, an empty middle, a light high cluster.
    let histogram = histogram_from(&[40, 40, 0, 0, 0, 0, 0, 0, 10, 10], 0.5, 0.05);
    let threshold = otsu_threshold(&histogram).expect("two populations split");

    assert!(
        (0.6..=0.9).contains(&threshold),
        "otsu put the split at {threshold}"
    );
}

#[test]
fn otsu_declines_when_there_is_nothing_to_split() {
    assert_eq!(otsu_threshold(&histogram_from(&[0, 0, 0], 0.5, 0.1)), None);
    assert_eq!(otsu_threshold(&histogram_from(&[7, 0, 0], 0.5, 0.1)), None);
}

#[test]
fn the_largest_gap_is_the_widest_hole_between_two_populated_buckets() {
    let histogram = histogram_from(&[5, 0, 3, 0, 0, 0, 4, 0], 0.5, 0.05);
    let (width, threshold) = largest_gap_threshold(&histogram).expect("a gap exists");

    assert_eq!(width, 3);
    // The threshold is the far side of the hole — the bottom edge of the next
    // populated bucket — so everything at or above it is upper population.
    assert!((threshold - 0.80).abs() < 1e-9, "{threshold}");

    // Trailing empty buckets are not a gap: nothing populated sits above them.
    assert_eq!(
        largest_gap_threshold(&histogram_from(&[5, 0, 0], 0.5, 0.1)),
        None
    );
}

#[test]
fn percentiles_use_nearest_rank_and_survive_the_edges() {
    let scores = [0.1, 0.2, 0.3, 0.4, 0.5];

    assert_eq!(percentile(&scores, 50), Some(0.3));
    assert_eq!(percentile(&scores, 100), Some(0.5));
    assert_eq!(percentile(&scores, 0), Some(0.1));
    assert_eq!(percentile(&[], 50), None);
}

#[test]
fn a_sample_too_small_to_support_a_recommendation_falls_back_to_the_default() {
    let options = top_level_options(0.87);
    let mut source = String::new();
    for index in 0..12 {
        source.push_str(&format!("(defun name{index} (x) (+ x {index} {index}))\n"));
    }
    source.push_str("(defun other-a (p q r) (list p q r (list p q r)))\n");
    source.push_str("(defun other-b (s u v) (list s u v (list s u v)))\n");

    let report = build_clone_threshold_report(
        candidates(&[("a.lisp", &source)], &options),
        &options,
        &CloneThresholdOptions::default(),
    )
    .expect("build threshold report");
    assert!(report.scored_pairs > 0);
    assert!(
        report
            .candidates
            .iter()
            .any(|candidate| candidate.method == ThresholdMethod::Otsu)
    );

    let unsupported = build_clone_threshold_report(
        candidates(&[("a.lisp", &source)], &options),
        &options,
        &CloneThresholdOptions {
            min_sample: usize::MAX,
            ..CloneThresholdOptions::default()
        },
    )
    .expect("build threshold report");
    assert!(!unsupported.well_supported);
    assert_eq!(unsupported.recommended.method, ThresholdMethod::Default);
    assert!((unsupported.recommended.threshold - 0.87).abs() < 1e-9);
}

#[test]
fn a_higher_candidate_threshold_never_admits_more_pairs() {
    let options = top_level_options(0.87);
    let source = "\
(defun alpha (x) (+ x 1))
(defun beta (y) (+ y 2))
(defun gamma (a b) (list a b))
(defun delta (c d) (list c d))
";
    let report = build_clone_threshold_report(
        candidates(&[("a.lisp", source)], &options),
        &options,
        &CloneThresholdOptions::default(),
    )
    .expect("build threshold report");

    let mut sorted = report.candidates.clone();
    sorted.sort_by(|left, right| {
        left.threshold
            .partial_cmp(&right.threshold)
            .expect("thresholds are finite")
    });
    for window in sorted.windows(2) {
        assert!(
            window[0].pairs_at_or_above >= window[1].pairs_at_or_above,
            "{:?} then {:?}",
            window[0],
            window[1]
        );
    }
}

// ------------------------------------------------------------- clone genealogy

/// A history port backed by a fixture, so the ordering is tested without a
/// repository and without `git` on `PATH`.
struct FixedHistory(Vec<(PathBuf, i64)>);

impl CloneHistoryPort for FixedHistory {
    fn oldest_commit(
        &self,
        path: &FsPath,
        _lines: std::ops::RangeInclusive<usize>,
    ) -> MemberHistory {
        self.0.iter().find(|(known, _)| known == path).map_or_else(
            || MemberHistory::unavailable("not tracked"),
            |(_, timestamp)| MemberHistory {
                commit: Some(format!("commit{timestamp}")),
                author: Some("Author".to_owned()),
                date: Some("2024-01-01T00:00:00Z".to_owned()),
                timestamp: Some(*timestamp),
                unavailable: None,
            },
        )
    }
}

const DAY: i64 = 86_400;

fn two_file_classes() -> CloneClassReport {
    let options = top_level_options(0.8);
    build_clone_class_report(
        candidates(
            &[
                ("old.lisp", "(defun alpha (x) (+ x 1))\n"),
                ("new.lisp", "(defun beta (y) (+ y 2))\n"),
            ],
            &options,
        ),
        0,
        &options,
        &CloneClassOptions::default(),
    )
    .expect("build classes")
}

#[test]
fn the_oldest_member_is_the_origin_and_the_rest_carry_their_lag() {
    let classes = two_file_classes();
    let history = FixedHistory(vec![
        (PathBuf::from("new.lisp"), 100 * DAY),
        (PathBuf::from("old.lisp"), 10 * DAY),
    ]);

    let report = build_clone_genealogy_report(&classes, &history);

    assert_eq!(report.genealogies.len(), 1);
    let genealogy = &report.genealogies[0];
    assert_eq!(genealogy.members.len(), 2);
    assert_eq!(genealogy.members[0].role, CloneOrigin::Origin);
    assert_eq!(genealogy.members[0].form.path(), FsPath::new("old.lisp"));
    assert_eq!(genealogy.members[0].lag_days, None);
    assert_eq!(genealogy.members[1].role, CloneOrigin::Copy);
    assert_eq!(genealogy.members[1].lag_days, Some(90));
    assert_eq!(genealogy.span_days, Some(90));
    assert_eq!(genealogy.origin_commit.as_deref(), Some("commit864000"));
    assert_eq!(report.dated_members, 2);
    assert_eq!(report.undated_members, 0);
}

#[test]
fn an_undated_member_is_reported_as_unknown_rather_than_guessed_at() {
    let classes = two_file_classes();
    let history = FixedHistory(vec![(PathBuf::from("old.lisp"), 10 * DAY)]);

    let report = build_clone_genealogy_report(&classes, &history);
    let genealogy = &report.genealogies[0];

    assert_eq!(genealogy.dated_members, 1);
    assert_eq!(genealogy.undated_members, 1);
    // One dated member is not two dates, so there is no span to report.
    assert_eq!(genealogy.span_days, None);
    assert_eq!(
        genealogy
            .members
            .iter()
            .find(|member| member.form.path() == FsPath::new("new.lisp"))
            .expect("the untracked member")
            .role,
        CloneOrigin::Unknown
    );
    assert_eq!(report.unavailable_reasons, vec!["not tracked".to_owned()]);
}

#[test]
fn a_class_committed_all_at_once_reports_exactly_one_origin() {
    let classes = two_file_classes();
    let history = FixedHistory(vec![
        (PathBuf::from("old.lisp"), 10 * DAY),
        (PathBuf::from("new.lisp"), 10 * DAY),
    ]);

    let report = build_clone_genealogy_report(&classes, &history);
    let origins = report.genealogies[0]
        .members
        .iter()
        .filter(|member| member.role == CloneOrigin::Origin)
        .count();

    assert_eq!(origins, 1);
    assert_eq!(report.genealogies[0].span_days, Some(0));
}

// -------------------------------------------------------------------- shared

#[test]
fn line_spans_count_the_lines_a_form_occupies() {
    assert_eq!(line_span_of("(f x)"), 1);
    assert_eq!(line_span_of("(f\n  x)"), 2);
    assert_eq!(line_span_of("(f\n  x\n  y)"), 3);
    assert_eq!(line_span_of(""), 1);
}

#[test]
fn form_keys_order_by_location() {
    let options = top_level_options(0.8);
    let collected = candidates(
        &[(
            "a.lisp",
            "(defun alpha (x) (+ x 1))\n(defun beta (y) (+ y 2))\n",
        )],
        &options,
    );
    let mut keys = collected
        .iter()
        .map(|candidate| FormKey::of(candidate.form()))
        .collect::<Vec<_>>();
    let mut sorted = keys.clone();
    sorted.sort();
    keys.sort();

    assert_eq!(keys, sorted);
    assert!(keys[0].start() < keys[1].start());
    assert_eq!(keys[0].path(), FsPath::new("a.lisp"));
}
