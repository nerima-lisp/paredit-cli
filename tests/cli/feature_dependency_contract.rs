//! FEATURE-CANDIDATES.md section P5: which feature package may depend on
//! which other feature package is a deliberate decision
//! (`SPEC-package-by-feature.md` section 2.2 measured 89 such edges among the
//! pre-split slices and treated every one as a choice, not an accident), not
//! something that should show up for the first time as a compiling
//! `Cargo.toml`. [`ALLOWED_FEATURE_DEPENDENCIES`] is that list, in the same
//! spirit as `architecture_contract.rs`'s `COMPOSITION_ROOT`: a
//! `paredit-feature-*` dependency that is not on it fails this test instead
//! of merging silently.
//!
//! Core packages are covered separately by
//! `architecture_contract.rs::core_packages_never_depend_on_a_feature` —
//! every `paredit-core-*` dependency is allowed for a feature, so only
//! feature-to-feature edges are enumerated here. Reads `Cargo.toml` as text
//! for the same reason `architecture_contract.rs` does: `cargo metadata`
//! would not run in the Nix sandbox without the whole dependency graph.

use std::fs;

/// `(feature, [feature it may depend on, ...])`, sorted by feature name. Add
/// a line here in the same commit that adds the `paredit-feature-*`
/// dependency to a package's `Cargo.toml` — that pairing is the review.
const ALLOWED_FEATURE_DEPENDENCIES: &[(&str, &[&str])] = &[
    ("generate", &["project-inventory"]),
    ("inline", &["rename"]),
    ("lint-form-shape", &["function-parameter"]),
    ("project-analysis", &["package", "remove-unused"]),
    ("refactor-workflow", &["project-analysis", "rename"]),
    ("remove-unused", &["form-transform", "package"]),
];

fn feature_packages() -> Vec<String> {
    let mut names = Vec::new();
    let Ok(entries) = fs::read_dir("packages/feature") else {
        return names;
    };
    for entry in entries.flatten() {
        if entry.path().join("Cargo.toml").is_file() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    names
}

/// Every `paredit-feature-<name>` this manifest's text mentions, deduplicated
/// and sorted. A whole-text scan rather than a line parse, matching how
/// `architecture_contract.rs` reads manifests.
fn declared_feature_dependencies(manifest: &str) -> Vec<String> {
    const MARKER: &str = "paredit-feature-";
    let mut deps = Vec::new();
    let mut search_from = 0;
    while let Some(offset) = manifest[search_from..].find(MARKER) {
        let start = search_from + offset + MARKER.len();
        let end = manifest[start..]
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
            .map_or(manifest.len(), |i| start + i);
        deps.push(manifest[start..end].to_owned());
        search_from = end.max(start + 1);
    }
    deps.sort();
    deps.dedup();
    deps
}

#[test]
fn every_allowlisted_feature_is_a_real_package() {
    let packages = feature_packages();
    for (feature, dependencies) in ALLOWED_FEATURE_DEPENDENCIES {
        assert!(
            packages.contains(&(*feature).to_owned()),
            "ALLOWED_FEATURE_DEPENDENCIES names `{feature}`, which is not a package under \
             packages/feature/"
        );
        for dependency in *dependencies {
            assert!(
                packages.contains(&(*dependency).to_owned()),
                "ALLOWED_FEATURE_DEPENDENCIES lists `{feature}` -> `{dependency}`, but \
                 `{dependency}` is not a package under packages/feature/"
            );
        }
    }
}

#[test]
fn feature_to_feature_dependencies_match_the_allowlist() {
    for package in feature_packages() {
        let manifest_path = format!("packages/feature/{package}/Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("read {manifest_path}: {error}"));
        // The manifest's own `name = "paredit-feature-<package>"` line
        // matches the same scan; a package never depends on itself.
        let declared: Vec<String> = declared_feature_dependencies(&manifest)
            .into_iter()
            .filter(|dependency| *dependency != package)
            .collect();

        let expected: Vec<String> = ALLOWED_FEATURE_DEPENDENCIES
            .iter()
            .find(|(feature, _)| *feature == package)
            .map(|(_, dependencies)| dependencies.iter().map(|s| (*s).to_owned()).collect())
            .unwrap_or_default();

        assert_eq!(
            declared, expected,
            "packages/feature/{package}/Cargo.toml declares a different feature-to-feature \
             dependency set ({declared:?}) than ALLOWED_FEATURE_DEPENDENCIES here ({expected:?}). \
             If this is a new, deliberate edge, add it to that list; if it is a stale entry \
             (the dependency was removed), delete it from that list."
        );
    }
}
