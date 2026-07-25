use super::*;

#[test]
fn cli_aggregates_findings_from_multiple_rules() {
    let dir = fresh_temp_dir("lint-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn (setq x x) (eql y \"z\"))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 2"))
        .stdout(predicate::str::contains("\"self-assignment\""))
        .stdout(predicate::str::contains("\"eql-string-comparison\""));
}

#[test]
fn cli_lint_expands_a_directory_argument_recursively() {
    let dir = fresh_temp_dir("lint-report-dir");
    fs::write(dir.join("a.lisp"), "(setq x x)\n").expect("write a.lisp");
    let sub = dir.join("sub");
    std::fs::create_dir_all(&sub).expect("create sub dir");
    fs::write(sub.join("c.lisp"), "(eq n 5)\n").expect("write c.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 2"))
        .stdout(predicate::str::contains("\"self-assignment\""))
        .stdout(predicate::str::contains("\"eq-number-comparison\""));
}

#[test]
fn cli_rule_selection_runs_only_the_named_rule() {
    let dir = fresh_temp_dir("lint-report-rule");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn (setq x x) (eql y \"z\"))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("self-assignment")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"self-assignment\""))
        .stdout(predicate::str::contains("eql-string-comparison").not());
}

#[test]
fn cli_exclude_selection_drops_the_named_rule() {
    let dir = fresh_temp_dir("lint-report-exclude");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn (setq x x) (eql y \"z\"))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--exclude")
        .arg("self-assignment")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"eql-string-comparison\""))
        .stdout(predicate::str::contains("\"self-assignment\"").not());
}

#[test]
fn cli_rejects_an_unknown_rule_name() {
    let dir = fresh_temp_dir("lint-report-unknown-rule");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("not-a-real-rule")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown lint rule"));
}

#[test]
fn cli_lists_every_rule_in_per_rule() {
    let dir = fresh_temp_dir("lint-report-per-rule");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate-boolean-operands\""))
        .stdout(predicate::str::contains("\"duplicate-case-keys\""));
}

#[test]
fn cli_reports_no_findings_for_a_clean_file() {
    let dir = fresh_temp_dir("lint-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun add (a b) (+ a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_lint_fail_on_finding_trips_gate() {
    let dir = fresh_temp_dir("lint-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fail-on-finding")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("lint-report policy failed"));
}

#[test]
fn cli_lint_passes_gate_for_a_clean_file() {
    let dir = fresh_temp_dir("lint-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun add (a b) (+ a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fail-on-finding")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_lint_list_rules_prints_the_catalog_without_files() {
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--list-rules")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"rule_count\": 85"))
        .stdout(predicate::str::contains("\"self-assignment\""))
        .stdout(predicate::str::contains(
            "a setq/setf/psetq/psetf that assigns a place to itself",
        ));
}

#[test]
fn cli_list_rules_filters_by_category() {
    let output = paredit()
        .arg("inspect")
        .arg("lint")
        .arg("--list-rules")
        .arg("--category")
        .arg("dead-code")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let value: serde_json::Value = serde_json::from_slice(&output).expect("catalog JSON is valid");
    let rules = value["rules"].as_array().expect("rules array");
    // Every listed rule is in the requested category, and it's a strict subset.
    assert!(!rules.is_empty());
    assert!(rules.iter().all(|r| r["category"] == "dead-code"));
    assert!(rules.len() < 85);
    assert_eq!(value["rule_count"], rules.len());
}

#[test]
fn cli_list_rules_filters_by_rule_name() {
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--list-rules")
        .arg("--rule")
        .arg("redundant-identity")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"rule_count\": 1"))
        .stdout(predicate::str::contains("\"rule\": \"redundant-identity\""))
        .stdout(predicate::str::contains("redundant-quote").not());
}

#[test]
fn cli_list_rules_rejects_an_unknown_category() {
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--list-rules")
        .arg("--category")
        .arg("not-a-category")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown lint category"));
}

