use std::fs;

/// `[profile.pr-check]` exists so `treefmt-pr-check` and
/// `lint-format-integration-pr-check` (see flake.nix) can build a *working*
/// paredit binary under thin LTO instead of the shipped release profile's fat
/// LTO. That is only a safe trade if the PR-only profile is additive: the
/// shipped `[profile.release]` must keep its fat-LTO/single-codegen-unit
/// settings unchanged, or the binary that ships would silently regress in
/// lockstep with the CI speedup.
#[test]
fn cargo_toml_adds_a_pr_check_profile_without_weakening_release() {
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");

    let marker = "[profile.pr-check]";
    let start = cargo_toml
        .find(marker)
        .expect("Cargo.toml must define [profile.pr-check]");
    let pr_check_block = &cargo_toml[start..];
    for needle in [
        "inherits = \"release\"",
        "lto = \"thin\"",
        "codegen-units = 4",
    ] {
        assert!(
            pr_check_block.contains(needle),
            "[profile.pr-check] must set: {needle}"
        );
    }

    let release_start = cargo_toml
        .find("[profile.release]")
        .expect("Cargo.toml must define [profile.release]");
    let release_end = release_start
        + cargo_toml[release_start..]
            .find("[profile.bench]")
            .expect("[profile.release] is followed by [profile.bench]");
    let release_block = &cargo_toml[release_start..release_end];
    for needle in ["lto = \"fat\"", "codegen-units = 1"] {
        assert!(
            release_block.contains(needle),
            "[profile.release] must keep shipping under: {needle}"
        );
    }
}

/// The flake wires a thin-LTO sibling of the release package (`mkParedit`)
/// specifically for the two PR-only checks. If `mkParateditPrCheck` or the
/// `pr-check` cargo profile selector went missing, those checks would either
/// fail to build or silently fall back to the slow fat-LTO package, defeating
/// the wall-clock win.
#[test]
fn flake_wires_the_pr_check_package_and_checks() {
    let flake = fs::read_to_string("flake.nix").expect("read flake.nix");

    for needle in [
        "mkParateditPrCheck",
        "CARGO_PROFILE = \"pr-check\"",
        "treefmt-pr-check",
        "lint-format-integration-pr-check",
    ] {
        assert!(
            flake.contains(needle),
            "flake.nix must keep the PR-only fast-path wiring: {needle}"
        );
    }
}

/// CI's `plan` job filters the flake-derived check list by
/// `github.event_name`: pull requests skip the slow fat-LTO `package`,
/// `treefmt`, and `lint-format-integration` checks in favor of their
/// thin-LTO `-pr-check` siblings. Every other event (pushes to main, etc.)
/// must NOT drop the `-pr-check` names: `pull_request` events are always
/// Cachix pull-only, so if no push-enabled event ever built `packagePrCheck`
/// either, that cache key would never be pushed and every PR would
/// cold-compile the thin-LTO binary from scratch, forever, instead of
/// substituting it.
///
/// This is also a regression guard for `ci_derives_its_check_matrix_from_the_flake`
/// in `action_contract.rs`: if a future edit to the enumerate step broke the
/// flake-driven matrix while adding or changing the event-based filter, this
/// test would fail alongside it rather than passing vacuously.
#[test]
fn ci_plan_job_filters_checks_by_event_name_around_the_flake_derived_matrix() {
    let workflow = fs::read_to_string(".github/workflows/ci.yml").expect("read CI workflow");

    for needle in ["github.event_name", "pull_request", "\"package\""] {
        assert!(
            workflow.contains(needle),
            "CI's plan job must filter the check matrix by event name: {needle}"
        );
    }

    // The Cachix-seeding regression this test exists to catch: a push/main
    // event must never drop the `-pr-check` names the way a pull_request
    // drops the fat-LTO names, or `packagePrCheck` never gets built anywhere
    // Cachix push is enabled, and every PR pays a full cold thin-LTO compile
    // forever instead of substituting a cached one.
    for bad_pattern in [
        "!= \"treefmt-pr-check\"",
        "!= \"lint-format-integration-pr-check\"",
    ] {
        assert!(
            !workflow.contains(bad_pattern),
            "a push/main event must not filter out the `-pr-check` names, or \
             their Cachix cache key never gets seeded: found {bad_pattern}"
        );
    }

    for needle in [
        "ciCheckNames",
        "nix flake check --no-build",
        ".#checks.x86_64-linux.${{ matrix.check }}",
    ] {
        assert!(
            workflow.contains(needle),
            "CI must still take its check matrix from the flake, unchanged by \
             the PR wall-clock filter: {needle}"
        );
    }
}
