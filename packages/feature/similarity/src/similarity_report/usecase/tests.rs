use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use crate::domain::dialect::Dialect;
use crate::domain::similarity_report::{
    PairProcessingCounts, PairResultCounts, ReportLimit, SimilarityReport, SimilarityReportSummary,
};

use super::SimilarityReportOptions;
use super::types::{
    DiscoveredSimilarityFile, InvalidSimilarityReportPlan, SimilarityDuplicatePolicy,
    SimilarityErrorPolicy, SimilarityFileError, SimilarityGateDecision,
    SimilarityIndeterminateReason, SimilarityInventory, SimilarityProcessingStage,
    SimilarityReportPlan, SimilarityReportRequest, SimilarityReportSourcePort,
    SimilarityReportWorkflowError,
};
use super::workflow::build_similarity_report;

#[derive(Clone)]
struct FakeSource {
    inventory: SimilarityInventory,
    files: BTreeMap<PathBuf, Result<Vec<u8>, String>>,
    parallelism: NonZeroUsize,
}

impl SimilarityReportSourcePort for FakeSource {
    fn discover(
        &mut self,
        _request: &SimilarityReportRequest,
    ) -> anyhow::Result<SimilarityInventory> {
        Ok(self.inventory.clone())
    }

    fn load(&self, file: &DiscoveredSimilarityFile) -> Result<Vec<u8>, String> {
        self.files[&file.path].clone()
    }

    fn available_parallelism(&self) -> NonZeroUsize {
        self.parallelism
    }
}

fn request(
    error_policy: SimilarityErrorPolicy,
    duplicate_policy: SimilarityDuplicatePolicy,
) -> SimilarityReportRequest {
    SimilarityReportRequest {
        roots: Vec::new(),
        include_unknown: false,
        include_hidden: false,
        include_generated: false,
        max_depth: None,
        exclude: Vec::new(),
        forced_dialect: None,
        options: SimilarityReportOptions::default(),
        error_policy,
        duplicate_policy,
    }
}

fn source(entries: Vec<(&str, Result<Vec<u8>, &str>)>, parallelism: usize) -> FakeSource {
    let paths = entries
        .iter()
        .map(|(path, _)| DiscoveredSimilarityFile {
            path: PathBuf::from(path),
            dialect: Dialect::CommonLisp,
        })
        .collect::<Vec<_>>();
    let files = entries
        .into_iter()
        .map(|(path, bytes)| {
            (
                PathBuf::from(path),
                bytes.map_err(std::string::ToString::to_string),
            )
        })
        .collect();
    FakeSource {
        inventory: SimilarityInventory {
            files: paths,
            ..SimilarityInventory::default()
        },
        files,
        parallelism: NonZeroUsize::new(parallelism).expect("parallelism must be non-zero"),
    }
}

#[test]
fn parallelism_does_not_change_the_plan() {
    let entries = vec![
        ("a.lisp", Ok(b"(defun alpha (x) (+ x 1))".to_vec())),
        ("b.lisp", Ok(b"(defun alpha (x) (+ x 1))".to_vec())),
        ("c.lisp", Ok(b"(defun beta (x) (* x 2))".to_vec())),
    ];
    let mut serial = source(entries.clone(), 1);
    let mut parallel = source(entries, 8);

    let serial_plan = build_similarity_report(
        &mut serial,
        request(SimilarityErrorPolicy::Skip, SimilarityDuplicatePolicy::Fail),
    )
    .expect("serial report succeeds");
    let parallel_plan = build_similarity_report(
        &mut parallel,
        request(SimilarityErrorPolicy::Skip, SimilarityDuplicatePolicy::Fail),
    )
    .expect("parallel report succeeds");

    assert_eq!(serial_plan, parallel_plan);
}