#[test]
fn cli_lint_requires_files_without_list_rules() {
    let mut cmd = paredit();
    cmd.arg("inspect").arg("lint").assert().failure().code(2);
}

#[test]
fn cli_lint_per_rule_carries_descriptions() {
    let dir = fresh_temp_dir("lint-report-descriptions");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"description\""))
        .stdout(predicate::str::contains(
            "a setq/setf/psetq/psetf that assigns a place to itself",
        ));
}

#[test]
fn cli_lint_list_rules_includes_categories() {
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--list-rules")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"categories\""))
        .stdout(predicate::str::contains("\"category\": \"arity\""));
}

#[test]
fn cli_lint_category_runs_only_that_category() {
    let dir = fresh_temp_dir("lint-report-category");
    let file = dir.join("a.lisp");
    // An arity finding (setf-arity) and a suspicious finding (self-assignment).
    fs::write(&file, "(setq a 1 b)\n(setq x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--category")
        .arg("arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"setf-arity\""))
        .stdout(predicate::str::contains("self-assignment").not());
}

#[test]
fn cli_lint_rejects_an_unknown_category() {
    let dir = fresh_temp_dir("lint-report-unknown-category");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--category")
        .arg("not-a-category")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown lint category"));
}

#[test]
fn cli_lint_rule_and_category_conflict() {
    let dir = fresh_temp_dir("lint-report-conflict");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("setf-arity")
        .arg("--category")
        .arg("arity")
        .arg(&file)
        .assert()
        .failure()
        .code(2);
}

#[test]
fn cli_lint_sarif_emits_a_valid_report() {
    let dir = fresh_temp_dir("lint-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x)\n  (setq x x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"version\": \"2.1.0\""))
        .stdout(predicate::str::contains("\"ruleId\": \"self-assignment\""))
        .stdout(predicate::str::contains("\"startLine\": 2"))
        .stdout(predicate::str::contains("\"level\": \"error\""));
}

#[test]
fn cli_lint_sarif_respects_category_and_gate() {
    let dir = fresh_temp_dir("lint-report-sarif-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq a 1 b)\n").expect("write a.lisp");

    // The arity finding is present, so --fail-on-finding trips the gate.
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg("--category")
        .arg("arity")
        .arg("--fail-on-finding")
        .arg(&file)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("\"ruleId\": \"setf-arity\""));
}

#[test]
fn cli_lint_sarif_conflicts_with_list_rules() {
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg("--list-rules")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn cli_lint_sarif_includes_stable_fingerprints() {
    let dir = fresh_temp_dir("lint-report-sarif-fp");
    // Two identical self-assignment lines get distinct fingerprints; a blank
    // line above does not change the content-based hash.
    let a = dir.join("a.lisp");
    fs::write(&a, "(setq x x)\n(setq x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg("--rule")
        .arg("self-assignment")
        .arg(&a)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"partialFingerprints\""))
        .stdout(predicate::str::contains("\"primaryLocationLineHash\""))
        .stdout(predicate::str::contains(":0"))
        .stdout(predicate::str::contains(":1"));
}

#[test]
fn cli_lint_github_emits_annotations() {
    let dir = fresh_temp_dir("lint-report-github");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x)\n  (setq x x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--github")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("::error file="))
        .stdout(predicate::str::contains("line=2"))
        .stdout(predicate::str::contains("self-assignment:"));
}

#[test]
fn cli_lint_github_respects_gate_and_filtering() {
    let dir = fresh_temp_dir("lint-report-github-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq a 1 b)\n(setq x x)\n").expect("write a.lisp");

    // --category arity keeps only the setf-arity finding, and the gate trips.
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--github")
        .arg("--category")
        .arg("arity")
        .arg("--fail-on-finding")
        .arg(&file)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("setf-arity:"))
        .stdout(predicate::str::contains("self-assignment").not());
}

