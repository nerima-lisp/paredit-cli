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

/// One unparsable file among several no longer takes the whole run down with
/// it (Q10): the other files' findings are still reported, and the broken
/// file is named — not silently dropped — as a `partial_failures` entry.
#[test]
fn cli_lint_reports_findings_from_the_files_that_parsed_despite_one_that_did_not() {
    let dir = fresh_temp_dir("lint-report-partial-failure");
    let good = dir.join("good.lisp");
    fs::write(&good, "(setq x x)\n").expect("write good.lisp");
    let broken = dir.join("broken.lisp");
    fs::write(&broken, "(defun f (x)\n").expect("write broken.lisp");

    let mut cmd = paredit();
    let stdout = cmd
        .arg("inspect")
        .arg("lint")
        .arg("--output")
        .arg("json")
        .arg(&good)
        .arg(&broken)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&stdout).expect("valid JSON");

    // The finding from the file that parsed is still there.
    assert_eq!(report["finding_count"], 1, "{report}");
    assert!(
        report["findings"][0]["path"]
            .as_str()
            .expect("path")
            .ends_with("good.lisp"),
        "{report}"
    );

    // The file that did not parse is named, not silently absent.
    let failures = report["partial_failures"].as_array().expect("failures");
    assert_eq!(failures.len(), 1, "{report}");
    assert!(
        failures[0]["file"]
            .as_str()
            .expect("file")
            .ends_with("broken.lisp"),
        "{report}"
    );
    assert!(
        failures[0]["error"]
            .as_str()
            .expect("error")
            .contains("unclosed list"),
        "{report}"
    );
}

/// The stderr note is unconditional: a caller reading only stdout for the
/// report body would otherwise never learn a file was skipped.
#[test]
fn cli_lint_notes_a_partial_failure_on_stderr() {
    let dir = fresh_temp_dir("lint-report-partial-failure-stderr");
    fs::write(dir.join("good.lisp"), "(setq x x)\n").expect("write good.lisp");
    fs::write(dir.join("broken.lisp"), "(defun f (x)\n").expect("write broken.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg(&dir)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "1 of the requested files could not be analyzed",
        ))
        .stderr(predicate::str::contains("broken.lisp"));
}

/// When every file fails, there is nothing to report and the run fails
/// outright rather than printing a report that looks clean.
#[test]
fn cli_lint_fails_outright_when_every_file_is_unparsable() {
    let dir = fresh_temp_dir("lint-report-total-failure");
    fs::write(dir.join("a.lisp"), "(defun f (x)\n").expect("write a.lisp");
    fs::write(dir.join("b.lisp"), "(defun g (y)\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg(&dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to analyze"));
}

/// Text mode reports the same partial failure as its own tab-separated line,
/// consistent with how a `finding` line is spelled.
#[test]
fn cli_lint_text_reports_partial_failures_as_their_own_line() {
    let dir = fresh_temp_dir("lint-report-partial-failure-text");
    fs::write(dir.join("good.lisp"), "(setq x x)\n").expect("write good.lisp");
    fs::write(dir.join("broken.lisp"), "(defun f (x)\n").expect("write broken.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--output")
        .arg("text")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("failed\t"))
        .stdout(predicate::str::contains("broken.lisp"))
        .stdout(predicate::str::contains("unclosed list"));
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
        // 252 (through the 37-rule batch) + all 20 of the 20-rule batch = 272,
        // + 8 of the 9-rule batch = 280 — the ninth, `elisp-hook-lambda`, is
        // `pedantic`. + all 8 of this branch's = 288, the whole suite's 303
        // less the 15 `pedantic` rules the default `recommended` preset holds
        // back. + all 10 of this branch's = 298, the whole suite's 313 less
        // the same 15. Nine of the 10 are untagged and the tenth carries
        // `RuleTag::Style`, which no preset filters on, so the rise here is
        // again the full count. + all 3 of this branch's = 301, the whole
        // suite's 316 less the same 15: all three are untagged, so once more
        // the rise is the full count. + 3 of this branch's 4 = 304, which is
        // the suite's 320 less *16* — the divisor moved for the first time in
        // four batches, because `lfe-catch-swallows-exit` ships tagged
        // `pedantic`. So this rises by 3 where the suite rises by 4, and the
        // two numbers are not the same arithmetic. + this branch's 1 = 305,
        // and the divisor holds at 16, so it is a plain +1 again. + 3, + 4 and
        // + 6 over the loop-facility, dispatch/binding and data-structure
        // batches = 318, the divisor still 16 throughout. + all 11 of this
        // branch's = 329, the whole suite's 345 less the same 16: none of the
        // 11 carries a tag, so the rise here is once more the full count, and
        // it is the full count across three *dialects* — 5 Racket, 2 Emacs
        // Lisp, 4 Clojure — none of which the divisor distinguishes. + 2 over
        // the macro-authoring batch = 331. + both of this branch's
        // `lint-condition-depth` rules = 333, the whole suite's 349 less the
        // same 16: neither is tagged, so this is a plain +2. Note the divisor
        // has now held at 16 for eight consecutive batches — it moves only
        // when a batch ships something tagged `pedantic`.
        .stdout(predicate::str::contains("\"rule_count\": 333"))
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
    assert!(rules.len() < 173);
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

/// SARIF mode tolerates a partial failure the same way the standard report
/// does: the file that parsed is still reported, rather than the whole run
/// aborting over the one that did not.
#[test]
fn cli_lint_sarif_reports_the_file_that_parsed_despite_one_that_did_not() {
    let dir = fresh_temp_dir("lint-report-sarif-partial-failure");
    fs::write(dir.join("good.lisp"), "(setq x x)\n").expect("write good.lisp");
    fs::write(dir.join("broken.lisp"), "(defun f (x)\n").expect("write broken.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--sarif")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ruleId\": \"self-assignment\""))
        .stderr(predicate::str::contains(
            "1 of the requested files could not be analyzed",
        ));
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