#[test]
fn fail_policy_reports_first_discovery_error_regardless_of_parallelism() {
    let entries = vec![
        ("first.lisp", Err("first failure")),
        ("valid.lisp", Ok(b"(list 1 2 3)".to_vec())),
        ("last.lisp", Err("last failure")),
    ];

    for parallelism in [1, 8] {
        let mut source = source(entries.clone(), parallelism);
        let error = build_similarity_report(
            &mut source,
            request(
                SimilarityErrorPolicy::Fail,
                SimilarityDuplicatePolicy::Ignore,
            ),
        )
        .expect_err("fail policy rejects the first processing error");

        match error {
            SimilarityReportWorkflowError::Processing(error) => {
                assert_eq!(error.path, PathBuf::from("first.lisp"));
                assert_eq!(error.stage, SimilarityProcessingStage::Read);
                assert_eq!(error.message, "first failure");
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}

#[test]
fn skip_policy_collects_errors_in_discovery_order_with_typed_stages() {
    let mut source = source(
        vec![
            ("read.lisp", Err("unreadable")),
            ("decode.lisp", Ok(vec![0xff])),
            ("parse.lisp", Ok(b"(".to_vec())),
        ],
        8,
    );

    let plan = build_similarity_report(
        &mut source,
        request(SimilarityErrorPolicy::Skip, SimilarityDuplicatePolicy::Fail),
    )
    .expect("skip policy returns a plan");

    assert_eq!(
        plan.errors()
            .iter()
            .map(|error| (&error.path, error.stage))
            .collect::<Vec<_>>(),
        vec![
            (&PathBuf::from("read.lisp"), SimilarityProcessingStage::Read),
            (
                &PathBuf::from("decode.lisp"),
                SimilarityProcessingStage::Decode
            ),
            (
                &PathBuf::from("parse.lisp"),
                SimilarityProcessingStage::Parse
            ),
        ]
    );
    assert_eq!(
        plan.gate(),
        &SimilarityGateDecision::Indeterminate(SimilarityIndeterminateReason::ProcessingErrors {
            file_count: 3
        })
    );
}

#[test]
fn gate_precedence_is_duplicates_then_limits_then_errors() {
    let processing_error = SimilarityFileError {
        path: PathBuf::from("bad.lisp"),
        stage: SimilarityProcessingStage::Read,
        message: "failed".to_owned(),
    };
    let inventory = || SimilarityInventory {
        files: vec![DiscoveredSimilarityFile {
            path: PathBuf::from("bad.lisp"),
            dialect: Dialect::CommonLisp,
        }],
        ..SimilarityInventory::default()
    };
    let plan = |report, error| {
        SimilarityReportPlan::new(
            report,
            inventory(),
            vec![error],
            SimilarityDuplicatePolicy::Fail,
        )
        .expect("test plan satisfies inventory invariants")
    };

    assert_eq!(
        plan(report_with_summary(1, true, true), processing_error.clone()).gate(),
        &SimilarityGateDecision::DuplicateFound { matched_pairs: 1 }
    );

    assert!(matches!(
        plan(report_with_summary(0, true, true), processing_error.clone()).gate(),
        SimilarityGateDecision::Indeterminate(
            SimilarityIndeterminateReason::ComparisonLimit { .. }
        )
    ));

    assert!(matches!(
        plan(
            report_with_summary(0, false, true),
            processing_error.clone()
        )
        .gate(),
        SimilarityGateDecision::Indeterminate(SimilarityIndeterminateReason::CandidateLimit { .. })
    ));

    assert!(matches!(
        plan(report_with_summary(0, false, false), processing_error).gate(),
        SimilarityGateDecision::Indeterminate(
            SimilarityIndeterminateReason::ProcessingErrors { .. }
        )
    ));
}

#[test]
fn plan_rejects_inventory_error_path_inconsistencies() {
    let report = || report_with_summary(0, false, false);
    let file = |path: &str| DiscoveredSimilarityFile {
        path: PathBuf::from(path),
        dialect: Dialect::CommonLisp,
    };
    let error = |path: &str| SimilarityFileError {
        path: PathBuf::from(path),
        stage: SimilarityProcessingStage::Read,
        message: "failed".to_owned(),
    };
    let inventory = |paths: &[&str]| SimilarityInventory {
        files: paths.iter().map(|path| file(path)).collect(),
        ..SimilarityInventory::default()
    };

    assert!(matches!(
        SimilarityReportPlan::new(
            report(),
            inventory(&["a.lisp", "a.lisp"]),
            Vec::new(),
            SimilarityDuplicatePolicy::Ignore,
        ),
        Err(InvalidSimilarityReportPlan::DuplicateInventoryPath { path })
            if path.as_path() == Path::new("a.lisp")
    ));
    assert!(matches!(
        SimilarityReportPlan::new(
            report(),
            inventory(&["a.lisp"]),
            vec![error("missing.lisp")],
            SimilarityDuplicatePolicy::Ignore,
        ),
        Err(InvalidSimilarityReportPlan::UnknownErrorPath { path })
            if path.as_path() == Path::new("missing.lisp")
    ));
    assert!(matches!(
        SimilarityReportPlan::new(
            report(),
            inventory(&["a.lisp"]),
            vec![error("a.lisp"), error("a.lisp")],
            SimilarityDuplicatePolicy::Ignore,
        ),
        Err(InvalidSimilarityReportPlan::DuplicateErrorPath { path })
            if path.as_path() == Path::new("a.lisp")
    ));
    assert!(matches!(
        SimilarityReportPlan::new(
            report(),
            inventory(&["a.lisp", "b.lisp"]),
            vec![error("b.lisp"), error("a.lisp")],
            SimilarityDuplicatePolicy::Ignore,
        ),
        Err(InvalidSimilarityReportPlan::OutOfOrderError {
            previous_path,
            path,
        }) if previous_path.as_path() == Path::new("b.lisp")
            && path.as_path() == Path::new("a.lisp")
    ));
}

#[test]
fn processing_stage_labels_are_stable() {
    assert_eq!(SimilarityProcessingStage::Read.label(), "read");
    assert_eq!(SimilarityProcessingStage::Decode.label(), "decode");
    assert_eq!(SimilarityProcessingStage::Parse.label(), "parse");
    assert_eq!(SimilarityProcessingStage::Collect.label(), "collect");
}

fn report_with_summary(
    matched_pairs: usize,
    comparison_limit_reached: bool,
    candidate_limit_reached: bool,
) -> SimilarityReport {
    SimilarityReport::new(
        SimilarityReportSummary::new(
            ReportLimit::from_omitted(if candidate_limit_reached { 2 } else { 0 }),
            PairProcessingCounts::new(if comparison_limit_reached { 4 } else { 1 }, 1, 0, 0)
                .unwrap(),
            ReportLimit::from_omitted(if comparison_limit_reached { 3 } else { 0 }),
            PairResultCounts::new(matched_pairs, 0, 0).unwrap(),
        )
        .unwrap(),
        Vec::new(),
    )
    .unwrap()
}
