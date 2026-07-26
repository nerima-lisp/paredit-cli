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
