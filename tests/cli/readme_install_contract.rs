use super::*;

#[test]
fn readme_installation_and_msrv_match_manifest_contract() {
    let readme = fs::read_to_string("README.md").expect("read README");
    let manifest = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");

    assert!(
        readme.contains("nix develop -c cargo install --path . --locked"),
        "README installation section should use a locked local install command"
    );
    assert!(
        readme.contains("cargo install --git https://github.com/nerima-lisp/paredit-cli --locked"),
        "README installation section should use a locked git install command"
    );

    let rust_version = manifest
        .lines()
        .find_map(|line| line.trim().strip_prefix("rust-version = "))
        .map(|value| value.trim_matches('"').to_owned())
        .expect("Cargo.toml rust-version");

    assert!(
        readme.contains(&format!(
            "The current minimum supported Rust version is `{rust_version}`."
        )),
        "README MSRV text must match Cargo.toml rust-version"
    );
}