#[test]
fn cli_lint_github_conflicts_with_sarif() {
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--github")
        .arg("--sarif")
        .arg("x.lisp")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn cli_lint_sarif_includes_a_fix_for_redundant_quote() {
    let dir = fresh_temp_dir("lint-report-sarif-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defparameter *n* '5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg("--rule")
        .arg("redundant-quote")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes\""))
        .stdout(predicate::str::contains("\"insertedContent\""))
        .stdout(predicate::str::contains("Remove the redundant quote"));
}

#[test]
fn cli_lint_sarif_includes_a_fix_for_redundant_progn() {
    let dir = fresh_temp_dir("lint-report-sarif-progn-fix");
    let file = dir.join("a.lisp");
    // The fix must copy the inner form's exact source, keeping the `'` prefix.
    fs::write(&file, "(defun f () (progn '(a b)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg("--rule")
        .arg("redundant-progn")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes\""))
        .stdout(predicate::str::contains("Unwrap the redundant progn"))
        .stdout(predicate::str::contains("'(a b)"));
}

#[test]
fn cli_lint_write_baseline_then_baseline_suppresses_known_findings() {
    let dir = fresh_temp_dir("lint-baseline-known");
    let file = dir.join("a.lisp");
    let base = dir.join("base.json");
    fs::write(&file, "(incf 5)\n(< x)\n").expect("write a.lisp");

    // Generate the baseline of the two current findings.
    paredit()
        .arg("inspect")
        .arg("lint")
        .arg("--write-baseline")
        .arg(&base)
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"entry_count\": 2"));

    // Re-linting with the baseline hides both known findings.
    paredit()
        .arg("inspect")
        .arg("lint")
        .arg("--baseline")
        .arg(&base)
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_lint_baseline_reports_only_new_findings() {
    let dir = fresh_temp_dir("lint-baseline-new");
    let file = dir.join("a.lisp");
    let base = dir.join("base.json");
    fs::write(&file, "(incf 5)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "lint", "--write-baseline"])
        .arg(&base)
        .arg(&file)
        .assert()
        .success();

    // Add a new finding alongside the baselined one.
    fs::write(&file, "(incf 5)\n(format \"~a\" y)\n").expect("rewrite a.lisp");

    paredit()
        .arg("inspect")
        .arg("lint")
        .arg("--baseline")
        .arg(&base)
        .arg("--fail-on-finding")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("format-missing-destination"));
}

#[test]
fn cli_lint_baseline_survives_line_shifts() {
    let dir = fresh_temp_dir("lint-baseline-shift");
    let file = dir.join("a.lisp");
    let base = dir.join("base.json");
    fs::write(&file, "(incf 5)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "lint", "--write-baseline"])
        .arg(&base)
        .arg(&file)
        .assert()
        .success();

    // Insert unrelated lines ABOVE the baselined finding; its line number
    // changes but the content-hash identity does not.
    fs::write(&file, ";; a new comment\n(defun g () nil)\n(incf 5)\n").expect("rewrite a.lisp");

    paredit()
        .arg("inspect")
        .arg("lint")
        .arg("--baseline")
        .arg(&base)
        .arg("--rule")
        .arg("literal-place")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_lint_baseline_missing_file_is_an_error() {
    let dir = fresh_temp_dir("lint-baseline-missing");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf 5)\n").expect("write a.lisp");

    paredit()
        .arg("inspect")
        .arg("lint")
        .arg("--baseline")
        .arg(dir.join("does-not-exist.json"))
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("reading baseline file"));
}