/// GitHub-annotation mode tolerates a partial failure too: the annotation
/// stream still emits for the file that parsed.
#[test]
fn cli_lint_github_reports_the_file_that_parsed_despite_one_that_did_not() {
    let dir = fresh_temp_dir("lint-report-github-partial-failure");
    fs::write(dir.join("good.lisp"), "(setq x x)\n").expect("write good.lisp");
    fs::write(dir.join("broken.lisp"), "(defun f (x)\n").expect("write broken.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--github")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("self-assignment:"))
        .stderr(predicate::str::contains(
            "1 of the requested files could not be analyzed",
        ));
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

    // by_category has one entry per category, firing or not.
    assert_eq!(stats["by_category"].as_array().unwrap().len(), 17);
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
    let warnings = rules.iter().filter(|r| r["severity"] == "warning").count();
    // The default preset is `recommended`, which holds back the sixteen
    // `pedantic` rules; `--preset all` is what lists the whole suite.
    // 181 (through the 37-rule batch) + 17 of the 20-rule batch = 198, + 6 of
    // the 9-rule batch — 2 are `Severity::Error` and `elisp-hook-lambda` is
    // `pedantic`, so the default preset holds it back — = 204. + 6 of this
    // branch's 8, the other 2 being `with-open-returns-lazy-seq` and
    // `def-inside-function-body`, both `Severity::Error`, and none of the 8
    // `pedantic` — = 210. + 7 of this branch's 10, the other 3 being
    // `fennel-each-over-non-iterator`, `janet-mutating-immutable-literal` and
    // `declare-not-at-head-of-body`, all `Severity::Error`, and none of the 10
    // `pedantic` — = 217, which is the suite's 232 warnings less the 15
    // `pedantic` rules, all of them warnings. + 0 of this branch's 3: all
    // three `lint-compile-time` rules are `Severity::Error`, so this stays at
    // 217 and the suite total stays at 232.
    //
    // + 2 of this branch's 4 = 219, and this is the batch where the two
    // subtractions come apart. Three of the four are `Severity::Warning`
    // (`hy-identity-comparison-with-literal`, `hy-bare-except`,
    // `lfe-catch-swallows-exit`) and `hy-mutable-default-argument` is an
    // `Error`, so the suite's warning total rises by 3 to 235. But
    // `lfe-catch-swallows-exit` is also tagged `pedantic`, so the default
    // preset holds it back and only 2 of the 3 are visible here: 235 less the
    // now-16 `pedantic` rules, all still warnings, = 219.
    //
    // + 1, + 2 and + 2 over the loop-facility, dispatch/binding and
    // data-structure batches = 225, the divisor holding at 16 throughout.
    //
    // + 6 of this branch's 11 = 231. The 6 are both `lint-elisp-depth` rules
    // and 4 of the 5 `lint-racket-depth` ones; the other 5 are
    // `Severity::Error` — `racket-match-unreachable-clause` and all four
    // `lint-clojure-depth` rules, each of which throws or deadlocks at run
    // time. None of the 11 is tagged `pedantic`, so the two subtractions stay
    // together this time: the suite's warning total rises by the same 6, to
    // 247, and 247 less the still-16 `pedantic` rules, all of them warnings,
    // = 231.
    //
    // + 1 of this branch's 2 `lint-condition-depth` rules.
    // `unwind-protect-cleanup-signals` is a `Severity::Warning`;
    // `condition-type-datum-with-string-initarg` is a `Severity::Error`,
    // because the odd-length case means the named condition is never
    // signalled at all. Neither is `pedantic`, so again both subtractions
    // move together: the suite's warning total rises by 1 to 248, and 248
    // less the still-16 `pedantic` rules = 232.
    assert_eq!(warnings, 232);
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
    // The suite's 103, unreduced: none of the 15 `pedantic` rules the default
    // `recommended` preset holds back is fixable. This sat at 99 for four
    // batches — the `lint-scheme-idiom` four are the first fixable rules added
    // since — and it is also the only place the *preset-filtered* fixable
    // count is pinned, so a fixable rule that arrived tagged `pedantic` would
    // show up here and nowhere else. Unmoved by this branch's 10: every one of
    // them is `ReportOnly`. Unmoved by this branch's 3 for the same reason —
    // all three `lint-compile-time` rules are `ReportOnly`, so the suite total
    // and the preset-filtered total both stay at 103. Unmoved by this branch's
    // 4 as well, and this one exercises the guard the comment above describes:
    // the batch *does* add a `pedantic` rule, taking the held-back set from 15
    // to 16, and this number still does not move — because
    // `lfe-catch-swallows-exit` is `ReportOnly` like the other three. The
    // sentence "none of the pedantic rules is fixable" is now load-bearing over
    // 16 rules rather than 15.
    //
    // This branch finally moves it: `carp-deprecated-thread-macro` is
    // `Fixability::Fixable` and untagged, so it lands in both the suite total
    // and the preset-filtered one. It is the rare mechanical repair -- `=>`
    // and `->` are byte-identical macro bodies in Carp's own stdlib, so the
    // fix is a rename.
    //
    // And this branch moves it again, by 2, to 106:
    // `racket-begin0-single-form` and `racket-case-lambda-single-clause` are
    // both `Fixability::Fixable` and both untagged, so they land in the suite
    // total and in this preset-filtered one alike. Both repairs copy an inner
    // span verbatim — the sole `begin0` body form with its reader prefix, the
    // sole `case-lambda` clause's formals and body into a `lambda` — so
    // neither invents a decision. The other 9 rules of the branch are
    // `ReportOnly`.
    //
    // These two are also why `fixable_rules_match_the_fix_engine` grew a
    // *Racket* fixture: they declare `[Dialect::Racket]` alone, which the
    // Scheme fixture added for the same failure in PR #88 does not reach.
    assert_eq!(
        fixable_count, 106,
        "the fixable rules the default preset admits"
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

// ---------------------------------------------------------------------------
// The rule *mechanism*: presets, tags, severity overrides, per-rule settings,
// long-form explanations, cost accounting, stable finding ids, and the
// suppression scopes. These are the parts of `inspect lint` that are about the
// rule set rather than about any one rule, so they are grouped rather than
// scattered among the per-rule tests above.
// ---------------------------------------------------------------------------

/// Parses a command's stdout as JSON, failing the test with the raw bytes if
/// it is not.
fn json_stdout(assert: assert_cmd::assert::Assert) -> serde_json::Value {
    let output = assert.success().get_output().stdout.clone();
    serde_json::from_slice(&output).unwrap_or_else(|error| {
        panic!(
            "expected JSON, got {error}: {}",
            String::from_utf8_lossy(&output)
        )
    })
}

#[test]
fn cli_lint_explain_prints_the_long_form_documentation() {
    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--explain",
                "redundant-the",
                "--output",
                "json",
            ])
            .assert(),
    );
    assert_eq!(value["rule"], "redundant-the");
    assert_eq!(value["category"], "suspicious");
    assert_eq!(value["fixable"], true);
    // Every rule can be explained, whether or not it declares a rationale: the
    // dialect list alone answers the commonest "why did this find nothing?".
    assert_eq!(value["dialects"][0], "common-lisp");
}

#[test]
fn cli_lint_explain_includes_the_example_and_caveat_when_declared() {
    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--explain",
                "unnecessary-copy",
                "--output",
                "json",
            ])
            .assert(),
    );
    assert!(
        value["rationale"]
            .as_str()
            .expect("a rationale")
            .contains("copy")
    );
    assert_eq!(value["example"]["bad"], "(length (copy-list xs))");
    assert_eq!(value["example"]["good"], "(length xs)");
    assert!(value["caveat"].as_str().expect("a caveat").contains("sort"));
}

#[test]
fn cli_lint_explain_lists_a_rules_tunable_settings() {
    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--explain",
                "linear-search-in-loop",
                "--output",
                "json",
            ])
            .assert(),
    );
    let settings = value["settings"].as_array().expect("settings array");
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0]["key"], "min-searches");
    assert_eq!(settings[0]["default"], 1);
}

#[test]
fn cli_lint_explain_rejects_an_unknown_rule() {
    paredit()
        .args(["inspect", "lint", "--explain", "no-such-rule"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown lint rule"));
}

#[test]
fn cli_lint_presets_widen_monotonically() {
    let count = |preset: &str| {
        let value = json_stdout(
            paredit()
                .args([
                    "inspect",
                    "lint",
                    "--list-rules",
                    "--preset",
                    preset,
                    "--output",
                    "json",
                ])
                .assert(),
        );
        value["rule_count"].as_u64().expect("a rule count")
    };
    let (minimal, recommended, pedantic, all) = (
        count("minimal"),
        count("recommended"),
        count("pedantic"),
        count("all"),
    );
    assert!(minimal < recommended, "{minimal} < {recommended}");
    assert!(recommended < pedantic, "{recommended} < {pedantic}");
    assert!(pedantic <= all, "{pedantic} <= {all}");
}

#[test]
fn cli_lint_list_presets_reports_each_rungs_size() {
    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--list-presets", "--output", "json"])
            .assert(),
    );
    assert_eq!(value["default"], "recommended");
    let presets = value["presets"].as_array().expect("presets array");
    assert_eq!(presets.len(), 4);
    assert_eq!(presets[0]["preset"], "minimal");
    assert!(presets[0]["rule_count"].as_u64().expect("count") > 0);
}

#[test]
fn cli_lint_default_preset_holds_back_the_pedantic_rules() {
    let dir = fresh_temp_dir("lint-preset-pedantic");
    let file = dir.join("a.lisp");
    // Undocumented and unmarked: two pedantic findings, and nothing else.
    fs::write(&file, "(defparameter timeout 30)\n").expect("write a.lisp");

    let recommended = json_stdout(
        paredit()
            .args(["inspect", "lint", "--output", "json"])
            .arg(&file)
            .assert(),
    );
    assert_eq!(recommended["finding_count"], 0);

    let pedantic = json_stdout(
        paredit()
            .args([
                "inspect", "lint", "--preset", "pedantic", "--output", "json",
            ])
            .arg(&file)
            .assert(),
    );
    assert!(pedantic["finding_count"].as_u64().expect("count") >= 2);
}

#[test]
fn cli_lint_tag_filter_narrows_to_rules_carrying_every_named_tag() {
    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--list-rules",
                "--tag",
                "pedantic",
                "--preset",
                "pedantic",
                "--output",
                "json",
            ])
            .assert(),
    );
    let rules = value["rules"].as_array().expect("rules array");
    assert!(!rules.is_empty());
    assert!(rules.iter().all(|rule| {
        rule["tags"]
            .as_array()
            .expect("tags")
            .contains(&serde_json::Value::String("pedantic".to_owned()))
    }));
}

