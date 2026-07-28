use super::*;

#[test]
fn readme_verification_model_matches_ci_trigger_surface() {
    let readme = fs::read_to_string("README.md").expect("read README");
    let workflow = fs::read_to_string(".github/workflows/ci.yml").expect("read CI workflow");

    assert!(
        readme.contains("Pull requests run every `nix flake check` output"),
        "README verification model should describe the current PR CI gate"
    );

    if !workflow.contains("\n  push:\n") {
        assert!(
            !readme.contains("The GitHub Actions badge reflects `nix flake check`"),
            "README must not claim the badge reflects nix flake check when CI has no push trigger"
        );
    }
}
