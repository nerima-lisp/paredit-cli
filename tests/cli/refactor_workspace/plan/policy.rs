use super::*;

#[test]
fn cli_fails_workspace_refactor_plan_policy_after_printing_json() {
    let dir = fresh_temp_dir("workspace refactor plan-policy");
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).expect("create source dir");

    write_fixture(
        &src_dir.join("core.lisp"),
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(defun render-pane (pane) (draw-pane pane))
(defun caller () (render-pane window) render-pane)
"#,
    );
    fs::write(
        src_dir.join("notes.txt"),
        "render-pane is mentioned in plain text",
    )
    .expect("write ignored workspace policy fixture");

    let mut cmd = paredit();
    cmd.args(["refactor", "workspace-plan"])
        .arg("--symbol")
        .arg("render-pane")
        .arg("--operation")
        .arg("rename")
        .arg("--fail-on-blocking-gate")
        .arg("--require-definitions")
        .arg("2")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"workspace\""))
        .stdout(predicate::str::contains("\"discovered_file_count\": 1"))
        .stdout(predicate::str::contains("\"unknown\": 1"))
        .stdout(predicate::str::contains("\"policy\""))
        .stdout(predicate::str::contains("\"decision\""))
        .stdout(predicate::str::contains("\"status\": \"policy_failed\""))
        .stdout(predicate::str::contains(
            "\"next_action\": \"resolve-policy-violations\"",
        ))
        .stdout(predicate::str::contains("\"safe_to_automate\": false"))
        .stdout(predicate::str::contains("\"policy_passed\": false"))
        .stdout(predicate::str::contains("\"name\": \"plan-policy\""))
        .stdout(predicate::str::contains("\"status\": \"failed\""))
        .stdout(predicate::str::contains("\"name\": \"apply-plan\""))
        .stdout(predicate::str::contains("\"status\": \"skipped\""))
        .stdout(predicate::str::contains("\"passed\": false"))
        .stdout(predicate::str::contains("\"fail_on_blocking_gate\": true"))
        .stdout(predicate::str::contains(
            "--require-definitions expected at least 2, found 1",
        ))
        .stderr(predicate::str::contains(
            "refactor workspace-plan policy failed",
        ));
}

#[test]
fn cli_fails_refactor_plan_policy_after_printing_json() {
    let dir = fresh_temp_dir("refactor plan-policy");
    let file = dir.join("core.lisp");
    write_fixture(
        &file,
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(defun render-pane (pane) (draw-pane pane))
(defun caller () (render-pane window) render-pane)
"#,
    );

    let mut cmd = paredit();
    cmd.args(["refactor", "plan"])
        .arg("--symbol")
        .arg("render-pane")
        .arg("--operation")
        .arg("rename")
        .arg("--fail-on-blocking-gate")
        .arg("--require-definitions")
        .arg("2")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"policy\""))
        .stdout(predicate::str::contains("\"decision\""))
        .stdout(predicate::str::contains("\"status\": \"policy_failed\""))
        .stdout(predicate::str::contains(
            "\"next_action\": \"resolve-policy-violations\"",
        ))
        .stdout(predicate::str::contains("\"safe_to_automate\": false"))
        .stdout(predicate::str::contains("\"policy_passed\": false"))
        .stdout(predicate::str::contains("\"name\": \"plan-policy\""))
        .stdout(predicate::str::contains("\"status\": \"failed\""))
        .stdout(predicate::str::contains("\"name\": \"apply-plan\""))
        .stdout(predicate::str::contains("\"status\": \"skipped\""))
        .stdout(predicate::str::contains("\"passed\": false"))
        .stdout(predicate::str::contains("\"fail_on_blocking_gate\": true"))
        .stdout(predicate::str::contains(
            "--require-definitions expected at least 2, found 1",
        ))
        .stderr(predicate::str::contains("refactor plan policy failed"));
}

#[test]
fn cli_marks_refactor_plan_manual_review_when_blocking_gates_do_not_fail_policy() {
    let dir = fresh_temp_dir("refactor plan-manual-review");
    let file = dir.join("core.lisp");
    write_fixture(
        &file,
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(defun render-pane (pane) (draw-pane pane))
(defun render-pane (pane theme) (draw-themed-pane pane theme))
(defun caller () (render-pane window))
"#,
    );

    let output = run_refactor_plan_json(&file, "render-pane", "rename");

    assert!(output.stdout.contains("\"decision\""));
    assert!(output.stdout.contains("\"status\": \"manual_review\""));
    assert!(output.stdout.contains("\"policy_passed\": true"));
    assert!(output.stdout.contains("\"safe_to_automate\": false"));
    assert!(output.stdout.contains("\"blocking_gate_count\": 2"));
    assert!(output.stdout.contains("\"name\": \"manual-review-gates\""));
    assert!(output.stdout.contains("\"status\": \"scheduled\""));
    assert!(
        output
            .stdout
            .contains("\"next_action\": \"review-rename-scope\"")
    );
    assert!(output.stdout.contains("\"risk_summary\""));
    assert!(output.stdout.contains("\"highest_level\": \"warning\""));
    assert!(output.stdout.contains("\"code\": \"ambiguous-definition\""));
    assert!(output.stdout.contains("\"code\": \"signature-mismatch\""));
    assert!(output.stdout.contains("\"command\": null"));

    // FR-003: two blocking gates fired, so the aggregate is "blocking" —
    // outranking the "warning" level either gate carries on its own.
    assert_eq!(
        output.json.pointer("/risk_summary/overall_risk"),
        Some(&serde_json::Value::String("blocking".to_owned()))
    );

    // FR-004: automation cannot proceed, so the suggestion is the command
    // that shows the blocking findings in context, not the apply command.
    let next_commands = output
        .json
        .pointer("/next_commands")
        .and_then(serde_json::Value::as_array)
        .expect("next_commands is present for a blocked plan");
    assert!(
        next_commands.iter().any(|command| command["command"]
            .as_str()
            .is_some_and(|command| command.starts_with("paredit inspect impact"))),
        "{next_commands:?}"
    );
}

#[test]
fn cli_prints_refactor_plan_risk_summary_in_text_output() {
    let dir = fresh_temp_dir("refactor plan-risk-text");
    let file = dir.join("core.lisp");
    write_fixture(
        &file,
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(defun render-pane (pane) (draw-pane pane))
(defun caller () (render-pane window) render-pane)
"#,
    );

    let mut cmd = paredit();
    cmd.args(["refactor", "plan"])
        .arg("--symbol")
        .arg("render-pane")
        .arg("--operation")
        .arg("rename")
        .arg("--output")
        .arg("text")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("risk_highest_level\twarning"))
        .stdout(predicate::str::contains("risk_info_count\t0"))
        .stdout(predicate::str::contains("risk_warning_count\t2"))
        .stdout(predicate::str::contains("risk_error_count\t0"))
        .stdout(predicate::str::contains("risk_blocking_count\t0"))
        .stdout(predicate::str::contains("risk_advisory_count\t2"));
}
