//! Golden-output safety net for the `inspect lint` rule suite: pins today's
//! text/JSON/SARIF/fix-plan/fix output for a fixture corpus that collectively
//! covers every rule in [`RULES`], byte-for-byte. This exists to protect the
//! rule-engine's planned trait+registry refactor — the fixtures and golden
//! files here must not be touched by that refactor; only its `src/` change
//! should be judged against these goldens staying green.
//!
//! Every fixture is scanned from a scratch directory with a bare relative
//! filename argument (`paredit inspect lint broad.lisp`, run with
//! `current_dir` set to the scratch dir), so every reported path in every
//! output is that literal filename — stable across machines and runs, with
//! no absolute-temp-path normalization required.
//!
//! Regenerate the goldens deliberately with:
//! `UPDATE_LINT_GOLDEN=1 cargo test --test cli lint_report_golden`

use super::*;
use std::path::{Path, PathBuf};

/// The fixture corpus, with extensions: the dialect a rule applies to is
/// resolved from the file name, so the Emacs Lisp rules need an `.el` fixture
/// and would report nothing against a `.lisp` one. Collectively these trigger
/// all 143 lint rules (see the module doc for the coverage story of each
/// file).
const FIXTURES: [&str; 4] = [
    "broad.lisp",
    "nested.lisp",
    "suppressed.lisp",
    "emacs-lisp.el",
];

/// A fixture's golden-file prefix: its name without the extension.
fn stem(fixture: &str) -> &str {
    fixture.rsplit_once('.').map_or(fixture, |(stem, _)| stem)
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lint_golden")
}

fn golden_dir() -> PathBuf {
    fixture_dir().join("expected")
}

/// Compares `actual` against the committed golden file `name`. With
/// `UPDATE_LINT_GOLDEN` set in the environment, overwrites the golden instead
/// — an explicit opt-in for regenerating them after an intentional output
/// change; the default path only ever compares, never writes.
fn check_golden(name: &str, actual: &str) {
    let path = golden_dir().join(name);
    if std::env::var_os("UPDATE_LINT_GOLDEN").is_some() {
        fs::write(&path, actual)
            .unwrap_or_else(|error| panic!("writing golden {}: {error}", path.display()));
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "reading golden {}: {error}; run `UPDATE_LINT_GOLDEN=1 cargo test --test cli lint_report_golden` to generate it",
            path.display()
        )
    });
    assert_eq!(actual, expected, "golden mismatch for {name}");
}

/// Seeds `scratch` with a fresh copy of `fixture`.lisp (overwriting any
/// earlier mutation left by a prior `--fix` call against the same scratch
/// dir) and runs `paredit inspect lint <extra_args> <fixture>.lisp` from
/// inside it. Rewritten byte-for-byte via `fs::write` rather than `fs::copy`:
/// on macOS the latter can clone source metadata (e.g. via `clonefile`),
/// which lets the seeded file's group ownership diverge from what a plain
/// new-file create in `scratch` gets — exactly the mismatch `--fix`'s atomic
/// write then refuses to resolve.
fn run_lint(fixture: &str, extra_args: &[&str], scratch: &Path) -> std::process::Output {
    let file_name = fixture.to_owned();
    let source = fs::read(fixture_dir().join(&file_name)).expect("read fixture source");
    fs::write(scratch.join(&file_name), source).expect("seed fixture into scratch dir");
    paredit()
        .current_dir(scratch)
        .arg("inspect")
        .arg("lint")
        .args(extra_args)
        .arg(&file_name)
        .output()
        .expect("run paredit inspect lint")
}

fn stdout_utf8(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is valid UTF-8")
}

#[test]
fn lint_golden_outputs_are_pinned() {
    for fixture in FIXTURES {
        let stem = stem(fixture);
        let dir = fresh_temp_dir(&format!("lint-golden-{stem}"));

        let text = run_lint(fixture, &["--output", "text"], &dir);
        assert!(text.status.success(), "{fixture} text run failed: {text:?}");
        check_golden(&format!("{stem}.text.golden"), &stdout_utf8(&text));

        let json = run_lint(fixture, &["--output", "json"], &dir);
        assert!(json.status.success(), "{fixture} json run failed: {json:?}");
        check_golden(&format!("{stem}.json.golden"), &stdout_utf8(&json));

        let sarif = run_lint(fixture, &["--sarif"], &dir);
        assert!(
            sarif.status.success(),
            "{fixture} sarif run failed: {sarif:?}"
        );
        check_golden(&format!("{stem}.sarif.golden"), &stdout_utf8(&sarif));

        let fix_plan = run_lint(fixture, &["--fix-plan", "--output", "json"], &dir);
        assert!(
            fix_plan.status.success(),
            "{fixture} fix-plan run failed: {fix_plan:?}"
        );
        check_golden(
            &format!("{stem}.fixplan.json.golden"),
            &stdout_utf8(&fix_plan),
        );

        // --fix mutates its own copy of the fixture in place; the golden here
        // is the resulting file content, not the command's stdout.
        let fix = run_lint(fixture, &["--fix"], &dir);
        assert!(fix.status.success(), "{fixture} fix run failed: {fix:?}");
        let fixed_path = dir.join(fixture);
        let fixed = fs::read_to_string(&fixed_path).expect("read fixed fixture");
        check_golden(&format!("{stem}.fixed.source.golden"), &fixed);
    }
}