#[test]
fn cli_lint_rejects_an_unknown_tag() {
    paredit()
        .args(["inspect", "lint", "--list-rules", "--tag", "experimentl"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown lint tag"));
}

#[test]
fn cli_lint_deny_promotes_a_warning_to_error() {
    let dir = fresh_temp_dir("lint-deny");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    let plain = json_stdout(
        paredit()
            .args(["inspect", "lint", "--output", "json"])
            .arg(&file)
            .assert(),
    );
    assert_eq!(plain["findings"][0]["severity"], "warning");

    let denied = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--deny",
                "redundant-quote",
                "--output",
                "json",
            ])
            .arg(&file)
            .assert(),
    );
    assert_eq!(denied["findings"][0]["severity"], "error");
}

#[test]
fn cli_lint_deny_by_category_reaches_every_rule_in_it() {
    let dir = fresh_temp_dir("lint-deny-category");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--deny",
                "suspicious",
                "--output",
                "json",
            ])
            .arg(&file)
            .assert(),
    );
    assert_eq!(value["findings"][0]["severity"], "error");
}

#[test]
fn cli_lint_deny_changes_what_the_severity_gate_fails_on() {
    let dir = fresh_temp_dir("lint-deny-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    // `redundant-quote` ships as a warning, so an error-level gate passes...
    paredit()
        .args(["inspect", "lint", "--fail-on", "error", "--output", "json"])
        .arg(&file)
        .assert()
        .success();

    // ...but not once the run has been told to treat it as an error.
    paredit()
        .args([
            "inspect",
            "lint",
            "--deny",
            "redundant-quote",
            "--fail-on",
            "error",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .failure();
}

#[test]
fn cli_lint_warn_demotes_an_error_below_the_gate() {
    let dir = fresh_temp_dir("lint-warn-gate");
    let file = dir.join("a.lisp");
    // `literal-place` ships as an error.
    fs::write(&file, "(incf 5)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "lint", "--fail-on", "error", "--output", "json"])
        .arg(&file)
        .assert()
        .failure();

    paredit()
        .args([
            "inspect",
            "lint",
            "--warn",
            "literal-place",
            "--fail-on",
            "error",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn cli_lint_rejects_an_unknown_deny_selector() {
    paredit()
        .args(["inspect", "lint", "--list-rules", "--deny", "no-such-thing"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown lint rule"));
}

#[test]
fn cli_lint_rule_arg_retunes_a_threshold() {
    let dir = fresh_temp_dir("lint-rule-arg");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(defun scan (items known) (dolist (x items) (member x known)))\n",
    )
    .expect("write a.lisp");

    let default = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--rule",
                "linear-search-in-loop",
                "--output",
                "json",
            ])
            .arg(&file)
            .assert(),
    );
    assert_eq!(default["finding_count"], 1);

    let raised = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--rule",
                "linear-search-in-loop",
                "--rule-arg",
                "linear-search-in-loop.min-searches=2",
                "--output",
                "json",
            ])
            .arg(&file)
            .assert(),
    );
    assert_eq!(raised["finding_count"], 0);
}

#[test]
fn cli_lint_rejects_a_rule_arg_the_rule_does_not_declare() {
    paredit()
        .args([
            "inspect",
            "lint",
            "--list-rules",
            "--rule-arg",
            "linear-search-in-loop.nope=2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("has no setting"));
}

#[test]
fn cli_lint_rejects_a_malformed_rule_arg() {
    paredit()
        .args(["inspect", "lint", "--list-rules", "--rule-arg", "nonsense"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("malformed --rule-arg"));

    paredit()
        .args([
            "inspect",
            "lint",
            "--list-rules",
            "--rule-arg",
            "linear-search-in-loop.min-searches=lots",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("integer value"));
}

#[test]
fn cli_lint_timings_attributes_cost_to_rules() {
    let dir = fresh_temp_dir("lint-timings");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (list '5) (+ x 1))\n").expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--timings", "--output", "json"])
            .arg(&file)
            .assert(),
    );
    assert_eq!(value["files_scanned"], 1);
    let rules = value["rules"].as_array().expect("rules array");
    assert!(!rules.is_empty(), "every rule that ran should be listed");
    // Slowest first, and every listed rule actually ran.
    let shares: Vec<f64> = rules
        .iter()
        .map(|rule| rule["micros"].as_f64().expect("micros"))
        .collect();
    assert!(shares.windows(2).all(|pair| pair[0] >= pair[1]));
    assert!(
        rules
            .iter()
            .all(|rule| rule["invocations"].as_u64().expect("invocations") > 0)
    );
}

#[test]
fn cli_lint_findings_carry_a_content_derived_id() {
    let dir = fresh_temp_dir("lint-finding-id");
    let tight = dir.join("tight.lisp");
    let loose = dir.join("loose.lisp");
    fs::write(&tight, "(the t (compute x))\n").expect("write tight.lisp");
    // The same form, reindented across three lines.
    fs::write(&loose, "(the\n   t\n   (compute\n     x))\n").expect("write loose.lisp");

    let id_of = |path: &std::path::Path| {
        let value = json_stdout(
            paredit()
                .args(["inspect", "lint", "--output", "json"])
                .arg(path)
                .assert(),
        );
        value["findings"][0]["id"]
            .as_str()
            .expect("a finding id")
            .to_owned()
    };

    let id = id_of(&tight);
    assert!(id.starts_with("redundant-the/"));
    // Reformatting must not change a finding's identity, or every baseline
    // would go stale the first time the file is formatted.
    assert_eq!(id, id_of(&loose));
}

#[test]
fn cli_lint_two_identical_findings_get_distinct_ids() {
    let dir = fresh_temp_dir("lint-finding-id-dup");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n(list '5)\n").expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--output", "json"])
            .arg(&file)
            .assert(),
    );
    let first = value["findings"][0]["id"].as_str().expect("first id");
    let second = value["findings"][1]["id"].as_str().expect("second id");
    assert_ne!(first, second);
}

#[test]
fn cli_lint_ignore_next_form_covers_the_whole_form() {
    let dir = fresh_temp_dir("lint-ignore-form");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        ";; paredit:ignore-next-form redundant-quote\n\
         (defun f ()\n  (list '5)\n  (list '6))\n\
         (defun g () (list '7))\n",
    )
    .expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--output", "json"])
            .arg(&file)
            .assert(),
    );
    // Two findings inside the guarded defun are silenced; the third is not.
    assert_eq!(value["finding_count"], 1);
}

#[test]
fn cli_lint_ignore_file_covers_every_line() {
    let dir = fresh_temp_dir("lint-ignore-file");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        ";; paredit:ignore-file redundant-quote\n(list '5)\n(list '6)\n",
    )
    .expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--output", "json"])
            .arg(&file)
            .assert(),
    );
    assert_eq!(value["finding_count"], 0);
}

#[test]
fn cli_lint_require_suppression_reason_flags_an_unexplained_directive() {
    let dir = fresh_temp_dir("lint-suppression-reason");
    let file = dir.join("a.lisp");
    fs::write(&file, ";; paredit:ignore redundant-quote\n(list '5)\n").expect("write a.lisp");

    // Without the flag the directive is doing its job and is not reported.
    paredit()
        .args([
            "inspect",
            "lint",
            "--report-unused-suppressions",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success();

    let assert = paredit()
        .args([
            "inspect",
            "lint",
            "--report-unused-suppressions",
            "--require-suppression-reason",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .failure();
    let value: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("JSON");
    assert_eq!(value["unused_suppression_count"], 1);
    assert_eq!(value["unused_suppressions"][0]["problem"], "missing-reason");
}

#[test]
fn cli_lint_a_directive_with_a_reason_satisfies_the_requirement() {
    let dir = fresh_temp_dir("lint-suppression-reason-ok");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        ";; paredit:ignore redundant-quote -- the macro reads better this way\n(list '5)\n",
    )
    .expect("write a.lisp");

    paredit()
        .args([
            "inspect",
            "lint",
            "--report-unused-suppressions",
            "--require-suppression-reason",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn cli_lint_remove_unused_suppressions_deletes_only_the_stale_ones() {
    let dir = fresh_temp_dir("lint-remove-suppressions");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        ";; paredit:ignore no-such-rule\n(defun g () t)\n\
         ;; paredit:ignore redundant-quote\n(list '5)\n",
    )
    .expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--remove-unused-suppressions",
                "--output",
                "json",
            ])
            .arg(&file)
            .assert(),
    );
    assert_eq!(value["suppressions_removed"], 1);

    let rewritten = fs::read_to_string(&file).expect("read a.lisp");
    assert!(!rewritten.contains("no-such-rule"));
    // The live directive is untouched.
    assert!(rewritten.contains("paredit:ignore redundant-quote"));
    assert!(rewritten.contains("(defun g () t)"));
}

#[test]
fn cli_lint_remove_unused_suppressions_narrows_a_partly_stale_directive() {
    let dir = fresh_temp_dir("lint-narrow-suppressions");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        ";; paredit:ignore redundant-quote no-such-rule\n(list '5)\n",
    )
    .expect("write a.lisp");

    paredit()
        .args([
            "inspect",
            "lint",
            "--remove-unused-suppressions",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success();

    let rewritten = fs::read_to_string(&file).expect("read a.lisp");
    assert!(rewritten.contains("paredit:ignore redundant-quote"));
    assert!(!rewritten.contains("no-such-rule"));
}

#[test]
fn cli_lint_report_expired_suppressions_flags_a_past_date() {
    let dir = fresh_temp_dir("lint-expired-past");
    let file = dir.join("a.lisp");
    // Still silences the finding — expiry is independent of use.
    fs::write(
        &file,
        ";; paredit:ignore-until 2000-01-01 redundant-quote\n(list '5)\n",
    )
    .expect("write a.lisp");

    let assert = paredit()
        .args([
            "inspect",
            "lint",
            "--report-expired-suppressions",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .code(3);
    let value: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("JSON");
    assert_eq!(value["expired_suppression_count"], 1);
    assert_eq!(
        value["expired_suppressions"][0]["rules"][0],
        "redundant-quote"
    );

    // And the plain report still shows nothing: the expired directive kept
    // silencing the finding.
    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--output", "json"])
            .arg(&file)
            .assert(),
    );
    assert_eq!(value["finding_count"], 0);
}

#[test]
fn cli_lint_report_expired_suppressions_is_clean_for_a_future_date() {
    let dir = fresh_temp_dir("lint-expired-future");
    let file = dir.join("a.lisp");
    fs::write(&file, ";; paredit:ignore-until 2099-01-01\n(list '5)\n").expect("write a.lisp");

    paredit()
        .args([
            "inspect",
            "lint",
            "--report-expired-suppressions",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"expired_suppression_count\": 0"));
}

#[test]
fn cli_lint_report_expired_suppressions_ignores_a_directive_with_no_until() {
    let dir = fresh_temp_dir("lint-expired-none");
    let file = dir.join("a.lisp");
    fs::write(&file, ";; paredit:ignore redundant-quote\n(list '5)\n").expect("write a.lisp");

    paredit()
        .args([
            "inspect",
            "lint",
            "--report-expired-suppressions",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"expired_suppression_count\": 0"));
}

#[test]
fn cli_lint_report_suppressions_lists_every_directive_used_or_not() {
    let dir = fresh_temp_dir("lint-suppression-inventory");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        ";; paredit:ignore redundant-quote -- kept for readability\n(list '5)\n\
         ;; paredit:ignore no-such-rule\n(clean-form)\n",
    )
    .expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--report-suppressions",
                "--output",
                "json",
            ])
            .arg(&file)
            .assert(),
    );
    assert_eq!(value["suppression_count"], 2);
    assert_eq!(value["unused_count"], 1);
    let entries = value["suppressions"].as_array().expect("array");
    let used = entries
        .iter()
        .find(|entry| entry["comment_line"] == 1)
        .expect("first directive");
    assert_eq!(used["used"], true);
    assert_eq!(used["reason"], "kept for readability");
    let unused = entries
        .iter()
        .find(|entry| entry["comment_line"] == 3)
        .expect("second directive");
    assert_eq!(unused["used"], false);
}

#[test]
fn cli_lint_report_suppressions_never_gates_the_run() {
    let dir = fresh_temp_dir("lint-suppression-inventory-exit");
    let file = dir.join("a.lisp");
    // A stale directive would fail --report-unused-suppressions; the
    // inventory is a survey and must exit 0 regardless.
    fs::write(&file, ";; paredit:ignore no-such-rule\n(clean-form)\n").expect("write a.lisp");

    paredit()
        .args([
            "inspect",
            "lint",
            "--report-suppressions",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn cli_lint_suppress_path_silences_a_whole_file() {
    let dir = fresh_temp_dir("lint-suppress-path");
    let vendor = dir.join("vendor");
    fs::create_dir(&vendor).expect("mkdir vendor");
    let vendored = vendor.join("generated.lisp");
    let own = dir.join("a.lisp");
    // Neither file carries an inline suppression; both would normally report.
    fs::write(&vendored, "(list '5)\n").expect("write vendor/generated.lisp");
    fs::write(&own, "(list '6)\n").expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--output", "json"])
            .arg("--suppress-path")
            .arg(&vendor)
            .arg(&vendored)
            .arg(&own)
            .assert(),
    );
    // Only a.lisp's finding survives; the vendored file's is gone entirely,
    // not merely marked suppressed.
    assert_eq!(value["finding_count"], 1);
    let findings = value["findings"].as_array().expect("array");
    assert!(findings.iter().all(|finding| {
        !finding["path"]
            .as_str()
            .unwrap_or_default()
            .contains("vendor")
    }));
}

#[test]
fn cli_lint_suppress_path_leaves_other_commands_untouched() {
    let dir = fresh_temp_dir("lint-suppress-path-scope");
    let vendor = dir.join("vendor");
    fs::create_dir(&vendor).expect("mkdir vendor");
    let vendored = vendor.join("generated.lisp");
    fs::write(&vendored, "(defun g () 1)\n").expect("write vendor/generated.lisp");

    // `--suppress-path` is a lint-only flag; `inspect definitions` still sees
    // the file, confirming this does not reach for the workspace-wide
    // `paths.exclude` mechanism.
    let value = json_stdout(
        paredit()
            .args(["inspect", "definitions", "--output", "json"])
            .arg(&vendored)
            .assert(),
    );
    assert_eq!(value["definition_count"], 1);
}

#[test]
fn cli_lint_docs_generates_a_markdown_reference() {
    let output = paredit()
        .args(["inspect", "lint", "--docs"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let markdown = String::from_utf8(output).expect("UTF-8 markdown");

    assert!(markdown.starts_with("# Lint rules"));
    // Every category is a heading and every rule a subheading.
    assert!(markdown.contains("## performance"));
    assert!(markdown.contains("### `unnecessary-copy`"));
    // The worked example a rule declares is rendered as a code block.
    assert!(markdown.contains("(length (copy-list xs))"));
    // A rule's tunable knob is documented with the flag that sets it.
    assert!(markdown.contains("--rule-arg linear-search-in-loop.min-searches="));
}

#[test]
fn cli_lint_github_annotations_follow_the_overridden_severity() {
    let dir = fresh_temp_dir("lint-github-severity");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "lint", "--github"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("::warning file="));

    paredit()
        .args(["inspect", "lint", "--github", "--deny", "redundant-quote"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("::error file="));
}

#[test]
fn cli_lint_sarif_publishes_the_finding_id_as_a_fingerprint() {
    let dir = fresh_temp_dir("lint-sarif-id");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the t x)\n").expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--sarif"])
            .arg(&file)
            .assert(),
    );
    let fingerprints = &value["runs"][0]["results"][0]["partialFingerprints"];
    assert!(
        fingerprints["pareditFindingId"]
            .as_str()
            .expect("a finding id")
            .starts_with("redundant-the/")
    );
}

#[test]
fn cli_lint_no_destructive_fixes_holds_back_the_tagged_rewrites() {
    let dir = fresh_temp_dir("lint-no-destructive");
    let file = dir.join("a.lisp");
    // `copy-before-destructive` is the one fix tagged `destructive`;
    // `redundant-the` next to it is not.
    fs::write(&file, "(list (nreverse (copy-list xs)) (the t y))\n").expect("write a.lisp");

    // Both fixes are pending, so the CI gate fails and names them.
    paredit()
        .args(["inspect", "lint", "--fix", "--check"])
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("2 auto-fixable finding(s)"));

    // With the tagged fix held back, only the safe one is offered.
    let held = paredit()
        .args([
            "inspect",
            "lint",
            "--fix",
            "--diff",
            "--no-destructive-fixes",
        ])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let diff = String::from_utf8(held).expect("UTF-8 diff");
    assert!(diff.contains("(the t y)"), "the safe fix still applies");
    assert!(
        !diff.contains("reverse xs"),
        "the destructive fix is held back"
    );
}

// ---------------------------------------------------------------------------
// Custom rules: a project's own pattern rules, their declarative tests, and
// the deprecation shorthand. These run as a second pass and are then
// indistinguishable from shipped findings in every output mode, which is what
// most of the assertions below are checking.
// ---------------------------------------------------------------------------

/// Writes a rule directory and returns its path.
fn rule_dir(name: &str, rules: &str) -> PathBuf {
    let dir = fresh_temp_dir(name).join("rules");
    std::fs::create_dir_all(&dir).expect("create rule dir");
    fs::write(dir.join("house.lisp"), rules).expect("write house.lisp");
    dir
}

#[test]
fn cli_lint_custom_rule_reports_like_a_shipped_one() {
    let rules = rule_dir(
        "lint-custom-report",
        r#"(defrule no-bare-print
             :category suspicious
             :severity error
             :description "print writes to *standard-output* directly"
             :pattern (print ?x)
             :message "use (format t ...) rather than print")"#,
    );
    let dir = fresh_temp_dir("lint-custom-report-src");
    let file = dir.join("a.lisp");
    // The shipped `leftover-print-debug` rule also matches a bare `print`
    // call now; suppressed here so this stays a test of one custom finding's
    // shape, not of two rules' interaction.
    fs::write(
        &file,
        ";; paredit:ignore leftover-print-debug\n(defun run () (print (compute 1)))\n",
    )
    .expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--custom-rules"])
            .arg(&rules)
            .args(["--output", "json"])
            .arg(&file)
            .assert(),
    );
    assert_eq!(value["finding_count"], 1);
    let finding = &value["findings"][0];
    assert_eq!(finding["rule"], "no-bare-print");
    // The custom rule's own metadata, not the "unknown rule" fallback.
    assert_eq!(finding["severity"], "error");
    assert_eq!(finding["category"], "suspicious");
    assert_eq!(finding["fixable"], false);
    // And a content-derived id, exactly like a shipped finding.
    assert!(
        finding["id"]
            .as_str()
            .expect("an id")
            .starts_with("no-bare-print/")
    );
}

