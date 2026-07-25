use super::*;

fn manifest_version() -> String {
    let manifest = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
    manifest
        .lines()
        .find_map(|line| line.trim().strip_prefix("version = "))
        .map(|value| value.trim_matches('"').to_owned())
        .expect("Cargo.toml declares a package version")
}

fn releases_guide() -> String {
    fs::read_to_string("docs/src/releases.md").expect("read docs/src/releases.md")
}

/// A major-version bump must be a deliberate act: the compatibility guide names
/// the series it applies to, so releasing `2.0.0` cannot silently inherit the
/// `1.x` promises.
#[test]
fn compatibility_guide_covers_the_released_major_series() {
    let version = manifest_version();
    let major = version
        .split('.')
        .next()
        .expect("version has a major component");
    let guide = releases_guide();

    assert!(
        guide.contains("Semantic Versioning"),
        "docs/src/releases.md must declare that the project follows Semantic Versioning"
    );
    assert!(
        guide.contains(&format!("`{major}.x`")),
        "docs/src/releases.md must state the guarantees for the released `{major}.x` series"
    );
    assert!(
        major.parse::<u32>().expect("major version is numeric") >= 1,
        "the stability guarantees in docs/src/releases.md require a 1.0.0 or later release"
    );
}

/// The stable surfaces are the reason automation can pin a major version. If a
/// surface is dropped from the guide, the promise silently disappears with it.
#[test]
fn compatibility_guide_enumerates_every_stable_surface() {
    let guide = releases_guide();

    for surface in [
        "**Command paths.**",
        "**Flags.**",
        "**Exit codes.**",
        "**JSON reports.**",
        "**The Rust library API**",
        "**The Nix interface**",
    ] {
        assert!(
            guide.contains(surface),
            "docs/src/releases.md must keep the stable surface documented: {surface}"
        );
    }

    for unstable in [
        "**Human-readable text output.**",
        "**Diagnostic and error message text**",
        "**Everything below the crate root**",
    ] {
        assert!(
            guide.contains(unstable),
            "docs/src/releases.md must keep the explicitly unstable surface documented: {unstable}"
        );
    }
}

/// Both machine-facing documents quote the schema version agents should expect.
/// Bumping the emitted version without updating them would leave the published
/// contract describing a shape the binary no longer produces.
#[test]
fn documented_schema_version_matches_the_emitted_reports() {
    let output = paredit()
        .args(["inspect", "capabilities", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value =
        serde_json::from_slice(&output).expect("capabilities emits valid JSON");
    let schema_version = report["schema_version"]
        .as_u64()
        .expect("capabilities reports a numeric schema_version");

    let documented = format!("(currently `{schema_version}`)");
    for (path, text) in [
        ("docs/src/releases.md", releases_guide()),
        (
            "docs/src/agents.md",
            fs::read_to_string("docs/src/agents.md").expect("read docs/src/agents.md"),
        ),
    ] {
        assert!(
            text.contains(&documented),
            "{path} must document the emitted schema version: {documented}"
        );
    }
}

/// The release checklist is the only place that turns the compatibility policy
/// into actions, so the published guide has to point at it.
#[test]
fn release_checklist_and_compatibility_guide_reference_each_other() {
    let checklist = fs::read_to_string("RELEASING.md").expect("read RELEASING.md");

    assert!(
        checklist.contains("docs/src/releases.md"),
        "RELEASING.md must link the release and compatibility guide"
    );
    assert!(
        checklist.contains("cargo publish --dry-run --locked"),
        "RELEASING.md must verify the package before publishing"
    );
    assert!(
        releases_guide().contains("RELEASING.md"),
        "docs/src/releases.md must link the maintainer release checklist"
    );
}
