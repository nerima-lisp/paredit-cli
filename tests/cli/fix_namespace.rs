//! The `fix` namespace end to end.
//!
//! `fix` reimplements nothing — each leaf builds the `inspect lint` arguments
//! its old spelling would have produced. So the tests worth having are the
//! ones that pin *that*: byte-identical output between the two spellings. A
//! test that only checked `fix apply` worked would pass just as happily if the
//! façade had quietly grown a second code path.

use super::*;

/// Two findings from two different fixable rules, so a divergence between the
/// two spellings has somewhere to show up.
const FIXABLE: &str = "(defun foo (x)\n  (setf x (1+ x))\n  (progn (print x)))\n";

fn workspace(name: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    fs::write(dir.join("a.lisp"), FIXABLE).expect("write fixture");
    dir
}

#[test]
fn fix_apply_writes_exactly_what_inspect_lint_fix_writes() {
    let through_fix = workspace("fix-apply-new");
    let through_lint = workspace("fix-apply-old");

    paredit()
        .args(["fix", "apply"])
        .arg(&through_fix)
        .assert()
        .success();
    paredit()
        .args(["inspect", "lint", "--fix"])
        .arg(&through_lint)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(through_fix.join("a.lisp")).expect("read back"),
        fs::read_to_string(through_lint.join("a.lisp")).expect("read back"),
        "the façade must not have grown a second fixing path"
    );
}

#[test]
fn fix_plan_emits_the_same_plan_as_the_flag_it_replaces() {
    let dir = workspace("fix-plan");
    let through_fix = paredit()
        .args(["fix", "plan"])
        .arg(&dir)
        .output()
        .expect("run fix plan");
    let through_lint = paredit()
        .args(["inspect", "lint", "--fix-plan"])
        .arg(&dir)
        .output()
        .expect("run inspect lint --fix-plan");

    assert!(through_fix.status.success());
    assert_eq!(through_fix.stdout, through_lint.stdout);
}

#[test]
fn fix_check_gates_on_pending_fixes_and_writes_nothing() {
    let dir = workspace("fix-check");
    paredit().args(["fix", "check"]).arg(&dir).assert().code(3);
    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        FIXABLE
    );

    paredit()
        .args(["fix", "apply"])
        .arg(&dir)
        .assert()
        .success();
    paredit()
        .args(["fix", "check"])
        .arg(&dir)
        .assert()
        .success();
}

#[test]
fn fix_apply_diff_previews_without_writing() {
    let dir = workspace("fix-diff");
    paredit()
        .args(["fix", "apply", "--diff"])
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("---"));
    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        FIXABLE
    );
}

/// `--diff` previews a different payload than `--compact` prints a headline
/// for, and `--diff` makes no write at all — so, like its sibling
/// `--group-by-impact-area`, `--compact` must be refused alongside `--diff`
/// rather than silently doing nothing while `--diff`'s early return wins.
#[test]
fn fix_apply_compact_and_diff_are_refused_together() {
    let dir = workspace("fix-apply-compact-diff");
    paredit()
        .args(["fix", "apply", "--compact", "--diff"])
        .arg(&dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

/// The whole reason `fix list` exists: `--list-rules` answers "is this rule
/// fixable" as a column over 170-odd rows, and a caller about to run a fixer
/// is asking which rows those are.
#[test]
fn fix_list_reports_only_the_fixable_rules() {
    let output = paredit()
        .args(["fix", "list"])
        .output()
        .expect("run fix list");
    assert!(output.status.success());

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    let rules = report["rules"].as_array().expect("rules");
    assert!(!rules.is_empty());
    assert!(
        rules.iter().all(|rule| rule["fixable"] == true),
        "fix list must not list a rule with no fix"
    );

    let all = paredit()
        .args(["inspect", "lint", "--list-rules"])
        .output()
        .expect("run inspect lint --list-rules");
    let all: serde_json::Value = serde_json::from_slice(&all.stdout).expect("json");
    assert!(
        all["rules"].as_array().expect("rules").len() > rules.len(),
        "the full catalogue must be strictly larger than the fixable subset"
    );
}

#[test]
fn rule_selection_narrows_a_fix_run_the_way_it_narrows_a_lint_run() {
    let dir = workspace("fix-rule-selection");
    paredit()
        .args(["fix", "apply", "--rule", "manual-incf"])
        .arg(&dir)
        .assert()
        .success();
    let fixed = fs::read_to_string(dir.join("a.lisp")).expect("read back");
    assert!(
        fixed.contains("(incf x)"),
        "the named rule's fix must have run: {fixed}"
    );
    assert!(
        fixed.contains("(progn (print x))"),
        "a rule that was not named must have been left alone: {fixed}"
    );
}

/// `paredit fix apply` with no arguments used to scan nothing, report zero
/// fixes, and exit zero — which reads exactly like a clean codebase.
#[test]
fn a_fix_run_with_no_files_is_refused_rather_than_reported_as_clean() {
    for leaf in ["apply", "check", "plan"] {
        paredit()
            .args(["fix", leaf])
            .assert()
            .failure()
            .stderr(predicate::str::contains("no files to fix"));
    }
    paredit().args(["fix", "list"]).assert().success();
}

/// FR-006b: `fix apply`'s JSON always carries `headline`, and `--compact`
/// text output is that headline and nothing else.
#[test]
fn fix_apply_reports_headline_and_compact_text_output() {
    let dir = workspace("fix-apply-headline");

    paredit()
        .args(["fix", "apply"])
        .arg(&dir)
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"headline\": \"1 modified definition.\"",
        ));

    let compact_dir = workspace("fix-apply-headline-compact");
    paredit()
        .args(["fix", "apply", "--compact"])
        .arg(&compact_dir)
        .arg("--output")
        .arg("text")
        .assert()
        .success()
        .stdout("1 modified definition.\n");

    let verbose_dir = workspace("fix-apply-headline-verbose-text");
    paredit()
        .args(["fix", "apply"])
        .arg(&verbose_dir)
        .arg("--output")
        .arg("text")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "headline\t1 modified definition.\n",
        ))
        .stdout(predicate::str::contains("fixes_applied\t"));
}

