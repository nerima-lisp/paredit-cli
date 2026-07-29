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