#[test]
fn cli_lint_stats_rolls_up_by_severity_category_and_rule() {
    let dir = fresh_temp_dir("lint-stats");
    let file = dir.join("a.lisp");
    // literal-place (malformed/error) + redundant-quote (suspicious/warning).
    fs::write(&file, "(incf 5)\n(list '5)\n").expect("write a.lisp");

    let output = paredit()
        .args(["inspect", "lint", "--stats", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stats: serde_json::Value = serde_json::from_slice(&output).expect("stats JSON is valid");
    assert_eq!(stats["finding_count"], 2);
    assert_eq!(stats["files_scanned"], 1);
    assert_eq!(stats["files_with_findings"], 1);

    let severity = |name: &str| {
        stats["by_severity"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["severity"] == name)
            .map(|s| s["count"].as_u64().unwrap())
            .unwrap_or(0)
    };
    assert_eq!(severity("error"), 1);
    assert_eq!(severity("warning"), 1);

    // by_category has one entry per category (all five).
    assert_eq!(stats["by_category"].as_array().unwrap().len(), 5);
    // by_rule lists exactly the two firing rules.
    assert_eq!(stats["by_rule"].as_array().unwrap().len(), 2);
}

#[test]
fn cli_lint_stats_respects_the_category_filter() {
    let dir = fresh_temp_dir("lint-stats-category");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf 5)\n(list '5)\n").expect("write a.lisp");

    let output = paredit()
        .args([
            "inspect",
            "lint",
            "--stats",
            "--category",
            "malformed",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stats: serde_json::Value = serde_json::from_slice(&output).expect("stats JSON is valid");
    // Only literal-place (malformed) counts; redundant-quote is filtered out.
    assert_eq!(stats["finding_count"], 1);
    assert_eq!(stats["by_rule"].as_array().unwrap().len(), 1);
}

#[test]
fn cli_lint_stats_conflicts_with_fix() {
    let dir = fresh_temp_dir("lint-stats-conflict");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "lint", "--stats", "--fix"])
        .arg(&file)
        .assert()
        .failure();
}

#[test]
fn cli_lint_fail_on_error_ignores_style_warnings() {
    let dir = fresh_temp_dir("lint-fail-on-error-warn");
    let file = dir.join("a.lisp");
    // redundant-quote is a warning-severity rule.
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    paredit()
        .arg("inspect")
        .arg("lint")
        .arg("--fail-on")
        .arg("error")
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn cli_lint_fail_on_error_trips_on_a_bug() {
    let dir = fresh_temp_dir("lint-fail-on-error-bug");
    let file = dir.join("a.lisp");
    // literal-place is an error-severity rule.
    fs::write(&file, "(incf 5)\n").expect("write a.lisp");

    paredit()
        .arg("inspect")
        .arg("lint")
        .arg("--fail-on")
        .arg("error")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("severity error or higher"));
}

#[test]
fn cli_lint_fail_on_warning_trips_on_any_finding() {
    let dir = fresh_temp_dir("lint-fail-on-warning");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    paredit()
        .arg("inspect")
        .arg("lint")
        .arg("--fail-on")
        .arg("warning")
        .arg(&file)
        .assert()
        .code(3);
}

#[test]
fn cli_lint_sarif_level_reflects_severity() {
    let dir = fresh_temp_dir("lint-sarif-severity");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n(incf 5)\n").expect("write a.lisp");

    let output = paredit()
        .args(["inspect", "lint", "--sarif"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let sarif: serde_json::Value = serde_json::from_slice(&output).expect("SARIF is valid JSON");
    let results = sarif["runs"][0]["results"].as_array().expect("results");
    let level_of = |rule: &str| {
        results
            .iter()
            .find(|r| r["ruleId"] == rule)
            .map(|r| r["level"].as_str().unwrap().to_owned())
            .unwrap_or_default()
    };
    assert_eq!(level_of("redundant-quote"), "warning");
    assert_eq!(level_of("literal-place"), "error");
}

#[test]
fn cli_lint_list_rules_marks_severity() {
    let output = paredit()
        .args(["inspect", "lint", "--list-rules", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let catalog: serde_json::Value =
        serde_json::from_slice(&output).expect("--list-rules JSON is valid");
    let rules = catalog["rules"].as_array().expect("rules array");
    let severity_of = |rule: &str| {
        rules
            .iter()
            .find(|r| r["rule"] == rule)
            .map(|r| r["severity"].as_str().unwrap().to_owned())
            .unwrap_or_default()
    };
    assert_eq!(severity_of("redundant-quote"), "warning");
    assert_eq!(severity_of("literal-place"), "error");
    // Exactly five rules are warnings; the rest are errors.
    let warnings = rules.iter().filter(|r| r["severity"] == "warning").count();
    assert_eq!(warnings, 45);
}

#[test]
fn cli_lint_list_rules_marks_fixability() {
    let output = paredit()
        .args(["inspect", "lint", "--list-rules", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let catalog: serde_json::Value =
        serde_json::from_slice(&output).expect("--list-rules JSON is valid");
    let rules = catalog["rules"].as_array().expect("rules array");

    let fixable_count = rules.iter().filter(|r| r["fixable"] == true).count();
    assert_eq!(
        fixable_count, 44,
        "exactly twenty-eight rules are auto-fixable"
    );

    let redundant_quote = rules
        .iter()
        .find(|r| r["rule"] == "redundant-quote")
        .expect("redundant-quote present");
    assert_eq!(redundant_quote["fixable"], true);

    let if_arity = rules
        .iter()
        .find(|r| r["rule"] == "if-arity")
        .expect("if-arity present");
    assert_eq!(if_arity["fixable"], false);
}

#[test]
fn cli_lint_sarif_rule_metadata_declares_fixability() {
    let dir = fresh_temp_dir("lint-sarif-fixable-meta");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    let output = paredit()
        .args(["inspect", "lint", "--sarif"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let sarif: serde_json::Value = serde_json::from_slice(&output).expect("SARIF is valid JSON");
    let rules = sarif["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .expect("driver rules");
    let redundant_quote = rules
        .iter()
        .find(|r| r["id"] == "redundant-quote")
        .expect("redundant-quote rule");
    assert_eq!(redundant_quote["properties"]["fixable"], true);
}

#[test]
fn cli_lint_report_unused_suppressions_flags_a_stale_directive() {
    let dir = fresh_temp_dir("lint-unused-stale");
    let file = dir.join("a.lisp");
    // The directive names a rule that does not fire on the next line.
    fs::write(&file, ";; paredit:ignore redundant-quote\n(clean-form)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--report-unused-suppressions")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("\"unused_suppression_count\": 1"))
        .stdout(predicate::str::contains("redundant-quote"));
}

#[test]
fn cli_lint_report_unused_suppressions_is_clean_when_all_used() {
    let dir = fresh_temp_dir("lint-unused-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, ";; paredit:ignore single-arg-comparison\n(< x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--report-unused-suppressions")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unused_suppression_count\": 0"));
}

#[test]
fn cli_lint_report_unused_suppressions_catches_a_typo() {
    let dir = fresh_temp_dir("lint-unused-typo");
    let file = dir.join("a.lisp");
    // (list '5) HAS a redundant-quote finding, but the directive misspells it,
    // so it neither suppresses the finding nor counts as used.
    fs::write(&file, ";; paredit:ignore redundant-quotes\n(list '5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--report-unused-suppressions")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("redundant-quotes"));
}

#[test]
fn cli_lint_own_line_suppression_silences_the_next_line() {
    let dir = fresh_temp_dir("lint-suppress-ownline");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        ";; paredit:ignore single-arg-comparison\n(< x)\n(> y)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("single-arg-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        // (< x) is suppressed; only (> y) remains.
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}

#[test]
fn cli_lint_trailing_suppression_silences_its_own_line() {
    let dir = fresh_temp_dir("lint-suppress-trailing");
    let file = dir.join("a.lisp");
    fs::write(&file, "(< x)  ; paredit:ignore single-arg-comparison\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("single-arg-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_lint_bare_suppression_silences_all_rules() {
    let dir = fresh_temp_dir("lint-suppress-bare");
    let file = dir.join("a.lisp");
    // One line, two different findings; a bare directive silences both.
    fs::write(&file, ";; paredit:ignore\n(list '5 (< x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_lint_named_suppression_leaves_other_rules() {
    let dir = fresh_temp_dir("lint-suppress-named");
    let file = dir.join("a.lisp");
    // Suppress only redundant-quote; the single-arg-comparison must survive.
    fs::write(
        &file,
        ";; paredit:ignore redundant-quote\n(list '5 (< x))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("single-arg-comparison"));
}

#[test]
fn cli_lint_fix_skips_suppressed_lines() {
    let dir = fresh_temp_dir("lint-suppress-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        ";; paredit:ignore redundant-quote\n(list '5)\n(list '6)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes_applied\": 1"));

    // The suppressed '5 stays; only '6 is fixed.
    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(
        fixed,
        ";; paredit:ignore redundant-quote\n(list '5)\n(list 6)\n"
    );
}

#[test]
fn cli_lint_fix_applies_fixes_in_place() {
    let dir = fresh_temp_dir("lint-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defparameter *n* '5)\n(list (and y))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes_applied\": 2"))
        .stdout(predicate::str::contains("\"files_changed\": 1"));

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(defparameter *n* 5)\n(list y)\n");
}

#[test]
fn cli_lint_fix_converges_on_nested_redundancy() {
    let dir = fresh_temp_dir("lint-report-fix-nested");
    let file = dir.join("a.lisp");
    // redundant-progn unwraps to (or x), then single-operand-boolean to x.
    fs::write(&file, "(defun f () (progn (or x)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes_applied\": 2"));

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(defun f () x)\n");
}

#[test]
fn cli_lint_fix_leaves_unfixable_rules_untouched() {
    let dir = fresh_temp_dir("lint-report-fix-none");
    let file = dir.join("a.lisp");
    let original = "(if a b c d)\n";
    fs::write(&file, original).expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes_applied\": 0"))
        .stdout(predicate::str::contains("\"files_changed\": 0"));

    let after = fs::read_to_string(&file).expect("read file");
    assert_eq!(after, original);
}

#[test]
fn cli_lint_fix_respects_rule_selection() {
    let dir = fresh_temp_dir("lint-report-fix-select");
    let file = dir.join("a.lisp");
    // Only redundant-quote is active, so (and y) must be left alone.
    fs::write(&file, "(list '5 (and y))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--rule")
        .arg("redundant-quote")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes_applied\": 1"));

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(list 5 (and y))\n");
}

#[test]
fn cli_lint_fix_diff_previews_without_writing() {
    let dir = fresh_temp_dir("lint-report-fix-diff");
    let file = dir.join("a.lisp");
    let original = "(defparameter *n* '5)\n";
    fs::write(&file, original).expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--diff")
        .arg(&file)
        .assert()
        .success()
        // stdout is a pure unified diff (pipeable); the tally lands on stderr.
        .stdout(predicate::str::contains("-(defparameter *n* '5)"))
        .stdout(predicate::str::contains("+(defparameter *n* 5)"))
        .stderr(predicate::str::contains(
            "1 fix(es) across 1 file(s) — preview only, nothing written",
        ));

    // Preview must not touch the file.
    let after = fs::read_to_string(&file).expect("read file");
    assert_eq!(after, original);
}

#[test]
fn cli_lint_fix_diff_stdout_is_pure_diff_even_with_json_output() {
    let dir = fresh_temp_dir("lint-report-fix-diff-json");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list (and y))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--diff")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        // No JSON summary bleeds into the diff stream on stdout.
        .stdout(predicate::str::contains("+(list y)"))
        .stdout(predicate::str::contains("fixes_applied").not())
        .stdout(predicate::str::contains("schema_version").not());
}

#[test]
fn cli_lint_diff_requires_fix() {
    let dir = fresh_temp_dir("lint-report-diff-requires-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--diff")
        .arg(&file)
        .assert()
        .failure();
}

#[test]
fn cli_lint_fix_conflicts_with_sarif() {
    let dir = fresh_temp_dir("lint-report-fix-conflict");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--sarif")
        .arg(&file)
        .assert()
        .failure();
}

#[test]
fn cli_lint_sarif_includes_a_fix_for_single_operand_boolean() {
    let dir = fresh_temp_dir("lint-report-sarif-boolean-fix");
    let file = dir.join("a.lisp");
    // The fix must copy the operand's exact source, keeping the `#'` prefix.
    fs::write(&file, "(defun f () (or #'g))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg("--rule")
        .arg("single-operand-boolean")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes\""))
        .stdout(predicate::str::contains("Unwrap the single-operand or"))
        .stdout(predicate::str::contains("#'g"));
}

#[test]
fn cli_lint_sarif_includes_a_fix_for_nested_progn() {
    let dir = fresh_temp_dir("lint-report-sarif-nested-progn-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn a (progn b c) d)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg("--rule")
        .arg("nested-progn")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes\""))
        .stdout(predicate::str::contains(
            "Splice the nested progn into the enclosing progn",
        ));
}

#[test]
fn cli_lint_fix_splices_nested_progn() {
    let dir = fresh_temp_dir("lint-report-fix-nested-progn");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn a (progn b c) d)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--rule")
        .arg("nested-progn")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes_applied\": 1"));

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(progn a b c d)\n");
}