/// FR-006b, continued: `--compact` only changes what gets *printed* — the
/// tests above pin the headline text, but none of them read the file back,
/// so a bug that made `--compact` skip or truncate the write itself could
/// still slip through with a green headline assertion.
#[test]
fn fix_apply_compact_writes_the_file_correctly() {
    let dir = workspace("fix-apply-compact-write");
    paredit()
        .args(["fix", "apply", "--compact"])
        .arg(&dir)
        .arg("--output")
        .arg("text")
        .assert()
        .success()
        .stdout("1 modified definition.\n");

    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        "(defun foo (x)\n  (incf x)\n  (print x))\n",
        "--compact must not change what actually gets written, only what gets printed"
    );
}

/// FR-006b, continued: `inspect lint --fix` reaches the exact same rendering
/// code, so it must carry the same `--compact`/`headline` behavior — not
/// merely a bytes-on-disk match, but the reported JSON too.
#[test]
fn fix_apply_and_inspect_lint_fix_report_identical_json_including_the_new_fields() {
    let through_fix = workspace("fix-apply-new-fields-new");
    let through_lint = workspace("fix-apply-new-fields-old");

    let via_fix = paredit()
        .args(["fix", "apply"])
        .arg(&through_fix)
        .arg("--output")
        .arg("json")
        .output()
        .expect("run fix apply");
    let via_lint = paredit()
        .args(["inspect", "lint", "--fix"])
        .arg(&through_lint)
        .arg("--output")
        .arg("json")
        .output()
        .expect("run inspect lint --fix");

    assert!(via_fix.status.success());
    let fix_stdout = String::from_utf8(via_fix.stdout).expect("fix apply stdout is utf8");
    let lint_stdout =
        String::from_utf8(via_lint.stdout).expect("inspect lint --fix stdout is utf8");
    assert_eq!(
        fix_stdout.replace(
            &through_fix.display().to_string(),
            &through_lint.display().to_string()
        ),
        lint_stdout,
        "the two spellings must report identical JSON (paths aside), including headline and impact_area_groups"
    );
}

/// FR-007b: `--group-by-impact-area` groups changed files by their declared
/// package, writes each group as its own transaction, and continues to the
/// next group when one group's write fails — the same partial-failure
/// resilience `refactor apply --group-by-impact-area` established, reported
/// through `impact_area_groups`.
#[cfg(unix)]
#[test]
fn fix_apply_group_by_impact_area_continues_after_one_group_fails() {
    use std::os::unix::fs::PermissionsExt;

    let dir = fresh_temp_dir("fix-apply-group-by-impact-area");
    let writable_file = dir.join("app.lisp");
    let readonly_dir = dir.join("readonly");
    let blocked_file = readonly_dir.join("util.lisp");
    let writable_original = "(in-package :app)\n(setf x (1+ x))\n";
    let blocked_original = "(in-package :util)\n(setf y (1+ y))\n";
    fs::write(&writable_file, writable_original).expect("write writable fixture");
    fs::create_dir_all(&readonly_dir).expect("create readonly dir");
    fs::write(&blocked_file, blocked_original).expect("write blocked fixture");

    fs::set_permissions(&readonly_dir, fs::Permissions::from_mode(0o555))
        .expect("chmod readonly dir");

    let mut apply = paredit();
    let assert = apply
        .args(["fix", "apply", "--group-by-impact-area"])
        .arg(&writable_file)
        .arg(&blocked_file)
        .arg("--output")
        .arg("json")
        .assert();

    fs::set_permissions(&readonly_dir, fs::Permissions::from_mode(0o755))
        .expect("restore readonly dir permissions");

    // One group's write failed and one succeeded: not everything failed, so
    // this is a warning on stderr and a zero exit, with the failed group's
    // file left untouched.
    let assert = assert
        .success()
        .stderr(predicate::str::contains("Permission denied"));
    let json: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("fix apply output is json");

    let groups = json
        .pointer("/impact_area_groups")
        .and_then(serde_json::Value::as_array)
        .expect("impact_area_groups is present");
    assert_eq!(groups.len(), 2, "{groups:?}");
    assert!(
        groups
            .iter()
            .any(|group| group["group"] == ":app" && group["written"] == true),
        "{groups:?}"
    );
    assert!(
        groups
            .iter()
            .any(|group| group["group"] == ":util" && group["written"] == false),
        "{groups:?}"
    );

    assert_eq!(
        fs::read_to_string(&writable_file).expect("read written app fixture"),
        "(in-package :app)\n(incf x)\n"
    );
    assert_eq!(
        fs::read_to_string(&blocked_file).expect("read unwritten util fixture"),
        blocked_original
    );
}