#[test]
fn cli_lint_custom_rule_fix_applies_like_a_shipped_one() {
    let rules = rule_dir(
        "lint-custom-fix",
        r#"(defrule no-bare-print
             :pattern (print ?x)
             :message "m"
             :fix (format t "~a~%" ?x))"#,
    );
    let dir = fresh_temp_dir("lint-custom-fix-src");
    let file = dir.join("a.lisp");
    // The shipped `leftover-print-debug` rule also matches and would remove
    // this call outright, racing the custom rule's own rewrite fix for the
    // same span; suppressed here so this stays a test of one custom rule's
    // fix, not of two competing fixes.
    fs::write(
        &file,
        ";; paredit:ignore leftover-print-debug\n(print (compute 1))\n",
    )
    .expect("write a.lisp");

    paredit()
        .args(["inspect", "lint", "--custom-rules"])
        .arg(&rules)
        .args(["--fix", "--output", "json"])
        .arg(&file)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&file).expect("read a.lisp"),
        ";; paredit:ignore leftover-print-debug\n(format t \"~a~%\" (compute 1))\n"
    );
}

#[test]
fn cli_lint_a_custom_finding_obeys_a_suppression_comment() {
    let rules = rule_dir(
        "lint-custom-suppress",
        r#"(defrule no-bare-print :pattern (print ?x) :message "m")"#,
    );
    let dir = fresh_temp_dir("lint-custom-suppress-src");
    let file = dir.join("a.lisp");
    // Also names the shipped `leftover-print-debug` rule, which matches the
    // same `print` call now — a suppression comment names every rule it
    // silences, and this one silences two.
    fs::write(
        &file,
        ";; paredit:ignore no-bare-print leftover-print-debug\n(print 1)\n",
    )
    .expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--custom-rules"])
            .arg(&rules)
            .args(["--output", "json"])
            .arg(&file)
            .assert(),
    );
    assert_eq!(value["finding_count"], 0);
}