#[test]
fn cli_lint_sarif_fix_for_negated_when_unless_has_two_replacements() {
    let dir = fresh_temp_dir("lint-report-sarif-negated-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (not ready) (go))\n").expect("write a.lisp");

    let output = paredit()
        .arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg("--rule")
        .arg("negated-when-unless")
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let sarif: serde_json::Value =
        serde_json::from_slice(&output).expect("SARIF output is valid JSON");
    let replacements =
        &sarif["runs"][0]["results"][0]["fixes"][0]["artifactChanges"][0]["replacements"];
    assert_eq!(
        replacements.as_array().expect("replacements array").len(),
        2,
        "the flip-and-unwrap fix must be two disjoint edits"
    );
}

#[test]
fn cli_lint_fix_flips_negated_when_unless_in_place() {
    let dir = fresh_temp_dir("lint-report-fix-negated");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (not ready) (go) (run))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--rule")
        .arg("negated-when-unless")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes_applied\": 1"));

    // Head flipped, negation dropped, body left byte-identical.
    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(unless ready (go) (run))\n");
}

#[test]
fn cli_lint_fix_rewrites_eq_to_eql_for_number_and_char() {
    let dir = fresh_temp_dir("lint-fix-eq-eql");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list (eq n 5) (eq c #\\a) (eq x y))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--rule")
        .arg("eq-number-comparison")
        .arg("--rule")
        .arg("eq-char-comparison")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes_applied\": 2"));

    // eq -> eql only for the flagged literals; operands and (eq x y) untouched.
    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(list (eql n 5) (eql c #\\a) (eq x y))\n");
}

#[test]
fn cli_lint_sarif_includes_an_eql_fix_for_eq_number() {
    let dir = fresh_temp_dir("lint-sarif-eq-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq n 5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg("--rule")
        .arg("eq-number-comparison")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes\""))
        .stdout(predicate::str::contains("Compare with eql"));
}