/// FR-005b: `fix apply --no-destructive-fixes` deliberately leaves a class of
/// fixes unapplied, and that is a real reason to suggest `fix plan` — unlike
/// the fixed-point loop's own per-pass conflict count, which converges to
/// zero pending fixes within the same run and would make the same suggestion
/// false.
#[test]
fn fix_apply_next_commands_suggest_fix_plan_when_destructive_fixes_are_skipped() {
    let dir = fresh_temp_dir("fix-apply-next-commands-destructive");
    fs::write(
        dir.join("a.lisp"),
        "(defun f (cbd) (nreverse (copy-list cbd)))\n",
    )
    .expect("write destructive-fixable fixture");

    let mut apply = paredit();
    let assert = apply
        .args(["fix", "apply", "--no-destructive-fixes"])
        .arg(&dir)
        .arg("--output")
        .arg("json")
        .assert()
        .success();
    let json: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("fix apply output is json");

    // The destructive fix was left in place.
    assert_eq!(
        fs::read_to_string(dir.join("a.lisp")).expect("read back"),
        "(defun f (cbd) (nreverse (copy-list cbd)))\n"
    );

    let next_commands = json
        .pointer("/next_commands")
        .and_then(serde_json::Value::as_array)
        .expect("next_commands is present when a destructive fix was skipped");
    assert!(
        next_commands.iter().any(|command| {
            command["command"]
                .as_str()
                .is_some_and(|command| command.contains("fix plan"))
        }),
        "{next_commands:?}"
    );

    // `fix plan` (unfiltered by --no-destructive-fixes) does show it, which
    // is what makes the suggestion above true rather than a dead end.
    paredit()
        .args(["fix", "plan"])
        .arg(&dir)
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("copy-before-destructive"));
}

/// FR-005b, the plan half: a `fix plan` that finds fixable findings suggests
/// `fix apply`, the command that applies them.
#[test]
fn fix_plan_next_commands_suggest_fix_apply_when_fixes_are_available() {
    let dir = workspace("fix-plan-next-commands");

    let assert = paredit()
        .args(["fix", "plan"])
        .arg(&dir)
        .arg("--output")
        .arg("json")
        .assert()
        .success();
    let json: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("fix plan output is json");

    let next_commands = json
        .pointer("/next_commands")
        .and_then(serde_json::Value::as_array)
        .expect("next_commands is present for a non-empty plan");
    assert!(
        next_commands.iter().any(|command| {
            command["command"]
                .as_str()
                .is_some_and(|command| command.contains("fix apply"))
        }),
        "{next_commands:?}"
    );
}

/// An empty plan has nothing to suggest applying.
#[test]
fn fix_plan_suggests_nothing_when_there_is_nothing_to_fix() {
    let dir = fresh_temp_dir("fix-plan-next-commands-empty");
    fs::write(dir.join("a.lisp"), "(defun f (x) x)\n").expect("write clean fixture");

    let assert = paredit()
        .args(["fix", "plan"])
        .arg(&dir)
        .arg("--output")
        .arg("json")
        .assert()
        .success();
    let json: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("fix plan output is json");
    assert!(
        json.pointer("/next_commands").is_none(),
        "a plan with nothing fixable must not suggest fix apply: {json}"
    );
}

/// `--compact` and `--group-by-impact-area` are new flags on the older
/// `inspect lint --fix` spelling too, since both entry points share the
/// workflow that grew them.
#[test]
fn inspect_lint_fix_also_accepts_compact_and_group_by_impact_area() {
    let dir = workspace("inspect-lint-fix-compact");
    paredit()
        .args(["inspect", "lint", "--fix", "--compact"])
        .arg(&dir)
        .arg("--output")
        .arg("text")
        .assert()
        .success()
        .stdout("1 modified definition.\n");

    let group_dir = fresh_temp_dir("inspect-lint-fix-group-by-impact-area");
    fs::write(
        group_dir.join("a.lisp"),
        "(in-package :app)\n(setf x (1+ x))\n",
    )
    .expect("write fixture");
    paredit()
        .args(["inspect", "lint", "--fix", "--group-by-impact-area"])
        .arg(&group_dir)
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"group\": \":app\""));
}
