use std::hint::black_box;
use std::path::Path;
use std::sync::OnceLock;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::time::Duration;

use paredit_core_lint_engine::engine::{PassOptions, build_head_index, collect_lint_pass};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::{dialect::Dialect, sexpr::SyntaxTree};

const RULE_NAMES: [&str; 4] = [
    "contains-on-non-associative",
    "go-block-blocking-channel-op",
    "parking-op-outside-go-machinery",
    "reference-type-operator-mismatch",
];

fn catalog() -> RuleCatalog {
    static ENTRIES: OnceLock<Vec<RuleEntry>> = OnceLock::new();
    let entries = ENTRIES.get_or_init(|| {
        paredit_cli::lint::CATALOG
            .entries()
            .iter()
            .filter(|entry| RULE_NAMES.contains(&entry.meta().name().as_str()))
            .map(|entry| RuleEntry::new(entry.meta(), entry.rule()))
            .collect()
    });
    assert_eq!(
        entries.len(),
        RULE_NAMES.len(),
        "all four rules are shipped"
    );
    RuleCatalog::new(entries)
}

fn generated_source(forms: usize) -> String {
    (0..forms)
        .map(|index| {
            format!(
                "(defn f{index} [m coll in out]\n  \
                 (let [total (atom 0) seen (volatile! 0)]\n    \
                 (go (doseq [x coll] (>! out x)) (<! in))\n    \
                 (binding [*print-length* 5] (mapv pr-str coll))\n    \
                 (when (contains? m :k) (swap! total inc) (vswap! seen inc))\n    \
                 [@total @seen]))\n"
            )
        })
        .collect()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[allow(unsafe_code)]
fn thread_cpu_time() -> Duration {
    let mut time = std::mem::MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: the pointer refers to a live timespec, which clock_gettime
    // initializes on success. No value is read before checking its result.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, time.as_mut_ptr()) };
    assert_eq!(result, 0, "clock_gettime(CLOCK_THREAD_CPUTIME_ID) failed");
    // SAFETY: clock_gettime returned zero, so time is initialized.
    let time = unsafe { time.assume_init() };
    let seconds = u64::try_from(time.tv_sec).expect("CPU time seconds are nonnegative");
    let nanos = u32::try_from(time.tv_nsec).expect("CPU time nanoseconds are nonnegative");
    assert!(nanos < 1_000_000_000, "normalized timespec nanoseconds");
    Duration::new(seconds, nanos)
}

fn measured_cost(operation: impl FnOnce()) -> u128 {
    // The dispatcher is synchronous. Thread CPU time excludes time spent
    // descheduled by other test binaries without excluding its actual work.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let started = thread_cpu_time();
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let started = std::time::Instant::now();

    operation();

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let elapsed = thread_cpu_time()
        .checked_sub(started)
        .expect("thread CPU clock is monotonic");
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let elapsed = started.elapsed();

    elapsed.as_nanos().max(1)
}

fn pass_cost(forms: usize) -> u128 {
    let source = generated_source(forms);
    let tree = SyntaxTree::parse_with_dialect(&source, Dialect::Clojure).expect("parse generated");
    let catalog = catalog();
    let index = build_head_index(catalog);
    let pass = |measure| {
        collect_lint_pass(
            catalog,
            &index,
            Path::new("gen.clj"),
            Dialect::Clojure,
            &tree,
            &source,
            RuleSelection::All,
            PassOptions {
                settings: None,
                measure,
            },
        )
        .expect("lint pass")
    };
    let elapsed = measured_cost(|| {
        let outcome = pass(false);
        assert!(outcome.outcomes.is_empty(), "the generated file is clean");
        assert!(outcome.timings.is_none(), "ordinary passes are not timed");
    });

    // Check reachability separately so per-rule clock reads do not mask a
    // regression in the ordinary pass's work.
    let outcome = pass(true);
    assert!(outcome.outcomes.is_empty(), "the generated file is clean");
    let timings = outcome.timings.expect("measure: true records timings");
    assert_eq!(timings.entries().count(), RULE_NAMES.len());
    for (position, _, invocations) in timings.entries() {
        assert_eq!(
            invocations, forms as u64,
            "rule {position} reaches each form"
        );
    }
    elapsed
}

#[test]
fn doubling_the_file_does_not_more_than_triple_the_cost() {
    let small = pass_cost(500);
    let large = pass_cost(1000);
    eprintln!("lint pass: small={small}ns large={large}ns");
    assert!(
        large < small.saturating_mul(3),
        "doubling the file cost {large}ns against {small}ns; a rule has gone superlinear"
    );
}

#[test]
fn cost_measurement_rejects_repeated_root_materialization() {
    fn quadratic_cost(forms: usize) -> u128 {
        let source = generated_source(forms);
        let tree =
            SyntaxTree::parse_with_dialect(&source, Dialect::Clojure).expect("parse generated");
        let mut visits = 0;
        let elapsed = measured_cost(|| {
            for _ in 0..forms {
                // This is the regression the gate must catch: materializing
                // the entire document again for every candidate.
                let root = black_box(tree.root_view());
                assert_eq!(root.children.len(), forms);
                visits += black_box(root.children.len());
            }
        });
        assert_eq!(visits, forms * forms, "control performs quadratic work");
        elapsed
    }

    let small = quadratic_cost(100);
    let large = quadratic_cost(200);
    eprintln!("quadratic control: small={small}ns large={large}ns");
    assert!(
        large >= small.saturating_mul(3),
        "the cost gate did not reject quadratic work: {large}ns against {small}ns"
    );
}
