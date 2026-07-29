use std::collections::BTreeMap;
use std::hint::black_box;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use paredit_cli::application::usecase::similarity_report::{
    DiscoveredSimilarityFile, SimilarityComparisonScope, SimilarityDuplicatePolicy,
    SimilarityErrorPolicy, SimilarityFormScope, SimilarityInventory, SimilarityOverlapPolicy,
    SimilarityReportOptions, SimilarityReportPlan, SimilarityReportRequest,
    SimilarityReportSourcePort, build_similarity_report,
};
use paredit_cli::domain::dialect::Dialect;

/// The two ends of the range, and deliberately nothing between them — see the
/// same constant in `benches/lint_report.rs`. It matters more here, where the
/// scenarios are about pruning behaviour that is quadratic in the candidate
/// count: 32 and 128 bracket that curve, and 64 sat on the line between them.
const INPUT_SIZES: [usize; 2] = [32, 128];
const RETENTION_MODES: [(Option<usize>, &str); 2] =
    [(Some(1), "top1-retention"), (None, "full-retention")];

#[derive(Clone)]
struct FixtureSource {
    inventory: SimilarityInventory,
    files: BTreeMap<PathBuf, Vec<u8>>,
}

impl SimilarityReportSourcePort for FixtureSource {
    // A fixture cannot fail to discover; the port still asks for a type.
    type Error = paredit_core_cli::CliError;

    fn discover(
        &mut self,
        _request: &SimilarityReportRequest,
    ) -> Result<SimilarityInventory, Self::Error> {
        Ok(self.inventory.clone())
    }

    fn load(&self, file: &DiscoveredSimilarityFile) -> Result<Vec<u8>, String> {
        Ok(self.files[&file.path].clone())
    }

    fn available_parallelism(&self) -> NonZeroUsize {
        NonZeroUsize::MIN
    }
}

#[derive(Clone, Copy)]
enum Scenario {
    RepeatedShape,
    NodeCountPruned,
    LeafCountPruned,
}

impl Scenario {
    const fn label(self) -> &'static str {
        match self {
            Self::RepeatedShape => "repeated-shape",
            Self::NodeCountPruned => "node-count-pruned",
            Self::LeafCountPruned => "leaf-count-pruned",
        }
    }

    fn source(self, candidate_count: usize) -> FixtureSource {
        assert_eq!(
            candidate_count % 2,
            0,
            "pruning fixtures require two equal groups"
        );

        let entries = (0..candidate_count).map(|index| {
            let body = match self {
                Self::RepeatedShape => "(defun shared (x) (+ x 1))".to_owned(),
                Self::NodeCountPruned if index < candidate_count / 2 => sized_defun(4),
                Self::NodeCountPruned => sized_defun(128),
                Self::LeafCountPruned if index < candidate_count / 2 => {
                    "(defun shared () ((a) (b)))".to_owned()
                }
                Self::LeafCountPruned => "(defun shared () (() (a b)))".to_owned(),
            };
            (
                PathBuf::from(format!("fixture-{index:03}.lisp")),
                body.into_bytes(),
            )
        });
        let files = entries.clone().collect::<BTreeMap<_, _>>();
        let inventory = SimilarityInventory {
            files: entries
                .map(|(path, _)| DiscoveredSimilarityFile {
                    path,
                    dialect: Dialect::CommonLisp,
                })
                .collect(),
            ..SimilarityInventory::default()
        };

        FixtureSource { inventory, files }
    }

    fn assert_contract(
        self,
        candidate_count: usize,
        max_results: Option<usize>,
        plan: &SimilarityReportPlan,
    ) {
        let summary = plan.report().summary();
        let possible_pairs = pair_count(candidate_count);

        assert_eq!(summary.possible_pairs(), possible_pairs);
        assert_eq!(summary.resource_skipped_pairs(), 0);
        assert_eq!(summary.unprocessed_pairs(), 0);
        assert!(!summary.candidate_limit_reached());
        assert!(!summary.comparison_limit_reached());
        assert!(plan.errors().is_empty());
        let matched_pairs = match self {
            Self::RepeatedShape => {
                assert_eq!(summary.evaluated_pairs(), possible_pairs);
                assert_eq!(summary.pruned_by_size(), 0);
                assert_eq!(summary.matched_pairs(), possible_pairs);
                possible_pairs
            }
            Self::NodeCountPruned => {
                let group_size = candidate_count / 2;
                let evaluated_pairs = 2 * pair_count(group_size);
                assert_eq!(summary.evaluated_pairs(), evaluated_pairs);
                assert_eq!(summary.pruned_by_size(), group_size * group_size);
                assert_eq!(summary.matched_pairs(), evaluated_pairs);
                evaluated_pairs
            }
            Self::LeafCountPruned => {
                let group_size = candidate_count / 2;
                let matched_pairs = 2 * pair_count(group_size);
                assert_eq!(summary.evaluated_pairs(), possible_pairs);
                assert_eq!(summary.pruned_by_size(), 0);
                assert_eq!(summary.matched_pairs(), matched_pairs);
                matched_pairs
            }
        };
        let (reported_pairs, suppressed_pairs, truncated) = match max_results {
            Some(1) => (matched_pairs.min(1), 0, matched_pairs > 1),
            None => (matched_pairs, 0, false),
            Some(limit) => panic!("unexpected benchmark retention limit: {limit}"),
        };

        assert_eq!(summary.reported_pairs(), reported_pairs);
        assert_eq!(summary.suppressed_pairs(), suppressed_pairs);
        assert_eq!(summary.truncated(), truncated);
    }
}