#[test]
fn cli_lint_sarif_omits_fixes_for_unfixable_rules() {
    let dir = fresh_temp_dir("lint-report-sarif-nofix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if a b c d)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("if-arity"))
        .stdout(predicate::str::contains("\"fixes\"").not());
}

#[test]
fn cli_lint_findings_carry_category_and_fixability() {
    let dir = fresh_temp_dir("lint-report-finding-fields");
    let file = dir.join("a.lisp");
    // self-assignment (error, not fixable) and nested-cxr (warning, fixable).
    fs::write(&file, "(setq x x)\n(car (cdr y))\n").expect("write a.lisp");

    let output = paredit()
        .arg("inspect")
        .arg("lint")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let value: serde_json::Value =
        serde_json::from_slice(&output).expect("default report JSON is valid");
    let findings = value["findings"].as_array().expect("findings array");

    let self_assignment = findings
        .iter()
        .find(|f| f["rule"] == "self-assignment")
        .expect("self-assignment finding");
    assert_eq!(self_assignment["severity"], "error");
    assert_eq!(self_assignment["category"], "suspicious");
    assert_eq!(self_assignment["fixable"], false);

    let nested_cxr = findings
        .iter()
        .find(|f| f["rule"] == "nested-cxr")
        .expect("nested-cxr finding");
    assert_eq!(nested_cxr["severity"], "warning");
    assert_eq!(nested_cxr["category"], "suspicious");
    assert_eq!(nested_cxr["fixable"], true);
}

#[test]
fn cli_lint_fix_check_fails_and_leaves_file_unchanged_when_fixes_pending() {
    let dir = fresh_temp_dir("lint-report-fix-check-dirty");
    let file = dir.join("a.lisp");
    let original = "(list '5)\n";
    fs::write(&file, original).expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--check")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("auto-fixable finding(s)"));

    // --check must not write.
    assert_eq!(fs::read_to_string(&file).expect("read file"), original);
}

#[test]
fn cli_lint_fix_check_passes_for_a_clean_file() {
    let dir = fresh_temp_dir("lint-report-fix-check-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun add (a b) (+ a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--check")
        .arg(&file)
        .assert()
        .success()
        .stderr(predicate::str::contains("no pending auto-fixes"));
}

#[test]
fn cli_lint_fix_check_ignores_report_only_findings() {
    let dir = fresh_temp_dir("lint-report-fix-check-reportonly");
    let file = dir.join("a.lisp");
    // t-comparison is a finding but report-only: --check has nothing to apply.
    fs::write(&file, "(eq x t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--check")
        .arg("--rule")
        .arg("t-comparison")
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn cli_lint_fix_check_with_diff_prints_diff_and_still_fails() {
    let dir = fresh_temp_dir("lint-report-fix-check-diff");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (cdr x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--check")
        .arg("--diff")
        .arg(&file)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("(cadr x)"));
}

#[test]
fn cli_lint_check_requires_fix() {
    let dir = fresh_temp_dir("lint-report-check-requires-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    // --check without --fix is a usage error.
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--check")
        .arg(&file)
        .assert()
        .failure()
        .code(2);
}

#[test]
fn cli_lint_fix_plan_lists_replacements_without_writing() {
    let dir = fresh_temp_dir("lint-report-fix-plan");
    let file = dir.join("a.lisp");
    let original = "(eq x nil)\n";
    fs::write(&file, original).expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix-plan")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fix_count\": 1"))
        .stdout(predicate::str::contains("\"rule\": \"nil-comparison\""))
        .stdout(predicate::str::contains("\"text\": \"(null x)\""));

    // --fix-plan is a preview: the source must be untouched.
    let after = fs::read_to_string(&file).expect("read file");
    assert_eq!(after, original);
}

#[test]
fn cli_lint_fix_plan_omits_report_only_rules() {
    let dir = fresh_temp_dir("lint-report-fix-plan-reportonly");
    let file = dir.join("a.lisp");
    // t-comparison is report-only; if-arity is a non-fixable bug rule.
    fs::write(&file, "(eq y t)\n(if a b c d)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix-plan")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fix_count\": 0"));
}

#[test]
fn cli_lint_fix_plan_honors_inline_suppression() {
    let dir = fresh_temp_dir("lint-report-fix-plan-suppress");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5) ; paredit:ignore redundant-quote\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix-plan")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fix_count\": 0"));
}

#[test]
fn cli_lint_fix_plan_conflicts_with_fix() {
    let dir = fresh_temp_dir("lint-report-fix-plan-conflict");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix-plan")
        .arg("--fix")
        .arg(&file)
        .assert()
        .failure();
}
