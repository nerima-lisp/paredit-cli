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

fn release_workflow() -> String {
    fs::read_to_string(".github/workflows/release.yml").expect("read release workflow")
}

/// Extracts the exact awk program text from release.yml's "Extract release
/// notes" step, so this test runs the real script instead of a hand-retyped
/// copy that could silently drift from it.
fn release_notes_awk_script() -> String {
    let workflow = release_workflow();
    let start_marker = "awk -v v=\"$version\" '";
    let start = workflow
        .find(start_marker)
        .expect("release.yml invokes awk to extract release notes")
        + start_marker.len();
    let end = start
        + workflow[start..]
            .find("' CHANGELOG.md")
            .expect("awk invocation is closed before piping into CHANGELOG.md");
    workflow[start..end].to_owned()
}

fn changelog() -> String {
    fs::read_to_string("CHANGELOG.md").expect("read CHANGELOG.md")
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
    let checklist =
        fs::read_to_string("docs/src/releasing.md").expect("read docs/src/releasing.md");

    assert!(
        checklist.contains("(releases.md)"),
        "docs/src/releasing.md must link the release and compatibility guide"
    );
    assert!(
        checklist.contains("nix flake check"),
        "docs/src/releasing.md must run the verification gate before releasing"
    );
    assert!(
        !checklist.contains("cargo publish") && !checklist.contains("crates.io"),
        "paredit-cli is released as a Git tag, not a registry package; \
         docs/src/releasing.md must not describe a registry publish"
    );
    assert!(
        releases_guide().contains("(releasing.md)"),
        "docs/src/releases.md must link the maintainer release checklist"
    );
}

/// `release.yml`'s "Extract release notes" step has no test coverage of its
/// own; a change to the awk script, or to CHANGELOG.md's heading format,
/// would only surface when a tag is actually pushed. This runs the real
/// script (extracted above) against the real CHANGELOG.md and mirrors the
/// step's own `[[ ! -s release-notes.md ]]` failure check as an assertion on
/// the command's stdout.
#[test]
fn release_notes_awk_script_extracts_the_current_version_section() {
    let script = release_notes_awk_script();
    let version = manifest_version();

    // `awk` is a system dependency, not a Rust crate: it is present in the
    // Nix devShell/sandbox and on standard macOS/Linux, so this is a low but
    // non-zero portability assumption for this one test.
    let output = std::process::Command::new("awk")
        .arg("-v")
        .arg(format!("v={version}"))
        .arg(&script)
        .arg("CHANGELOG.md")
        .output()
        .expect("run awk against CHANGELOG.md");

    assert!(
        output.status.success(),
        "awk exited non-zero extracting release notes for {version}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.is_empty(),
        "release.yml's awk script produced no release-notes.md content for \
         version {version}; CHANGELOG.md must keep a `## [{version}]` section, \
         mirroring the `[[ ! -s release-notes.md ]]` failure check in \
         .github/workflows/release.yml"
    );
}

/// `release.yml`'s awk script matches headings literally as `## [X.Y.Z]`, so
/// a malformed future CHANGELOG.md entry (wrong bracket, missing date, extra
/// whitespace) would silently produce an empty release body instead of
/// failing loudly. This checks every heading, not just the current release.
#[test]
fn changelog_headings_match_the_keep_a_changelog_format() {
    let changelog = changelog();

    for line in changelog.lines().filter(|line| line.starts_with("## [")) {
        assert!(
            is_unreleased_heading(line) || is_dated_release_heading(line),
            "CHANGELOG.md heading does not match `## [Unreleased]` or \
             `## [X.Y.Z] - YYYY-MM-DD`: {line}"
        );
    }
}

fn is_unreleased_heading(line: &str) -> bool {
    line == "## [Unreleased]"
}

fn is_dated_release_heading(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("## [") else {
        return false;
    };
    let Some((version, date)) = rest.split_once("] - ") else {
        return false;
    };
    is_semver(version) && is_iso_date(date)
}

fn is_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn is_iso_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && date[0..4].bytes().all(|byte| byte.is_ascii_digit())
        && date[5..7].bytes().all(|byte| byte.is_ascii_digit())
        && date[8..10].bytes().all(|byte| byte.is_ascii_digit())
}
