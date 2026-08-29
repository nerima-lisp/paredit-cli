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
        .stdout(predicate::str::contains("0\t0..39\tdefun\ttrue"));
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

// ---------------------------------------------------------------------------
// `inspect check --paredit-config`: a migration aid over `.paredit/rules`,
// which ordinary discovery never sees because a `.paredit` directory is
// hidden. It never fails the run on its own — see the flag's own help — so
// every case below still expects `success()`.
// ---------------------------------------------------------------------------

#[test]
fn cli_check_paredit_config_is_silent_with_no_rule_directory() {
    let dir = fresh_temp_dir("check-paredit-config-absent");
    let mut cmd = paredit();
    cmd.current_dir(&dir)
        .args(["inspect", "check", "--output", "json"])
        .write_stdin("(defun add (x y) (+ x y))")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"paredit_config\": null"));
}

#[test]
fn cli_check_paredit_config_reports_a_syntax_error_in_a_rule_file() {
    let dir = fresh_temp_dir("check-paredit-config-syntax-error");
    let rules = dir.join(".paredit").join("rules");
    fs::create_dir_all(&rules).expect("create rule dir");
    fs::write(rules.join("broken.lisp"), "(defrule r :pattern (f ?x").expect("write broken.lisp");

    let mut cmd = paredit();
    cmd.current_dir(&dir)
        .args(["inspect", "check", "--paredit-config"])
        .write_stdin("(defun add (x y) (+ x y))")
        .assert()
        .success()
        .stdout(predicate::str::contains("paredit-config:"))
        .stdout(predicate::str::contains("broken.lisp"))
        .stdout(predicate::str::contains("unclosed list"))
        // The syntax problem is in the rule directory, not the checked
        // input, so the input's own report is still a plain "ok".
        .stdout(predicate::str::contains("ok\n"));
}

#[test]
fn cli_check_paredit_config_flags_a_rule_that_does_not_read_as_a_pattern() {
    let dir = fresh_temp_dir("check-paredit-config-invalid-pattern");
    let rules = dir.join(".paredit").join("rules");
    fs::create_dir_all(&rules).expect("create rule dir");
    fs::write(
        rules.join("house.lisp"),
        r#"(defrule r :pattern (f ?x:bogus) :message "m")"#,
    )
    .expect("write house.lisp");

    let mut cmd = paredit();
    cmd.current_dir(&dir)
        .args(["inspect", "check", "--paredit-config", "--output", "json"])
        .write_stdin("(defun add (x y) (+ x y))")
        .assert()
        .success()
        .stdout(predicate::str::contains("unknown capture kind"));
}

#[test]
fn cli_check_paredit_config_nudges_a_non_trailing_ellipsis_pattern() {
    let dir = fresh_temp_dir("check-paredit-config-non-trailing-ellipsis");
    let rules = dir.join(".paredit").join("rules");
    fs::create_dir_all(&rules).expect("create rule dir");
    fs::write(
        rules.join("house.lisp"),
        r#"(defrule old-style :pattern (?a ... ?b) :message "m")"#,
    )
    .expect("write house.lisp");

    let mut cmd = paredit();
    cmd.current_dir(&dir)
        .args(["inspect", "check", "--paredit-config", "--output", "json"])
        .write_stdin("(defun add (x y) (+ x y))")
        .assert()
        .success()
        .stdout(predicate::str::contains("old-style"))
        .stdout(predicate::str::contains("`?name...`"));
}

#[test]
fn cli_check_paredit_config_reports_a_clean_directory_with_no_issues() {
    let dir = fresh_temp_dir("check-paredit-config-clean");
    let rules = dir.join(".paredit").join("rules");
    fs::create_dir_all(&rules).expect("create rule dir");
    fs::write(
        rules.join("house.lisp"),
        r#"(defrule house-style :pattern (print ?x) :message "m")"#,
    )
    .expect("write house.lisp");

    let mut cmd = paredit();
    cmd.current_dir(&dir)
        .args(["inspect", "check", "--paredit-config", "--output", "json"])
        .write_stdin("(defun add (x y) (+ x y))")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"path\": \".paredit/rules/house.lisp\"",
        ))
        .stdout(predicate::str::contains("\"issues\": []"))
        .stdout(predicate::str::contains("\"syntax_errors\": []"));
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