#[test]
fn cli_lint_a_custom_finding_trips_the_severity_gate() {
    let rules = rule_dir(
        "lint-custom-gate",
        r#"(defrule no-bare-print :severity error :pattern (print ?x) :message "m")"#,
    );
    let dir = fresh_temp_dir("lint-custom-gate-src");
    let file = dir.join("a.lisp");
    fs::write(&file, "(print 1)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "lint", "--custom-rules"])
        .arg(&rules)
        .args(["--fail-on", "error", "--output", "json"])
        .arg(&file)
        .assert()
        .failure();
}

// --- FR-E16: `--deny`/`--warn`/`--rule`/`--exclude` — what `lint.deny` /
// `lint.warn` / `lint.enable` / `lint.disable` become on the command line —
// also recognise a loaded custom rule by name, not just the shipped
// catalogue. ---

#[test]
fn cli_lint_deny_promotes_a_loaded_custom_rule_to_error() {
    let rules = rule_dir(
        "lint-custom-deny-promote",
        r#"(defrule my-custom-rule :pattern (print ?x) :message "m")"#,
    );
    let dir = fresh_temp_dir("lint-custom-deny-promote-src");
    let file = dir.join("a.lisp");
    // The shipped `leftover-print-debug` rule also matches; suppressed so
    // `findings[0]` stays the custom rule's own finding.
    fs::write(&file, ";; paredit:ignore leftover-print-debug\n(print 1)\n").expect("write a.lisp");

    // The custom rule ships at warning severity by default.
    let plain = json_stdout(
        paredit()
            .args(["inspect", "lint", "--custom-rules"])
            .arg(&rules)
            .args(["--output", "json"])
            .arg(&file)
            .assert(),
    );
    assert_eq!(plain["findings"][0]["severity"], "warning");

    // `--deny my-custom-rule` promotes it, and that is what turns
    // `--fail-on error` into a failing exit code.
    let denied = json_stdout(
        paredit()
            .args(["inspect", "lint", "--custom-rules"])
            .arg(&rules)
            .args(["--deny", "my-custom-rule", "--output", "json"])
            .arg(&file)
            .assert(),
    );
    assert_eq!(denied["findings"][0]["severity"], "error");

    paredit()
        .args(["inspect", "lint", "--custom-rules"])
        .arg(&rules)
        .args(["--deny", "my-custom-rule", "--fail-on", "error"])
        .arg(&file)
        .assert()
        .failure();
}

#[test]
fn cli_lint_exclude_stops_a_loaded_custom_rule_from_being_reported() {
    let rules = rule_dir(
        "lint-custom-exclude",
        r#"(defrule my-custom-rule :pattern (print ?x) :message "m")"#,
    );
    let dir = fresh_temp_dir("lint-custom-exclude-src");
    let file = dir.join("a.lisp");
    // The shipped `leftover-print-debug` rule also matches `--exclude` does
    // not touch it; suppressed so the excluded custom rule's absence is what
    // this test is actually observing.
    fs::write(&file, ";; paredit:ignore leftover-print-debug\n(print 1)\n").expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--custom-rules"])
            .arg(&rules)
            .args(["--exclude", "my-custom-rule", "--output", "json"])
            .arg(&file)
            .assert(),
    );
    assert_eq!(value["finding_count"], 0);
}

