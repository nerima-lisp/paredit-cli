//! The architecture rules the package split exists to enforce.
//!
//! Section 6 Phase 6-4 makes these mandatory. Each one guards a property that
//! is invisible until it is already broken: a package with no README, a core
//! package that has grown a dependency on a feature, `clap` leaking out of a
//! `cli` module, or a member that silently opted out of `unsafe_code = "deny"`.
//!
//! They read `Cargo.toml` and source as text on purpose. `cargo metadata` would
//! be more precise and would also mean these tests could not run in the Nix
//! sandbox without the whole dependency graph.

use std::fs;
use std::path::{Path, PathBuf};

/// Every `packages/*/*` directory that has a `Cargo.toml`.
fn workspace_members() -> Vec<PathBuf> {
    let mut members = Vec::new();
    for kind in ["core", "feature"] {
        let Ok(entries) = fs::read_dir(format!("packages/{kind}")) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().join("Cargo.toml").is_file() {
                members.push(entry.path());
            }
        }
    }
    members.sort();
    members
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Section 3.3: a package that does not say what its boundary means has only
/// declared one mechanically.
#[test]
fn every_workspace_package_documents_itself() {
    let members = workspace_members();
    assert!(
        members.len() >= 20,
        "expected the split to have produced at least 20 members, found {}",
        members.len()
    );

    for member in &members {
        let manifest_path = member.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path).expect("read member manifest");

        let name = manifest
            .lines()
            .find_map(|line| line.strip_prefix("name = \""))
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_else(|| panic!("{} has no name", manifest_path.display()));

        assert!(
            manifest.contains("readme = \"README.md\""),
            "{} must declare readme = \"README.md\"",
            manifest_path.display()
        );

        let readme_path = member.join("README.md");
        let readme = fs::read_to_string(&readme_path)
            .unwrap_or_else(|_| panic!("{} is missing", readme_path.display()));

        let heading = readme.lines().next().unwrap_or_default();
        assert_eq!(
            heading,
            format!("# {name}"),
            "{}'s first heading must name the package",
            readme_path.display()
        );

        let lib_rs = member.join("src/lib.rs");
        let lib = fs::read_to_string(&lib_rs).expect("read member lib.rs");
        assert!(
            lib.contains("#![doc = include_str!(\"../README.md\")]"),
            "{} must embed its README, so a stale one shows up in rustdoc",
            lib_rs.display()
        );
    }
}

/// Section 6 Phase 6-4: core must not depend on a feature.
///
/// This is the direction that cannot be recovered from once it is allowed. A
/// core package that names a feature stops being reusable by the others, and
/// the compiler will happily accept it.
#[test]
fn core_packages_never_depend_on_a_feature() {
    for member in workspace_members() {
        if !member.starts_with("packages/core") {
            continue;
        }
        let manifest_path = member.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path).expect("read core manifest");
        assert!(
            !manifest.contains("paredit-feature-"),
            "{} depends on a feature package; core must not. \
             Move the shared type down into core, or the module up into the feature.",
            manifest_path.display()
        );
    }
}

/// Section 3.1: the dependency rule the layer directories used to express.
///
/// Layers are no longer directories, so "domain logic knows nothing about CLI
/// delivery" needs a mechanical statement. This is it: `clap` may appear only
/// under a `cli` path.
#[test]
fn domain_logic_never_depends_on_the_cli_argument_parser() {
    let mut offenders = Vec::new();
    for member in workspace_members() {
        for source in rust_sources(&member.join("src")) {
            let text = source.to_string_lossy();
            let in_cli = text.contains("/cli/")
                || text.ends_with("/cli.rs")
                // core/cli is the one package whose whole purpose is CLI
                // vocabulary; it is named for it.
                || text.contains("packages/core/cli/");
            if in_cli {
                continue;
            }
            let body = fs::read_to_string(&source).expect("read source");
            if body.lines().any(|line| {
                let code = line.split("//").next().unwrap_or_default();
                code.contains("use clap") || code.contains("clap::")
            }) {
                offenders.push(source);
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "clap must appear only under a cli path; found it in {offenders:?}"
    );
}

/// Section 9.3: `[lints]` is per-package and is NOT inherited.
///
/// A member that omits `[lints] workspace = true` loses
/// `unsafe_code = "deny"` with no error of any kind, which is why this is
/// checked mechanically rather than left to review.
#[test]
fn every_workspace_member_declares_the_shared_lints() {
    for member in workspace_members() {
        let manifest_path = member.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path).expect("read member manifest");
        assert!(
            manifest.contains("[lints]") && manifest.contains("workspace = true"),
            "{} must declare `[lints] workspace = true`, or it silently opts out of \
             unsafe_code = \"deny\"",
            manifest_path.display()
        );
    }
}

/// Section 3.1.1: the root's layer modules must not accumulate code again.
///
/// They are kept as the public API's namespace, and the cost of keeping them is
/// this test. Without it, "just put it in domain for now" comes back.
///
/// The allowance below is the composition root: modules that enumerate or
/// aggregate several features, which section 11.5.1 establishes cannot live in
/// core or in a feature. `REGISTRY` naming all 134 lint rules is the canonical
/// case; `lint_report` runs that registry, `lint_suppression` builds on it, and
/// `report_policy` re-exports policy types owned by three different features.
#[test]
fn root_layer_modules_hold_only_facades_and_the_composition_root() {
    const COMPOSITION_ROOT: &[&str] = &[
        // Enumerates every lint rule; see section 4.2.
        "lint",
        // Runs the registry, so it cannot be core or a feature.
        "lint_report",
        "lint_suppression",
        // Re-exports policy types owned by three separate features.
        "report_policy",
        // A development harness measured against the semantics layer, reached
        // by examples/semantic_coverage.rs through the public API.
        "semantic_coverage",
        // Reports that still await a home; each is a candidate for its own
        // feature package, and this list must shrink, never grow.
        "duplicate_export_report",
        "duplicate_method_report",
        "duplicate_slot_report",
        "shadowed_binding_report",
        "unused_parameter_report",
        "mutation_safety",
        "symbol_report",
    ];

    for (layer, dir) in [
        ("domain", "src/domain"),
        ("application", "src/application/usecase"),
        ("infrastructure", "src/infrastructure"),
    ] {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if stem == "mod" || COMPOSITION_ROOT.contains(&stem.as_str()) {
                continue;
            }
            panic!(
                "{} holds `{stem}`, which is neither a facade nor listed as composition \
                 root. Extract it into a package, or add it to COMPOSITION_ROOT with a \
                 reason. The {layer} layer exists to name the public API, not to hold code.",
                path.display()
            );
        }
    }
}
