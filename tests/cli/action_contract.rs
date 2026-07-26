use std::fs;

#[test]
fn composite_action_exposes_lint_format_and_fix_modes() {
    let action = fs::read_to_string("action.yml").expect("read action.yml");

    assert!(
        action.contains("using: composite"),
        "action.yml must stay a composite action so consumers need no container"
    );
    for needle in [
        "cachix/install-nix-action",
        "cachix/cachix-action",
        "default: takeokunn-paredit-cli",
        "default: lint",
    ] {
        assert!(
            action.contains(needle),
            "action.yml must keep the documented toolchain and defaults: {needle}"
        );
    }
    for mode in ["lint)", "format)", "fix)"] {
        assert!(
            action.contains(mode),
            "action.yml dispatch must handle the documented mode {mode}"
        );
    }
    assert!(
        action.contains("github.action_ref"),
        "action.yml must default the binary ref to the pinned action ref"
    );
}

#[test]
fn documentation_covers_lint_and_format_integration_surfaces() {
    let docs =
        fs::read_to_string("docs/src/integrations.md").expect("read integrations documentation");

    assert!(
        docs.contains("## GitHub Actions"),
        "documentation must include integration guidance"
    );
    for needle in [
        "uses: nerima-lisp/paredit-cli@",
        "mode: lint",
        "nix run github:nerima-lisp/paredit-cli -- inspect check",
        "paredit inspect check --file source.lisp",
    ] {
        assert!(
            docs.contains(needle),
            "integration documentation must include: {needle}"
        );
    }
}

#[test]
fn flake_exposes_the_documented_integration_surfaces() {
    let flake = fs::read_to_string("flake.nix").expect("read flake.nix");

    for needle in [
        "paredit-lint",
        "paredit-format",
        "paredit-format-files",
        "overlays.default",
        "mkLintCheck",
        "mkFormatCheck",
        "treefmtFormatter",
        "treefmt-nix",
        "lint-format-integration",
        "pkgs.cargo-audit",
    ] {
        assert!(
            flake.contains(needle),
            "flake.nix must keep the documented integration surface: {needle}"
        );
    }
    assert!(
        flake.contains("excludes = [ \"tests/fixtures/*\" ]"),
        "flake.nix must keep test fixtures out of the paredit treefmt formatter"
    );
}

/// `lispIncludes` decides which files `paredit-lint`, `paredit-format`, and the
/// treefmt formatter ever look at. If it drifts from `Dialect::from_extension`,
/// a supported dialect is silently skipped by every Nix gate — the failure is
/// invisible because the tools simply report nothing for those files.
#[test]
fn flake_lisp_includes_cover_exactly_the_recognized_dialect_extensions() {
    use std::collections::BTreeSet;

    let flake = fs::read_to_string("flake.nix").expect("read flake.nix");
    let marker = "lispIncludes = [";
    let start = flake.find(marker).expect("flake.nix defines lispIncludes") + marker.len();
    let end = start
        + flake[start..]
            .find(']')
            .expect("lispIncludes list is closed");
    let flake_extensions: BTreeSet<&str> = flake[start..end]
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("\"*."))
        .filter_map(|line| line.strip_suffix('"'))
        .collect();

    let dialect = fs::read_to_string("src/domain/dialect/mod.rs").expect("read dialect module");
    let arms_start = dialect
        .find("pub fn from_extension")
        .expect("dialect module defines from_extension");
    let arms_end = arms_start
        + dialect[arms_start..]
            .find("_ => Self::Unknown")
            .expect("from_extension has a fallback arm");
    let dialect_extensions: BTreeSet<&str> = dialect[arms_start..arms_end]
        .lines()
        .filter(|line| line.contains("=> Self::"))
        .flat_map(|line| {
            line.split("=>")
                .next()
                .expect("match arm has a pattern")
                .split('|')
        })
        .map(|pattern| pattern.trim().trim_matches('"'))
        .filter(|pattern| !pattern.is_empty())
        .collect();

    assert!(
        !dialect_extensions.is_empty(),
        "failed to extract any extension from Dialect::from_extension"
    );
    assert_eq!(
        flake_extensions, dialect_extensions,
        "flake.nix lispIncludes must mirror Dialect::from_extension so the Nix \
         lint, format, and treefmt gates cover every file paredit can parse"
    );
}

/// The org-wide conformance check expects `checks.{default,formatting,docs}`
/// aliases inside the `checks` flake output. Scoped to that block
/// specifically: `default` and `docs` also appear as `packages.default` and
/// `packages.docs` keys elsewhere in flake.nix, so an unscoped substring
/// search would pass vacuously even if these aliases were removed from
/// `checks`.
#[test]
fn flake_checks_expose_the_org_conformance_aliases() {
    let flake = fs::read_to_string("flake.nix").expect("read flake.nix");
    let marker = "checks = lib.genAttrs systems (";
    let start = flake.find(marker).expect("flake.nix defines checks") + marker.len();
    let end_marker = "overlays.default = final: _prev: {";
    let end = start
        + flake[start..]
            .find(end_marker)
            .expect("checks block is followed by overlays.default");
    let checks_block = &flake[start..end];

    for alias in [
        "default = nextest;",
        "formatting = treefmt;",
        "docs = documentation;",
    ] {
        assert!(
            checks_block.contains(alias),
            "flake.nix checks block must keep the org conformance alias: {alias}"
        );
    }
}

