use super::*;

#[test]
fn check_accepts_valid_input() {
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("check")
        .write_stdin("(defun add (x y) (+ x y))")
        .assert()
        .success()
        .stdout("ok\n");
}

#[test]
fn check_rejects_invalid_input() {
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("check")
        .write_stdin("(defun add (x y)")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unclosed list"));
}

#[test]
fn cli_selects_by_path() {
    let mut cmd = paredit();
    cmd.args(["edit", "select", "--path", "0.2"])
        .write_stdin("(defun add (x y) (+ x y))")
        .assert()
        .success()
        .stdout("(x y)");
}

#[test]
fn cli_replaces_by_path() {
    let mut cmd = paredit();
    cmd.args(["edit", "replace", "--path", "0.1", "--with", "sum"])
        .write_stdin("(defun add (x y) (+ x y))")
        .assert()
        .success()
        .stdout("(defun sum (x y) (+ x y))");
}

#[test]
fn cli_detects_emacs_lisp_from_extension() {
    let mut cmd = paredit();
    cmd.args(["inspect", "dialect", "--file", "tests/fixtures/sample.el"])
        .assert()
        .success()
        .stdout("emacs-lisp\n");
}

#[test]
fn cli_prints_definition_outline() {
    let mut cmd = paredit();
    cmd.args(["inspect", "outline", "--file", "tests/fixtures/sample.el"])
        .assert()
        .success()
        .stdout(predicate::str::contains("0\t0..37\tdefun\ttrue"));
}

#[test]
fn cli_reports_selected_form_structure_for_agents() {
    let mut cmd = paredit();
    cmd.args([
        "inspect",
        "form",
        "--dialect",
        "common-lisp",
        "--path",
        "0",
        "--include-source",
        "--output",
        "json",
    ])
    .write_stdin("(defun add (x y) (+ x y))")
    .assert()
    .success()
    .stdout(predicate::str::contains("\"dialect\": \"common-lisp\""))
    .stdout(predicate::str::contains("\"path\": \"0\""))
    .stdout(predicate::str::contains("\"kind\": \"list\""))
    .stdout(predicate::str::contains("\"head\": \"defun\""))
    .stdout(predicate::str::contains("\"definitionLike\": true"))
    .stdout(predicate::str::contains("\"childCount\": 4"))
    .stdout(predicate::str::contains(
        "\"source\": \"(defun add (x y) (+ x y))\"",
    ))
    .stdout(predicate::str::contains("\"symbol\": \"x\""));
}

/// `--at` now reports the path it landed on rather than `null`, which is what
/// makes `inspect form --at` a way of turning a byte offset into a path.
#[test]
fn cli_reports_form_by_byte_offset() {
    let mut cmd = paredit();
    cmd.args(["inspect", "form", "--at", "17", "--output", "json"])
        .write_stdin("(defun add (x y) (+ x y))")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"path\": \"0.3\""))
        .stdout(predicate::str::contains("\"head\": \"+\""))
        .stdout(predicate::str::contains("\"childCount\": 3"));
}

#[test]
fn cli_check_reports_ok_as_json() {
    let mut cmd = paredit();
    cmd.args([
        "inspect",
        "check",
        "--dialect",
        "common-lisp",
        "--output",
        "json",
    ])
    .write_stdin("(defun add (x y) (+ x y))")
    .assert()
    .success()
    .stdout(predicate::str::contains("\"status\": \"ok\""))
    .stdout(predicate::str::contains("\"dialect\": \"common-lisp\""))
    .stdout(predicate::str::contains("\"error\": null"));
}

#[test]
fn cli_check_json_applies_reader_policy_and_preserves_unknown_parsing() {
    for (dialect, input) in [
        ("common-lisp", "(list #\\))"),
        ("emacs-lisp", "(list ?\\))"),
    ] {
        let mut cmd = paredit();
        cmd.args(["inspect", "check", "--dialect", dialect, "--output", "json"])
            .write_stdin(input)
            .assert()
            .success()
            .stdout(predicate::str::contains("\"status\": \"ok\""))
            .stdout(predicate::str::contains(format!(
                "\"dialect\": \"{dialect}\""
            )));
    }

    let mut known = paredit();
    known
        .args([
            "inspect",
            "check",
            "--dialect",
            "common-lisp",
            "--output",
            "json",
        ])
        .write_stdin("#?value")
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"status\": \"error\""))
        .stdout(predicate::str::contains("unsupported reader dispatch"));

    let mut unknown = paredit();
    unknown
        .args(["inspect", "check", "--output", "json"])
        .write_stdin("#?value")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"ok\""))
        .stdout(predicate::str::contains("\"dialect\": \"unknown\""));
}

#[test]
fn cli_check_reports_parse_error_as_json_and_exits_nonzero() {
    let mut cmd = paredit();
    cmd.args(["inspect", "check", "--output", "json"])
        .write_stdin("(defun broken (")
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"status\": \"error\""))
        .stdout(predicate::str::contains("unclosed list"));
}

/// `errors` reports every syntax problem in the file, not only the one the
/// singular `error` field already reported — the round trip Q6 exists to cut.
#[test]
fn cli_check_json_reports_every_syntax_error_not_only_the_first() {
    let mut cmd = paredit();
    let stdout = cmd
        .args(["inspect", "check", "--output", "json"])
        .write_stdin("(foo))\n(bar))\n")
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&stdout).expect("valid JSON");

    assert_eq!(report["status"], "error");
    // The singular field is unchanged: the first problem, as text.
    assert!(
        report["error"]
            .as_str()
            .expect("error")
            .contains("unexpected closing delimiter")
    );
    let errors = report["errors"].as_array().expect("errors array");
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(errors[0]["offset"], 5);
    assert_eq!(errors[1]["offset"], 12);
}

/// A clean file's `errors` array is empty, not merely absent.
#[test]
fn cli_check_json_errors_array_is_empty_for_a_clean_file() {
    let mut cmd = paredit();
    let stdout = cmd
        .args(["inspect", "check", "--output", "json"])
        .write_stdin("(defun add (x y) (+ x y))")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&stdout).expect("valid JSON");
    assert_eq!(report["errors"].as_array(), Some(&Vec::new()));
}

/// Text mode reports every problem too, one line each, before failing on the
/// first — the same "fix one, see the next without re-running" the JSON
/// shape offers.
#[test]
fn cli_check_text_reports_every_syntax_error_one_per_line() {
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("check")
        .write_stdin("(foo))\n(bar))\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "error: unexpected closing delimiter ')' at byte 5",
        ))
        .stdout(predicate::str::contains(
            "error: unexpected closing delimiter ')' at byte 12",
        ));
}

#[test]
fn cli_stats_reports_structural_metrics_as_json() {
    let mut cmd = paredit();
    cmd.args([
        "inspect",
        "stats",
        "--dialect",
        "common-lisp",
        "--output",
        "json",
    ])
    .write_stdin("(defun add (x y) (+ x y))\n(defvar *limit* 10)\n")
    .assert()
    .success()
    .stdout(predicate::str::contains("\"dialect\": \"common-lisp\""))
    .stdout(predicate::str::contains("\"topLevelForms\": 2"))
    .stdout(predicate::str::contains("\"outlineEntries\": 2"));
}
