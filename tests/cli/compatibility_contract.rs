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
    fs::read_to_string("docs/src/reference/compatibility.md").expect("read docs/src/reference/compatibility.md")
}

fn release_workflow() -> String {
    fs::read_to_string(".github/workflows/release.yml").expect("read release workflow")
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
        "docs/src/reference/compatibility.md must declare that the project follows Semantic Versioning"
    );
    assert!(
        guide.contains(&format!("`{major}.x`")),
        "docs/src/reference/compatibility.md must state the guarantees for the released `{major}.x` series"
    );
    assert!(
        major.parse::<u32>().expect("major version is numeric") >= 1,
        "the stability guarantees in docs/src/reference/compatibility.md require a 1.0.0 or later release"
    );
}

/// The stable surfaces are the reason automation can pin a major version. If a
/// surface is dropped from the guide, the promise silently disappears with it.
#[test]
fn compatibility_guide_enumerates_every_stable_surface() {
    let guide = releases_guide();

    // Presence alone is not enough: a surface moved from one list to the other
    // still "appears in the guide" while promising the opposite of what it did
    // before. v1.2.0 moved the Rust library API across this line, so the two
    // halves are checked separately.
    let (stable, unstable) = guide
        .split_once("Not stable —")
        .expect("docs/src/reference/compatibility.md must keep its stable/not-stable split");

    for surface in [
        "**Command paths.**",
        "**Flags.**",
        "**Exit codes.**",
        "**JSON reports.**",
        "**The Nix interface**",
    ] {
        assert!(
            stable.contains(surface),
            "docs/src/reference/compatibility.md must keep the stable surface documented: {surface}"
        );
    }

    for surface in [
        "**Human-readable text output.**",
        "**Diagnostic and error message text**",
        "**Everything below the crate root**",
        // Not stable since 1.2.0: the crate is `publish = false` and the CLI is
        // the supported interface, so the library is free to change shape.
        "**The Rust library API**",
    ] {
        assert!(
            unstable.contains(surface),
            "docs/src/reference/compatibility.md must keep the explicitly unstable surface documented: {surface}"
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
        ("docs/src/reference/compatibility.md", releases_guide()),
        (
            "docs/src/guide/agents.md",
            fs::read_to_string("docs/src/guide/agents.md").expect("read docs/src/guide/agents.md"),
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
    let checklist =
        fs::read_to_string("docs/notes/releasing.md").expect("read docs/notes/releasing.md");

    assert!(
        checklist.contains("(../src/reference/compatibility.md)"),
        "docs/notes/releasing.md must link the release and compatibility guide"
    );
    assert!(
        checklist.contains("nix flake check"),
        "docs/notes/releasing.md must run the verification gate before releasing"
    );
    assert!(
        !checklist.contains("cargo publish") && !checklist.contains("crates.io"),
        "paredit-cli is released as a Git tag, not a registry package; \
         docs/notes/releasing.md must not describe a registry publish"
    );
    assert!(
        releases_guide().contains("docs/notes/releasing.md)"),
        "docs/src/reference/compatibility.md must link the maintainer release checklist"
    );
}

/// The release workflow publishes an EMPTY DRAFT and writes no body. As of the
/// 2026-08-01 org revision the GitHub Release description is the only canonical
/// changelog — there is no CHANGELOG.md to extract a body from — so the failure
/// this test guards against is the opposite of the old one: not "the changelog
/// section is missing", but "someone reintroduced a body-generating step and
/// the release publishes itself with machine-written notes".
///
/// `draft: true` is the load-bearing half. Without it, forgetting the notes is
/// a user-visible state: the release appears under "Latest release" and in
/// `gh release list` with an empty description.
#[test]
fn release_workflow_creates_an_empty_draft_release() {
    let workflow = release_workflow();

    assert!(
        workflow.contains("draft: true"),
        ".github/workflows/release.yml must create the release as a draft, so a \
         release whose notes were never written cannot reach downstream"
    );
    assert!(
        !workflow.contains("body_path:"),
        ".github/workflows/release.yml must not set body_path: the release body \
         is written by hand into the GitHub Release description, which is the \
         only canonical changelog in this org"
    );
    assert!(
        !workflow.contains("generate_release_notes"),
        ".github/workflows/release.yml must not generate release notes: notes \
         are selected by \"does a user have to change their own code\", a \
         judgement no generator can make"
    );
    assert!(
        !workflow.contains("name: Extract release notes"),
        ".github/workflows/release.yml must not carry a release-notes extraction \
         step: CHANGELOG.md was abolished in the 2026-08-01 org revision, so \
         there is nothing to extract from"
    );
}

/// The maintainer checklist has to describe the draft-and-publish flow, since
/// the workflow deliberately stops short of publishing.
#[test]
fn release_checklist_describes_publishing_the_draft() {
    let guide = fs::read_to_string("docs/notes/releasing.md").expect("read docs/notes/releasing.md");

    assert!(
        guide.contains("--draft=false"),
        "docs/notes/releasing.md must tell the maintainer how to publish the \
         draft release the workflow creates"
    );
}
