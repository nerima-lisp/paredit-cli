//! `inspect diff` and `refactor patch`: comparing and transplanting by parse.
//!
//! What these assert is the *difference from a text diff*. That a reformatted
//! file reports nothing, that an edited argument reports as that argument, that
//! a change ports to a file which wrote the same form on different lines — each
//! is a case where `diff(1)` gives a different and less useful answer, and each
//! is the reason this exists.

use super::*;

fn write(dir: &std::path::Path, name: &str, source: &str) -> PathBuf {
    let file = dir.join(name);
    fs::write(&file, source).expect("write lisp fixture");
    file
}

fn diff_json(name: &str, old: &str, new: &str) -> serde_json::Value {
    let dir = fresh_temp_dir(name);
    let assert = paredit()
        .args(["inspect", "diff", "--output", "json"])
        .arg(write(&dir, "old.lisp", old))
        .arg(write(&dir, "new.lisp", new))
        .assert()
        .success();
    serde_json::from_slice(&assert.get_output().stdout).expect("diff parses as json")
}

#[test]
fn cli_diff_reports_nothing_for_a_pure_reformatting() {
    let report = diff_json(
        "diff-reformat",
        "(defun f (x) (let ((y 1)) (+ x y)))\n",
        "(defun f (x)\n  (let ((y 1))\n    (+ x\n       y)))\n",
    );
    assert_eq!(report["change_count"], 0);
}

/// The blind spot has to be in the output, not only in the manual.
///
/// An empty structural diff of a change that rewrote every comment in the file
/// is a correct answer to the question this command asks and the wrong answer
/// to the one a reviewer is asking.
#[test]
fn cli_diff_states_that_it_did_not_compare_comments() {
    let report = diff_json(
        "diff-comment",
        "(defun f (x) x)\n",
        ";; this comment is new and wrong\n(defun f (x) x)\n",
    );
    assert_eq!(report["change_count"], 0);
    assert_eq!(report["compares"], "structure");
    assert!(
        report["note"]
            .as_str()
            .is_some_and(|note| note.contains("comments")),
        "{report}"
    );
}

/// A text diff of this reports the whole line. The point is that this does not.
#[test]
fn cli_diff_narrows_to_the_argument_that_changed() {
    let report = diff_json(
        "diff-narrow",
        "(defun f (x) (truncate x 100))\n",
        "(defun f (x) (truncate x 1000))\n",
    );
    assert_eq!(report["change_count"], 1);
    let change = &report["changes"][0];
    assert_eq!(change["kind"], "replaced");
    assert_eq!(change["before"]["text"], "100");
    assert_eq!(change["after"]["text"], "1000");
    assert!(
        change["depth"].as_u64().is_some_and(|depth| depth > 1),
        "{change}"
    );
}

/// A definition inserted above an unchanged one must not drag the unchanged one
/// into the report. This is what the common-subsequence alignment buys.
#[test]
fn cli_diff_leaves_an_unchanged_definition_alone_when_one_is_added_above_it() {
    let report = diff_json(
        "diff-insert",
        "(defun b () 2)\n",
        "(defun a () 1)\n(defun b () 2)\n",
    );
    assert_eq!(report["change_count"], 1);
    assert_eq!(report["changes"][0]["kind"], "inserted");
    assert_eq!(report["changes"][0]["head"], "defun");
}

#[test]
fn cli_diff_fail_on_change_gates_a_formatting_only_rewrite() {
    let dir = fresh_temp_dir("diff-gate");
    let old = write(&dir, "old.lisp", "(defun f (x) (+ x 1))\n");
    let same = write(&dir, "same.lisp", "(defun f (x)\n  (+ x 1))\n");
    let changed = write(&dir, "changed.lisp", "(defun f (x) (+ x 2))\n");

    paredit()
        .args(["inspect", "diff", "--fail-on-change"])
        .arg(&old)
        .arg(&same)
        .assert()
        .success();

    paredit()
        .args(["inspect", "diff", "--fail-on-change"])
        .arg(&old)
        .arg(&changed)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("inspect diff policy failed"));
}

