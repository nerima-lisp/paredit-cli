use std::fs;

/// `scripts/bench-compare.sh`'s baseline build used to write into
/// `$worktree/target`, which the script's own `cleanup` trap deletes via
/// `git worktree remove --force` on every exit. Caching that path in CI
/// would therefore cache nothing: `actions/cache`'s post-job save step runs
/// after the trap has already removed it. The fix moves the baseline's
/// `CARGO_TARGET_DIR` outside the ephemeral worktree so it survives long
/// enough for CI to persist it.
#[test]
fn bench_compare_keeps_the_baseline_build_outside_the_ephemeral_worktree() {
    let script = fs::read_to_string("scripts/bench-compare.sh").expect("read bench-compare.sh");

    assert!(
        script.contains("baseline_target_dir"),
        "bench-compare.sh must give the baseline build a target dir that \
         outlives the worktree's own cleanup"
    );
    assert!(
        !script.contains("CARGO_TARGET_DIR=\"$worktree/target\""),
        "the baseline build must not write into $worktree/target again: that \
         path is deleted by this script's own `git worktree remove --force` \
         cleanup trap, so nothing CI caches there would ever be reused"
    );
}

/// CI's benchmark job runs plain `cargo bench` (not a crane/Nix derivation),
/// so it never benefits from Cachix. Without these caches, every PR
/// cold-compiles the full dependency graph twice — once for the working
/// tree, once for the merge-base baseline.
#[test]
fn ci_caches_the_benchmark_jobs_cargo_builds() {
    let workflow = fs::read_to_string(".github/workflows/ci.yml").expect("read CI workflow");

    for needle in [
        "actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6",
        "cargo-registry-",
        "bench-target-workingtree-",
        "bench-target-baseline-",
        "/tmp/paredit-bench-baseline-target",
    ] {
        assert!(
            workflow.contains(needle),
            "CI's benchmark job must cache its cargo builds: {needle}"
        );
    }

    // Each cache key falls back to a `restore-keys` prefix rather than
    // requiring an exact match, so a fuzzy hit still lets cargo's own
    // incremental fingerprinting skip whatever did not change.
    assert!(
        workflow.contains("restore-keys"),
        "the benchmark job's caches must fall back to a prefix match on a miss"
    );
}