/// A name that is neither shipped nor loaded is still rejected — loading a
/// custom ruleset widens what `--deny` accepts, it does not turn the check
/// off.
#[test]
fn cli_lint_deny_still_rejects_an_unrelated_unknown_name_with_custom_rules_loaded() {
    let rules = rule_dir(
        "lint-custom-deny-unknown",
        r#"(defrule my-custom-rule :pattern (print ?x) :message "m")"#,
    );
    paredit()
        .args(["inspect", "lint", "--custom-rules"])
        .arg(&rules)
        .args(["--list-rules", "--deny", "not-a-real-rule"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown lint rule"));
}

#[test]
fn cli_lint_deprecate_reports_any_call_to_the_named_operator() {
    let rules = rule_dir(
        "lint-custom-deprecate",
        r#"(deprecate legacy-connect :use connect :reason "removed in 3.0")"#,
    );
    let dir = fresh_temp_dir("lint-custom-deprecate-src");
    let file = dir.join("a.lisp");
    fs::write(&file, "(legacy-connect)\n(legacy-connect \"db\" 5)\n").expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--custom-rules"])
            .arg(&rules)
            .args(["--output", "json"])
            .arg(&file)
            .assert(),
    );
    assert_eq!(value["finding_count"], 2);
    assert_eq!(value["findings"][0]["rule"], "deprecated-legacy-connect");
    assert!(
        value["findings"][0]["message"]
            .as_str()
            .expect("a message")
            .contains("use connect instead (removed in 3.0)")
    );
}

#[test]
fn cli_lint_test_rules_passes_a_correct_rule_set() {
    let rules = rule_dir(
        "lint-custom-harness-ok",
        r#"(defrule no-bare-print
             :pattern (print ?x)
             :message "m"
             :fix (format t "~a" ?x))
           (deftest no-bare-print
             (:matches "(print 1)")
             (:no-match "(princ 1)")
             (:fix "(print 1)" "(format t \"~a\" 1)"))"#,
    );
    let value = json_stdout_or_text(
        paredit()
            .args(["inspect", "lint", "--test-rules", "--custom-rules"])
            .arg(&rules)
            .args(["--output", "text"])
            .assert()
            .success(),
    );
    assert!(value.contains("failure_count\t0"), "{value}");
}

/// `--test-rules` honors `--output json` the same way a scan does: one JSON
/// object per [`paredit_feature_lint_custom::TestFailure`] rather than the
/// text mode's flattened tab-separated line.
#[test]
fn cli_lint_test_rules_reports_structured_failures_as_json() {
    let rules = rule_dir(
        "lint-custom-harness-json",
        r#"(defrule no-bare-print :pattern (?op ?x) :message "m")
           (deftest no-bare-print (:no-match "(princ 1)"))"#,
    );
    let stdout = paredit()
        .args(["inspect", "lint", "--test-rules", "--custom-rules"])
        .arg(&rules)
        .args(["--output", "json"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&stdout).expect("valid JSON");
    assert_eq!(value["failure_count"], 1, "{value}");
    let failure = &value["failures"][0];
    assert_eq!(failure["rule"], "no-bare-print");
    assert_eq!(failure["clause"], ":no-match");
    assert_eq!(failure["input"], "(princ 1)");
    assert_eq!(failure["expected"], "no finding");
    assert_eq!(failure["actual"], "1 finding(s)");
}

/// `--test-rules` still defaults to JSON like every other mode, so a
/// passing rule set with no `--output` flag reports `failure_count: 0` as a
/// JSON number, not the text mode's tab-separated line.
#[test]
fn cli_lint_test_rules_defaults_to_json_output() {
    let rules = rule_dir(
        "lint-custom-harness-json-default",
        r#"(defrule no-bare-print
             :pattern (print ?x)
             :message "m")
           (deftest no-bare-print (:matches "(print 1)"))"#,
    );
    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--test-rules", "--custom-rules"])
            .arg(&rules)
            .assert()
            .success(),
    );
    assert_eq!(value["failure_count"], 0, "{value}");
    assert_eq!(value["custom_rule_count"], 1, "{value}");
}

#[test]
fn cli_lint_test_rules_fails_a_pattern_that_grew_too_broad() {
    let rules = rule_dir(
        "lint-custom-harness-broad",
        r#"(defrule no-bare-print :pattern (?op ?x) :message "m")
           (deftest no-bare-print (:no-match "(princ 1)"))"#,
    );
    paredit()
        .args(["inspect", "lint", "--test-rules", "--custom-rules"])
        .arg(&rules)
        .assert()
        .failure()
        .stdout(predicate::str::contains(":no-match"));
}

#[test]
fn cli_lint_rejects_a_custom_rule_that_shadows_a_shipped_one() {
    let rules = rule_dir(
        "lint-custom-collision",
        r#"(defrule redundant-quote :pattern (print ?x) :message "m")"#,
    );
    paredit()
        .args(["inspect", "lint", "--list-rules", "--custom-rules"])
        .arg(&rules)
        .assert()
        .failure()
        .stderr(predicate::str::contains("collides with a shipped rule"));
}

#[test]
fn cli_lint_rejects_a_custom_rule_whose_fix_names_an_unbound_variable() {
    let rules = rule_dir(
        "lint-custom-unbound",
        r#"(defrule r :pattern (f ?a) :message "m" :fix (g ?b))"#,
    );
    paredit()
        .args(["inspect", "lint", "--list-rules", "--custom-rules"])
        .arg(&rules)
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not bind"));
}