/// The whole point of `refactor patch`: the target wrote the same form with
/// different indentation, in a different definition, and the change still lands.
#[test]
fn cli_patch_carries_a_change_into_a_differently_formatted_file() {
    let dir = fresh_temp_dir("patch-carry");
    let from = write(&dir, "from.lisp", "(defun r (xs)\n  (car (reverse xs)))\n");
    let to = write(&dir, "to.lisp", "(defun r (xs)\n  (first (reverse xs)))\n");
    let target = write(
        &dir,
        "target.lisp",
        "(defun elsewhere (xs)\n  (let ((last-one\n          (car\n            (reverse xs))))\n    last-one))\n",
    );

    let assert = paredit()
        .args(["refactor", "patch", "--output", "json", "--write"])
        .arg("--from")
        .arg(&from)
        .arg("--to")
        .arg(&to)
        .arg("--apply-to")
        .arg(&target)
        .assert()
        .success();
    let plan: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("plan parses");

    assert_eq!(plan["written"], true);
    assert_eq!(plan["changes"][0]["outcome"], "applied");
    // The anchor widened past the bare `car`, which alone would have matched
    // nothing useful.
    assert_eq!(plan["changes"][0]["anchor_widened"], true);

    let patched = fs::read_to_string(&target).expect("read patched target");
    assert!(patched.contains("(first (reverse xs))"), "{patched}");
}

#[test]
fn cli_patch_plans_without_writing_by_default() {
    let dir = fresh_temp_dir("patch-plan-only");
    let from = write(&dir, "from.lisp", "(f (g 1))\n");
    let to = write(&dir, "to.lisp", "(f (h 1))\n");
    let target = write(&dir, "target.lisp", "(other (g 1))\n");

    paredit()
        .args(["refactor", "patch", "--output", "text"])
        .arg("--from")
        .arg(&from)
        .arg("--to")
        .arg(&to)
        .arg("--apply-to")
        .arg(&target)
        .assert()
        .success()
        .stdout(predicate::str::contains("written\tfalse"))
        .stdout(predicate::str::contains("applied\t1"));

    assert_eq!(
        fs::read_to_string(&target).expect("read target"),
        "(other (g 1))\n",
        "a plan must not write"
    );
}

/// Two sites match and only the caller knows whether both have the bug.
#[test]
fn cli_patch_refuses_an_ambiguous_change_until_all_is_given() {
    let dir = fresh_temp_dir("patch-ambiguous");
    let from = write(&dir, "from.lisp", "(f (g 1))\n");
    let to = write(&dir, "to.lisp", "(f (h 1))\n");
    let target = write(&dir, "target.lisp", "(one (g 1))\n(two (g 1))\n");

    paredit()
        .args(["refactor", "patch", "--output", "text", "--write"])
        .arg("--from")
        .arg(&from)
        .arg("--to")
        .arg(&to)
        .arg("--apply-to")
        .arg(&target)
        .assert()
        .success()
        .stdout(predicate::str::contains("ambiguous\t1"));
    assert_eq!(
        fs::read_to_string(&target).expect("read target"),
        "(one (g 1))\n(two (g 1))\n",
    );

    paredit()
        .args(["refactor", "patch", "--output", "text", "--write", "--all"])
        .arg("--from")
        .arg(&from)
        .arg("--to")
        .arg(&to)
        .arg("--apply-to")
        .arg(&target)
        .assert()
        .success();
    let patched = fs::read_to_string(&target).expect("read target");
    assert_eq!(patched.matches("(h 1)").count(), 2, "{patched}");
}

/// A partial port is the failure worth gating on: the file was written, some
/// sites are fixed, and the rest is left behind for nobody to notice.
#[test]
fn cli_patch_fail_on_unapplied_catches_a_partial_port() {
    let dir = fresh_temp_dir("patch-partial");
    let from = write(&dir, "from.lisp", "(f (g 1))\n");
    let to = write(&dir, "to.lisp", "(f (h 1))\n");
    let target = write(&dir, "target.lisp", "(unrelated 1)\n");

    paredit()
        .args([
            "refactor",
            "patch",
            "--output",
            "text",
            "--fail-on-unapplied",
        ])
        .arg("--from")
        .arg(&from)
        .arg("--to")
        .arg(&to)
        .arg("--apply-to")
        .arg(&target)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("not-found\t1"))
        .stderr(predicate::str::contains("refactor patch policy failed"));
}

#[test]
fn cli_patch_diff_previews_without_writing() {
    let dir = fresh_temp_dir("patch-diff");
    let from = write(&dir, "from.lisp", "(f (g 1))\n");
    let to = write(&dir, "to.lisp", "(f (h 1))\n");
    let target = write(&dir, "target.lisp", "(other (g 1))\n");

    paredit()
        .args(["refactor", "patch", "--diff"])
        .arg("--from")
        .arg(&from)
        .arg("--to")
        .arg(&to)
        .arg("--apply-to")
        .arg(&target)
        .assert()
        .success()
        .stdout(predicate::str::contains("-(other (g 1))"))
        .stdout(predicate::str::contains("+(other (h 1))"));

    assert_eq!(
        fs::read_to_string(&target).expect("read target"),
        "(other (g 1))\n",
    );
}
