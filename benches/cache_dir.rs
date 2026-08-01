//! Does `--cache-dir` earn its keep?
//!
//! The flag is opt-in and documented as narrow: discovery is not the expensive
//! part of a lint run, so the cache only pays off where the walk genuinely
//! dominates. That claim was written from the design and never measured. This
//! benchmark measures it, and — more importantly — keeps measuring it, so a
//! change that makes a hit as slow as a walk fails
//! `scripts/bench-compare.sh` instead of quietly making the flag pointless.
//!
//! Three arms, because there are three distinct code paths and the middle one
//! is the one users forget:
//!
//! * `cold` — no `--cache-dir`. The walk, and the baseline everything else is
//!   compared against.
//! * `populate` — `--cache-dir` with nothing usable in it, i.e. the first run
//!   or any run after `--clear-cache`. Walks *and* serialises the entry, so it
//!   is necessarily slower than `cold`; it is here because the store path is
//!   otherwise unbenchmarked and a regression in it would be invisible.
//! * `warm` — `--cache-dir` reusing a valid entry. What the flag is for.
//!
//! Read the results as a ratio between the arms of one run, never as absolute
//! numbers: see the header of `scripts/bench-compare.sh` for why, and
//! `docs/src/reference/benchmarks.md` for how the gate reports them.

use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use paredit_core_cli::workspace_args::scan_workspace;
use paredit_core_workspace::workspace::{CacheOutcome, DiscoveryCache, WorkspaceDiscoveryOptions};

/// The two ends of the range, and deliberately nothing between them — the same
/// reasoning as the matching constant in `benches/lint_report.rs`.
const FILE_COUNTS: [usize; 2] = [128, 1024];

/// A realistic fanout, and the number that decides whether this benchmark
/// tells the truth.
///
/// Validating a hit re-stats every directory the walk listed and counts its
/// entries, so the cache's advantage is per *file*, not per path: one directory
/// holding a thousand files would make the hit look almost free and prove
/// nothing about a real source tree. Sixteen puts 1024 files behind 64
/// directories, which is roughly what a Lisp project looks like, and leaves the
/// comparison measuring what it claims to.
const FILES_PER_DIRECTORY: usize = 16;

/// One synthetic workspace, removed when it goes out of scope.
struct Fixture {
    base: PathBuf,
    root: PathBuf,
    cache: DiscoveryCache,
}

impl Fixture {
    fn new(file_count: usize) -> Self {
        let base = unique_directory();
        let root = base.join("root");
        for index in 0..file_count {
            let area = index / FILES_PER_DIRECTORY;
            let directory = root.join(format!("area-{area:03}"));
            fs::create_dir_all(&directory).expect("create the fixture directory");
            fs::write(
                directory.join(format!("form-{index:04}.lisp")),
                "(defun bench-fn (a b)\n  \"Return A plus B.\"\n  (+ a b))\n",
            )
            .expect("write the fixture file");
        }

        Self {
            // Outside the scanned root on purpose: a cache written inside the
            // tree it describes changes that tree, which is its own first
            // invalidation, and the `warm` arm would never hit.
            cache: DiscoveryCache::new(base.join("cache")),
            base,
            root,
        }
    }

    fn options(&self) -> WorkspaceDiscoveryOptions {
        WorkspaceDiscoveryOptions::for_roots(vec![self.root.clone()])
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn unique_directory() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is after the epoch")
        .as_nanos();
    let process = std::process::id();
    std::env::temp_dir().join(format!("paredit-bench-cache-{process}-{nanos}"))
}

fn walk(options: &WorkspaceDiscoveryOptions) -> usize {
    scan_workspace(options, false, None)
        .expect("the benchmark fixture is discoverable")
        .0
        .files()
        .len()
}

fn scan_with_cache(
    options: &WorkspaceDiscoveryOptions,
    cache: &DiscoveryCache,
) -> (usize, CacheOutcome) {
    let (discovery, outcome) =
        scan_workspace(options, false, Some(cache)).expect("the benchmark fixture is discoverable");
    (
        discovery.files().len(),
        outcome.expect("a cache was supplied, so an outcome is reported"),
    )
}

/// Fails the benchmark on a fixture that would measure nothing.
///
/// Both halves have a real failure mode behind them. A workspace the walk
/// cannot see — an ignore file, or a directory name on the unconditional skip
/// list — still benchmarks perfectly happily, comparing two empty scans. And a
/// `warm` arm that has stopped hitting is a second `cold` arm reported under a
/// name that says otherwise, which is worse than no measurement at all.
fn assert_measurable(fixture: &Fixture, file_count: usize) {
    let options = fixture.options();
    assert_eq!(
        walk(&options),
        file_count,
        "the walk found a different file set than the fixture wrote"
    );

    fixture.cache.clear().expect("clear the cache");
    let (populated, outcome) = scan_with_cache(&options, &fixture.cache);
    assert_eq!(outcome, CacheOutcome::Missing);
    assert_eq!(populated, file_count);

    let (reused, outcome) = scan_with_cache(&options, &fixture.cache);
    assert_eq!(outcome, CacheOutcome::Hit);
    assert_eq!(reused, file_count);
}

fn benchmark_cache_dir(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache-dir");

    for file_count in FILE_COUNTS {
        let fixture = Fixture::new(file_count);
        let options = fixture.options();
        assert_measurable(&fixture, file_count);

        group.throughput(Throughput::Elements(file_count as u64));

        group.bench_with_input(
            BenchmarkId::new("cold", file_count),
            &options,
            |bencher, options| {
                bencher.iter(|| black_box(walk(options)));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("populate", file_count),
            &options,
            |bencher, options| {
                // `PerIteration` is required, not a tuning choice: the setup
                // mutates state the routine then consumes. Every other
                // `BatchSize` runs the whole batch's setups first, so all but
                // the first iteration would find the entry the previous one
                // stored and measure a hit under the `populate` label.
                bencher.iter_batched(
                    || {
                        fixture
                            .cache
                            .clear()
                            .expect("clear the cache between iterations");
                    },
                    |()| {
                        let (files, outcome) = scan_with_cache(options, &fixture.cache);
                        assert_eq!(outcome, CacheOutcome::Missing);
                        black_box(files)
                    },
                    BatchSize::PerIteration,
                );
            },
        );

        // `populate` left the cache in whichever state its last iteration
        // produced. Restore a valid entry explicitly rather than depending on
        // the arm order above.
        let _ = scan_with_cache(&options, &fixture.cache);

        group.bench_with_input(
            BenchmarkId::new("warm", file_count),
            &options,
            |bencher, options| {
                bencher.iter(|| {
                    let (files, outcome) = scan_with_cache(options, &fixture.cache);
                    // One integer comparison against a whole filesystem scan.
                    assert_eq!(outcome, CacheOutcome::Hit);
                    black_box(files)
                });
            },
        );
    }

    group.finish();
}

/// Enough samples for the confidence interval to be narrower than the gate's
/// threshold — see the same function in `benches/similarity_report.rs`.
///
/// It matters more here than anywhere else in this directory. Every arm of this
/// benchmark is dominated by filesystem syscalls, which are the noisiest thing
/// a shared CI runner does, so the ten-sample minimum would produce an interval
/// wide enough to swallow the entire effect being measured.
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
    targets = benchmark_cache_dir
}
criterion_main!(benches);