/// FR-E12: a `:dialects` clause scopes a `defrule` to the dialects it names,
/// as a guard rather than a hint — a file outside them is skipped entirely,
/// not matched and reported anyway.
#[test]
fn cli_lint_dialect_scoped_rule_only_fires_in_its_own_dialects() {
    let rules = rule_dir(
        "lint-custom-dialects",
        r#"(defrule cl-only-incf
             :dialects (common-lisp)
             :pattern (incf ?x)
             :message "m")"#,
    );
    let dir = fresh_temp_dir("lint-dialects-run");

    // `--rule` scopes the report to just this custom rule (FR-E16 widened
    // rule selectors to recognise loaded custom rule names), so the
    // assertion is not at the mercy of an unrelated shipped rule also firing
    // on the fixture.
    let cl_file = dir.join("a.lisp");
    fs::write(&cl_file, "(incf x)\n").expect("write a.lisp");
    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--output",
                "json",
                "--rule",
                "cl-only-incf",
                "--custom-rules",
            ])
            .arg(&rules)
            .arg(&cl_file)
            .assert()
            .success(),
    );
    assert_eq!(value["finding_count"], 1, "{value}");

    let el_file = dir.join("a.el");
    fs::write(&el_file, "(incf x)\n").expect("write a.el");
    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--output",
                "json",
                "--rule",
                "cl-only-incf",
                "--custom-rules",
            ])
            .arg(&rules)
            .arg(&el_file)
            .assert()
            .success(),
    );
    assert_eq!(value["finding_count"], 0, "{value}");
}

#[test]
fn cli_lint_a_rule_with_no_dialects_clause_still_runs_everywhere() {
    let rules = rule_dir(
        "lint-custom-dialects-default",
        r#"(defrule no-print :pattern (print ?x) :message "m")"#,
    );
    let dir = fresh_temp_dir("lint-dialects-default-run");
    let el_file = dir.join("a.el");
    fs::write(&el_file, "(print x)\n").expect("write a.el");
    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--output",
                "json",
                "--rule",
                "no-print",
                "--custom-rules",
            ])
            .arg(&rules)
            .arg(&el_file)
            .assert()
            .success(),
    );
    assert_eq!(value["finding_count"], 1, "{value}");
}

/// FR-E14: a `defpattern` fragment, referenced with `(:fragment name)`, is
/// substituted into a rule's `:pattern` before matching.
#[test]
fn cli_lint_a_rule_can_reuse_a_named_pattern_fragment() {
    let rules = rule_dir(
        "lint-custom-fragment",
        r#"(defpattern bare-print (print ?x))
           (defrule no-bare-print-in-progn
             :pattern (progn (:fragment bare-print))
             :message "no bare print inside progn")"#,
    );
    let dir = fresh_temp_dir("lint-fragment-run");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn (print 1))\n(print 2)\n").expect("write a.lisp");
    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--output",
                "json",
                "--rule",
                "no-bare-print-in-progn",
                "--custom-rules",
            ])
            .arg(&rules)
            .arg(&file)
            .assert()
            .success(),
    );
    // Only the form matching the whole `(progn (print ?x))` shape, not the
    // bare `(print 2)` outside a `progn`.
    assert_eq!(value["finding_count"], 1, "{value}");
}

#[test]
fn cli_lint_rejects_a_rule_referencing_an_undefined_fragment() {
    let rules = rule_dir(
        "lint-custom-fragment-undefined",
        r#"(defrule r :pattern (:fragment ghost) :message "m")"#,
    );
    paredit()
        .args(["inspect", "lint", "--list-rules", "--custom-rules"])
        .arg(&rules)
        .assert()
        .failure()
        .stderr(predicate::str::contains("undefined pattern fragment"));
}

#[test]
fn cli_lint_rejects_a_self_referential_pattern_fragment() {
    let rules = rule_dir(
        "lint-custom-fragment-cycle",
        r#"(defpattern loopy (progn (:fragment loopy)))
           (defrule r :pattern (f) :message "m")"#,
    );
    paredit()
        .args(["inspect", "lint", "--list-rules", "--custom-rules"])
        .arg(&rules)
        .assert()
        .failure()
        .stderr(predicate::str::contains("cycle"));
}

/// FR-E17: two custom rules sharing a name are rejected at load time, rather
/// than the pass loop, the metadata cache, and `deftest` resolution each
/// silently picking a different one.
#[test]
fn cli_lint_rejects_two_custom_rules_sharing_a_name() {
    let rules = rule_dir(
        "lint-custom-self-collision",
        r#"(defrule dup :pattern (f ?x) :message "m")
           (defrule dup :pattern (g ?x) :message "m")"#,
    );
    paredit()
        .args(["inspect", "lint", "--list-rules", "--custom-rules"])
        .arg(&rules)
        .assert()
        .failure()
        .stderr(predicate::str::contains("two custom rules are named"));
}

/// FR-E5: `--timings` reports the loaded custom rules' own per-rule cost in
/// a separate section, alongside the built-in suite's.
#[test]
fn cli_lint_timings_includes_a_custom_rules_section() {
    let rules = rule_dir(
        "lint-custom-timings",
        r#"(defrule no-bare-print :pattern (print ?x) :message "m")"#,
    );
    let dir = fresh_temp_dir("lint-custom-timings-run");
    let file = dir.join("a.lisp");
    fs::write(&file, "(print 1)\n").expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--timings",
                "--output",
                "json",
                "--custom-rules",
            ])
            .arg(&rules)
            .arg(&file)
            .assert()
            .success(),
    );
    let custom_rules = value["custom_rules"]["rules"]
        .as_array()
        .expect("custom_rules.rules array");
    assert_eq!(custom_rules.len(), 1);
    assert_eq!(custom_rules[0]["rule"], "no-bare-print");
    assert!(
        custom_rules[0]["invocations"]
            .as_u64()
            .expect("invocations")
            > 0
    );
}

#[test]
fn cli_lint_list_rules_shows_the_projects_own_rules() {
    let rules = rule_dir(
        "lint-custom-list",
        r#"(defrule house-style :category naming :pattern (print ?x) :message "m")"#,
    );
    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--list-rules", "--custom-rules"])
            .arg(&rules)
            .args(["--output", "json"])
            .assert(),
    );
    let custom = value["custom_rules"]
        .as_array()
        .expect("custom_rules array");
    assert_eq!(custom.len(), 1);
    assert_eq!(custom[0]["rule"], "house-style");
    assert_eq!(custom[0]["category"], "naming");
}

/// `--docs` includes a project's own rules in their own section, with the
/// `deftest` clauses rendered as worked examples straight from the rule file
/// — not the shipped catalogue's `RULE_DOCS`, which knows nothing about them.
#[test]
fn cli_lint_docs_includes_the_projects_own_rules_with_worked_examples() {
    let rules = rule_dir(
        "lint-custom-docs",
        r#"(defrule no-bare-print
             :category suspicious
             :severity error
             :description "print writes to *standard-output* directly"
             :pattern (print ?x)
             :message "use (format t ...) rather than print")
           (deftest no-bare-print
             (:matches "(print 1)")
             (:no-match "(princ 1)"))"#,
    );
    let docs = String::from_utf8(
        paredit()
            .args(["inspect", "lint", "--docs", "--custom-rules"])
            .arg(&rules)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .expect("UTF-8 docs");

    assert!(docs.contains("# Custom rules"), "{docs}");
    assert!(docs.contains("### `no-bare-print`"), "{docs}");
    assert!(
        docs.contains("use (format t ...) rather than print"),
        "{docs}"
    );
    // The worked examples come from the rule's own `deftest`, not hand-written
    // prose.
    assert!(docs.contains("(print 1)"), "{docs}");
    assert!(docs.contains("(princ 1)"), "{docs}");
}

