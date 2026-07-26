use std::fs;

#[test]
fn cargo_manifest_keeps_public_crate_metadata_explicit() {
    let manifest = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");

    for required in [
        "name = \"paredit-cli\"",
        "publish = false",
        "license = \"MIT\"",
        "readme = \"README.md\"",
        "repository = \"https://github.com/nerima-lisp/paredit-cli\"",
        "homepage = \"https://github.com/nerima-lisp/paredit-cli\"",
        "documentation = \"https://nerima-lisp.github.io/paredit-cli/\"",
        "rust-version = \"1.85\"",
    ] {
        assert!(
            manifest.contains(required),
            "Cargo.toml must keep public crate metadata explicit: {required}"
        );
    }
}

/// `[package].version` and `[workspace.package].version` must not drift apart.
///
/// `.github/workflows/release.yml` verifies the release tag against
/// `sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml | head -n1`, which
/// takes whichever column-0 `version =` line comes first in the file. With two
/// such lines, bumping only one lets a release either ship under the wrong tag
/// or hand workspace members a stale version, depending on section order.
#[test]
fn workspace_and_package_versions_stay_in_lockstep() {
    let manifest = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");

    let versions: Vec<&str> = manifest
        .lines()
        .filter_map(|line| line.strip_prefix("version = \""))
        .filter_map(|rest| rest.split('"').next())
        .collect();

    assert!(
        !versions.is_empty(),
        "Cargo.toml must declare a version at column 0 for release.yml to read"
    );
    for version in &versions {
        assert_eq!(
            version, &versions[0],
            "every column-0 `version =` line in Cargo.toml must agree, \
             because release.yml compares the tag against only the first one; \
             found {versions:?}"
        );
    }
}

/// Every workspace member must opt into the shared lint table.
///
/// `[lints]` is per-package and is **not** inherited by workspace members. A
/// member that omits `[lints] workspace = true` silently loses
/// `unsafe_code = "deny"` with no error of any kind, so this has to be checked
/// mechanically rather than left to review.
#[test]
fn every_workspace_member_inherits_the_shared_lints() {
    let root = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
    assert!(
        root.contains("[workspace.lints.rust]") && root.contains("unsafe_code = \"deny\""),
        "the workspace lint table must keep denying unsafe code"
    );

    let mut checked = 0usize;
    for kind in ["core", "feature"] {
        let Ok(entries) = fs::read_dir(format!("packages/{kind}")) else {
            continue;
        };
        for entry in entries.flatten() {
            let manifest_path = entry.path().join("Cargo.toml");
            if !manifest_path.is_file() {
                continue;
            }
            let manifest = fs::read_to_string(&manifest_path).expect("read member Cargo.toml");
            assert!(
                manifest.contains("[lints]") && manifest.contains("workspace = true"),
                "{} must declare `[lints] workspace = true`, or it silently \
                 opts out of unsafe_code = \"deny\"",
                manifest_path.display()
            );
            checked += 1;
        }
    }

    // Zero members is legitimate before the first package is extracted; the
    // loop above then simply has nothing to assert on.
    println!("checked {checked} workspace member manifests");
}

#[test]
fn rustdoc_entrypoint_stays_aligned_with_readme_and_public_library_surface() {
    let lib_rs = fs::read_to_string("src/lib.rs").expect("read src/lib.rs");
    let readme = fs::read_to_string("README.md").expect("read README.md");

    assert!(
        readme.contains("A typed Rust library API behind the CLI"),
        "contract only applies while README advertises a public Rust library API"
    );
    assert!(
        lib_rs.contains("#![doc = include_str!(\"../README.md\")]"),
        "src/lib.rs must use README.md as the crate-level rustdoc entrypoint"
    );
    for required in ["pub use domain::dialect;", "pub use domain::sexpr;"] {
        assert!(
            lib_rs.contains(required),
            "src/lib.rs must keep the public library entrypoint explicit: {required}"
        );
    }
}