/// CI keeps two gates: the flake checks and the dependency audit.
///
/// The host matrix this test used to pin is gone on purpose. `nix flake check`
/// only ever evaluated the checks for the host it ran on, so the macOS leg
/// verified the `aarch64-darwin` outputs and nothing the Linux leg did — and
/// the two legs shared no work, so a matrix was paying twice for one
/// verification while the Nix setup itself was duplicated in every job.
/// Both jobs now run on Linux through the shared `nix-setup` composite action.
///
/// What is asserted is therefore the *gates*, not the hosts: whichever way the
/// jobs are arranged, a run that stops checking the flake or stops auditing
/// dependencies is a run that has lost its purpose. Darwin coverage is a
/// deliberate gap — see `docs/src/development.md`.
#[test]
fn ci_runs_the_flake_checks_and_audits_dependencies() {
    let workflow = fs::read_to_string(".github/workflows/ci.yml").expect("read CI workflow");

    for needle in [
        "runs-on: ubuntu-latest",
        "./.github/actions/nix-setup",
        "nix flake check",
        "supply-chain:",
        "nix develop --command cargo audit --deny warnings",
    ] {
        assert!(
            workflow.contains(needle),
            "CI must keep its verification and supply-chain gates: {needle}"
        );
    }
}

#[test]
fn external_github_actions_are_immutably_pinned() {
    let action = fs::read_to_string("action.yml").expect("read action.yml");
    let ci = fs::read_to_string(".github/workflows/ci.yml").expect("read CI workflow");
    let docs =
        fs::read_to_string(".github/workflows/docs.yml").expect("read documentation workflow");
    let nix_setup = fs::read_to_string(".github/actions/nix-setup/action.yml")
        .expect("read shared nix-setup composite action");
    let release =
        fs::read_to_string(".github/workflows/release.yml").expect("read release workflow");
    let flake_update = fs::read_to_string(".github/workflows/flake-update.yml")
        .expect("read flake-update workflow");

    for pin in [
        "cachix/install-nix-action@630ae543ea3a38a9a4166f03376c02c50f408342 # v31",
        "cachix/cachix-action@5f2d7c5294214f71b873db4b969586b980625e71 # v17",
    ] {
        assert!(
            action.contains(pin),
            "action.yml must keep the approved immutable action pin: {pin}"
        );
    }

    let pin = "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7";
    assert!(
        ci.contains(pin),
        "CI must keep the approved immutable action pin: {pin}"
    );

    for pin in [
        "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7",
        "actions/configure-pages@983d7736d9b0ae728b81ab479565c72886d7745b # v5",
        "actions/upload-pages-artifact@7b1f4a764d45c48632c6b24a0339c27f5614fb0b # v4",
        "actions/deploy-pages@d6db90164ac5ed86f2b6aed7e0febac5b3c0c03e # v4",
    ] {
        assert!(
            docs.contains(pin),
            "documentation workflow must keep the approved immutable action pin: {pin}"
        );
    }

    for pin in [
        "DeterminateSystems/nix-installer-action@ef8a148080ab6020fd15196c2084a2eea5ff2d25 # v22",
        "cachix/cachix-action@5f2d7c5294214f71b873db4b969586b980625e71 # v17",
    ] {
        assert!(
            nix_setup.contains(pin),
            "the shared nix-setup composite action must keep the approved immutable action pin: {pin}"
        );
    }

    for pin in [
        "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7",
        "softprops/action-gh-release@3bb12739c298aeb8a4eeaf626c5b8d85266b0e65 # v2.6.2",
    ] {
        assert!(
            release.contains(pin),
            "the release workflow must keep the approved immutable action pin: {pin}"
        );
    }

    for pin in [
        "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7",
        "DeterminateSystems/update-flake-lock@834c491b2ece4de0bbd00d85214bb5e83b4da5c6 # v28",
    ] {
        assert!(
            flake_update.contains(pin),
            "the flake-update workflow must keep the approved immutable action pin: {pin}"
        );
    }

    for (path, contents) in [
        ("action.yml", action.as_str()),
        (".github/workflows/ci.yml", ci.as_str()),
        (".github/workflows/docs.yml", docs.as_str()),
        (".github/actions/nix-setup/action.yml", nix_setup.as_str()),
        (".github/workflows/release.yml", release.as_str()),
        (".github/workflows/flake-update.yml", flake_update.as_str()),
    ] {
        for mutable_tag in [
            "actions/checkout@v",
            "cachix/install-nix-action@v",
            "cachix/cachix-action@v",
            "actions/configure-pages@v",
            "actions/upload-pages-artifact@v",
            "actions/deploy-pages@v",
            "DeterminateSystems/nix-installer-action@v",
            "softprops/action-gh-release@v",
            "DeterminateSystems/update-flake-lock@v",
        ] {
            assert!(
                !contents.contains(mutable_tag),
                "{path} must not use mutable action tag {mutable_tag}"
            );
        }
    }
}