/// A project with no custom rules loaded gets no "Custom rules" section at
/// all, rather than an empty heading.
#[test]
fn cli_lint_docs_omits_the_custom_section_with_no_custom_rules() {
    let docs = String::from_utf8(
        paredit()
            .args(["inspect", "lint", "--docs"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .expect("UTF-8 docs");
    assert!(!docs.contains("# Custom rules"), "{docs}");
}

/// `--explain` on a custom rule name no longer hard-errors "unknown lint
/// rule" — it explains the rule using its own metadata, the same way it
/// already shows up in `--list-rules`/`--docs`.
#[test]
fn cli_lint_explain_reports_a_custom_rules_own_metadata() {
    let rules = rule_dir(
        "lint-custom-explain",
        r#"(defrule no-bare-print
             :category suspicious
             :severity error
             :description "print writes to *standard-output* directly"
             :pattern (print ?x)
             :message "use (format t ...) rather than print")"#,
    );
    let value = json_stdout(
        paredit()
            .args([
                "inspect",
                "lint",
                "--explain",
                "no-bare-print",
                "--custom-rules",
            ])
            .arg(&rules)
            .args(["--output", "json"])
            .assert()
            .success(),
    );
    assert_eq!(value["rule"], "no-bare-print");
    assert_eq!(value["source"], "custom");
    assert_eq!(value["category"], "suspicious");
    assert_eq!(value["severity"], "error");
    assert_eq!(value["fixable"], false);
    assert_eq!(
        value["description"],
        "print writes to *standard-output* directly"
    );
    assert_eq!(value["message"], "use (format t ...) rather than print");
    // No `doc_url` — a custom rule has none, and it should be omitted rather
    // than reported as an error or a made-up value.
    assert!(value.get("doc_url").is_none(), "{value}");
}

/// `--explain` on a name that is neither a shipped rule nor a loaded custom
/// one still refuses the same way it always did.
#[test]
fn cli_lint_explain_still_rejects_an_unknown_rule_with_custom_rules_loaded() {
    let rules = rule_dir(
        "lint-custom-explain-unknown",
        r#"(defrule no-bare-print :pattern (print ?x) :message "m")"#,
    );
    paredit()
        .args([
            "inspect",
            "lint",
            "--explain",
            "not-a-real-rule",
            "--custom-rules",
        ])
        .arg(&rules)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown lint rule"));
}

/// stdout as a `String`, for the modes whose output is not JSON.
fn json_stdout_or_text(assert: assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).expect("UTF-8 stdout")
}

// ---------------------------------------------------------------------------
// --suggest-severity (FR-E9)
// ---------------------------------------------------------------------------

/// A currently-`error` rule firing on every scanned file suggests demoting it
/// to `warning`, and a currently-`warning` rule that never fires at all
/// suggests promoting it to `error`. `self-assignment` and `manual-incf` are
/// each real shipped rules at those severities (see `--list-rules`).
#[test]
fn cli_lint_suggest_severity_flags_both_directions() {
    let dir = fresh_temp_dir("lint-suggest-severity");
    let a = dir.join("a.lisp");
    let b = dir.join("b.lisp");
    // self-assignment (error) fires in both files; manual-incf (warning)
    // never fires anywhere in this workspace.
    fs::write(&a, "(defun foo (x) (setf x x) (setf x x))\n").expect("write a.lisp");
    fs::write(&b, "(defun bar (y) (setf y y))\n").expect("write b.lisp");

    let stdout = paredit()
        .args([
            "inspect",
            "lint",
            "--suggest-severity",
            "--rule",
            "self-assignment",
            "--rule",
            "manual-incf",
            "--output",
            "json",
        ])
        .arg(&dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let report: serde_json::Value = serde_json::from_slice(&stdout).expect("valid JSON");
    assert_eq!(report["files_scanned"], 2);
    assert_eq!(report["suggestion_count"], 2);

    let suggestions = report["suggestions"].as_array().expect("suggestions array");
    let by_rule = |rule: &str| {
        suggestions
            .iter()
            .find(|entry| entry["rule"] == rule)
            .unwrap_or_else(|| panic!("no suggestion for {rule}"))
    };

    let self_assignment = by_rule("self-assignment");
    assert_eq!(self_assignment["current_severity"], "error");
    assert_eq!(self_assignment["suggested_severity"], "warning");
    assert_eq!(self_assignment["finding_count"], 3);
    assert_eq!(self_assignment["files_with_finding"], 2);
    assert_eq!(self_assignment["density_bucket"], "very high");

    let manual_incf = by_rule("manual-incf");
    assert_eq!(manual_incf["current_severity"], "warning");
    assert_eq!(manual_incf["suggested_severity"], "error");
    assert_eq!(manual_incf["finding_count"], 0);
    assert_eq!(manual_incf["density_bucket"], "never");
}

/// A rule whose severity already matches its firing rate (an `error` rule
/// that never fires, the ordinary case for a rule that catches real bugs)
/// gets no suggestion.
#[test]
fn cli_lint_suggest_severity_omits_rules_whose_severity_already_fits() {
    let dir = fresh_temp_dir("lint-suggest-severity-quiet");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun add (a b) (+ a b))\n").expect("write a.lisp");

    let stdout = paredit()
        .args([
            "inspect",
            "lint",
            "--suggest-severity",
            "--rule",
            "self-assignment",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let report: serde_json::Value = serde_json::from_slice(&stdout).expect("valid JSON");
    assert_eq!(report["suggestion_count"], 0);
    assert_eq!(report["suggestions"].as_array().unwrap().len(), 0);
}

/// The text rendering prints the same prose message documented for
/// `--suggest-severity`, not just a machine-readable JSON shape.
#[test]
fn cli_lint_suggest_severity_text_output_is_prose() {
    let dir = fresh_temp_dir("lint-suggest-severity-text");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun foo (x) (setf x x))\n").expect("write a.lisp");

    paredit()
        .args([
            "inspect",
            "lint",
            "--suggest-severity",
            "--rule",
            "self-assignment",
            "--output",
            "text",
        ])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rule self-assignment: 1 finding(s) across 1 file(s) (very high density) -- consider Warning instead of Error",
        ));
}

/// **Critical invariant**: `--suggest-severity` is advisory-only. It must
/// never write `paredit.toml` (byte-identical before and after), and it must
/// never change the exit code a subsequent, ordinary `inspect lint` run gets —
/// whether that run passes or fails its own gate.
#[test]
fn cli_lint_suggest_severity_never_writes_config_or_changes_gate_exit_code() {
    let dir = fresh_temp_dir("lint-suggest-severity-invariant");
    let file = dir.join("a.lisp");
    // Fires self-assignment (error), which trips --fail-on-finding.
    fs::write(&file, "(defun foo (x) (setf x x))\n").expect("write a.lisp");

    let config = dir.join("paredit.toml");
    let config_contents = "[lint]\npreset = \"recommended\"\n";
    fs::write(&config, config_contents).expect("write paredit.toml");
    let config_before = fs::read(&config).expect("read paredit.toml before");

    // A normal run's gate outcome, taken before --suggest-severity ever runs.
    let exit_code_before = paredit()
        .args(["inspect", "lint", "--fail-on-finding"])
        .arg(&file)
        .assert()
        .code(3)
        .get_output()
        .status
        .code();

    // --suggest-severity itself must exit 0 regardless of what it finds, and
    // must not touch paredit.toml.
    paredit()
        .args(["inspect", "lint", "--suggest-severity", "--output", "json"])
        .arg(&file)
        .assert()
        .success();

    let config_after = fs::read(&config).expect("read paredit.toml after");
    assert_eq!(
        config_before, config_after,
        "--suggest-severity must never mutate paredit.toml"
    );

    // The same ordinary run, after --suggest-severity, must trip the same
    // gate the same way.
    let exit_code_after = paredit()
        .args(["inspect", "lint", "--fail-on-finding"])
        .arg(&file)
        .assert()
        .code(3)
        .get_output()
        .status
        .code();

    assert_eq!(
        exit_code_before, exit_code_after,
        "--suggest-severity must not change a later ordinary run's exit code"
    );
}

/// `--suggest-severity` is mutually exclusive with `--stats`, matching the
/// other single-purpose report modes on this command (`--fix`, `--sarif`,
/// `--github`, `--list-rules`).
#[test]
fn cli_lint_suggest_severity_conflicts_with_stats() {
    let dir = fresh_temp_dir("lint-suggest-severity-conflict");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list '5)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "lint", "--suggest-severity", "--stats"])
        .arg(&file)
        .assert()
        .failure()
        .code(2);
}