fn sized_defun(operand_count: usize) -> String {
    format!(
        "(defun shared (x) (+ x {}))",
        std::iter::repeat_n("1", operand_count)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

const fn pair_count(item_count: usize) -> usize {
    item_count * (item_count - 1) / 2
}

fn request(max_results: Option<usize>) -> SimilarityReportRequest {
    SimilarityReportRequest {
        roots: Vec::new(),
        include_unknown: false,
        include_hidden: false,
        include_generated: false,
        max_depth: None,
        exclude: Vec::new(),
        forced_dialect: None,
        options: SimilarityReportOptions::new(
            0.9,
            2,
            1,
            SimilarityComparisonScope::All,
            SimilarityFormScope::TopLevel,
            SimilarityOverlapPolicy::All,
            None,
            None,
            max_results,
        )
        .expect("benchmark options are valid"),
        error_policy: SimilarityErrorPolicy::Fail,
        duplicate_policy: SimilarityDuplicatePolicy::Ignore,
    }
}

fn run(mut source: FixtureSource, max_results: Option<usize>) -> SimilarityReportPlan {
    build_similarity_report(&mut source, request(max_results)).expect("benchmark fixture is valid")
}

fn benchmark_similarity_report(c: &mut Criterion) {
    for scenario in [
        Scenario::RepeatedShape,
        Scenario::NodeCountPruned,
        Scenario::LeafCountPruned,
    ] {
        let mut group = c.benchmark_group(scenario.label());
        for candidate_count in INPUT_SIZES {
            let fixture = scenario.source(candidate_count);

            for &(max_results, retention_label) in &RETENTION_MODES {
                scenario.assert_contract(
                    candidate_count,
                    max_results,
                    &run(fixture.clone(), max_results),
                );
                group.bench_with_input(
                    BenchmarkId::new(retention_label, candidate_count),
                    &fixture,
                    |bencher, fixture| {
                        bencher.iter_batched(
                            || fixture.clone(),
                            |source| black_box(run(source, max_results)),
                            BatchSize::SmallInput,
                        );
                    },
                );
            }
        }
        group.finish();
    }
}

/// Enough samples for the confidence interval to be narrower than the gate's
/// threshold.
///
/// This used to be Criterion's minimum — `sample_size(10)` over a one-second
/// window — which measured fast but produced an interval tens of percent wide.
/// `scripts/bench-compare.sh` fails a branch at a 10% regression, so an
/// interval that wide meant the gate could not distinguish a real regression
/// from a noisy neighbour on the runner, and it reported both.
///
/// The cost is roughly three seconds per benchmark instead of one and a half,
/// paid twice (baseline and working tree). That is minutes against a 45-minute
/// job budget, and it buys a verdict that means something.
fn criterion_config() -> Criterion {
    Criterion::default()
        .sample_size(50)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .without_plots()
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets = benchmark_similarity_report
}
criterion_main!(benches);
