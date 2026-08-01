use super::*;

// ---------------------------------------------------------------------------
// `custom.rs`'s module doc claims a custom rule's finding is interned so it
// "travel[s] through the summary, the gate, the baseline, the suppression
// scan, the fix map, SARIF, and the GitHub annotations with no second code
// path anywhere". `tests/cli/lint_report.rs` pins that claim for most of the
// list — but nothing exercised `--baseline`/`--write-baseline` or
// `--suppress-path` against a custom finding specifically. These tests do.
// ---------------------------------------------------------------------------

/// Writes a minimal custom-rule directory (a single `.lisp` file) and returns
/// its path.
fn rule_dir(name: &str, rules: &str) -> PathBuf {
    let dir = fresh_temp_dir(name).join("rules");
    fs::create_dir_all(&dir).expect("create rule dir");
    fs::write(dir.join("house.lisp"), rules).expect("write house.lisp");
    dir
}

/// `--write-baseline` must capture a custom rule's finding exactly as it
/// captures a shipped one's, or a later `--baseline` run has no entry to
/// suppress it with. Both findings are baselined and suppressed together, by
/// the same path/rule/line-content-hash identity `retain_unbaselined` applies
/// to every rule alike.
#[test]
fn cli_lint_baseline_round_trip_suppresses_a_custom_rule_finding_like_a_shipped_one() {
    let rules = rule_dir(
        "lint-custom-baseline-contract",
        r#"(defrule no-bare-print :pattern (print ?x) :message "m")"#,
    );
    let dir = fresh_temp_dir("lint-custom-baseline-contract-src");
    let file = dir.join("a.lisp");
    let base = dir.join("base.json");
    // One custom finding (no-bare-print) and one shipped finding
    // (self-assignment), so the baseline round trip is checked for both at
    // once.
    fs::write(&file, "(print 1)\n(setq x x)\n").expect("write a.lisp");

    let written = json_stdout(
        paredit()
            .args(["inspect", "lint", "--custom-rules"])
            .arg(&rules)
            .arg("--write-baseline")
            .arg(&base)
            .args(["--output", "json"])
            .arg(&file)
            .assert()
            .success(),
    );
    assert_eq!(written["entry_count"], 2, "{written}");

    let baseline: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&base).expect("read baseline")).expect("json");
    let entries = baseline["entries"].as_array().expect("entries array");
    let rules_in_baseline: Vec<&str> = entries
        .iter()
        .map(|entry| entry["rule"].as_str().expect("rule"))
        .collect();
    assert!(
        rules_in_baseline.contains(&"no-bare-print"),
        "the custom finding is missing from the baseline: {baseline}"
    );
    assert!(
        rules_in_baseline.contains(&"self-assignment"),
        "the shipped finding is missing from the baseline: {baseline}"
    );

    // Re-linting with the baseline hides both — the custom finding is
    // suppressed by the exact same identity match as the shipped one.
    let rerun = json_stdout(
        paredit()
            .args(["inspect", "lint", "--custom-rules"])
            .arg(&rules)
            .arg("--baseline")
            .arg(&base)
            .args(["--output", "json"])
            .arg(&file)
            .assert()
            .success(),
    );
    assert_eq!(rerun["finding_count"], 0, "{rerun}");
}

/// A finding introduced after the baseline was written is still reported —
/// for a custom rule exactly as for a shipped one — so `--baseline` narrows
/// to new findings rather than silencing the rule outright.
#[test]
fn cli_lint_baseline_reports_a_new_custom_rule_finding_alongside_a_new_shipped_one() {
    let rules = rule_dir(
        "lint-custom-baseline-contract-new",
        r#"(defrule no-bare-print :pattern (print ?x) :message "m")"#,
    );
    let dir = fresh_temp_dir("lint-custom-baseline-contract-new-src");
    let file = dir.join("a.lisp");
    let base = dir.join("base.json");
    fs::write(&file, "(print 1)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "lint", "--custom-rules"])
        .arg(&rules)
        .arg("--write-baseline")
        .arg(&base)
        .arg(&file)
        .assert()
        .success();

    // Add a second custom finding and a shipped one alongside the baselined
    // custom finding.
    fs::write(&file, "(print 1)\n(print 2)\n(setq x x)\n").expect("rewrite a.lisp");

    let rerun = json_stdout(
        paredit()
            .args(["inspect", "lint", "--custom-rules"])
            .arg(&rules)
            .arg("--baseline")
            .arg(&base)
            .args(["--output", "json"])
            .arg(&file)
            .assert()
            .success(),
    );
    assert_eq!(rerun["finding_count"], 2, "{rerun}");
    let rules_reported: Vec<&str> = rerun["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .map(|finding| finding["rule"].as_str().expect("rule"))
        .collect();
    assert!(rules_reported.contains(&"no-bare-print"), "{rerun}");
    assert!(rules_reported.contains(&"self-assignment"), "{rerun}");
}

/// `--suppress-path` drops a whole file before either pass runs, so it was
/// already rule-agnostic in how it is implemented — but nothing pinned that a
/// custom rule's finding is actually silenced under a suppressed prefix, the
/// same as a shipped one's.
#[test]
fn cli_lint_suppress_path_silences_a_custom_rule_finding_like_a_shipped_one() {
    let rules = rule_dir(
        "lint-custom-suppress-path-contract",
        r#"(defrule no-bare-print :pattern (print ?x) :message "m")"#,
    );
    let dir = fresh_temp_dir("lint-custom-suppress-path-contract-src");
    let suppressed = dir.join("legacy");
    fs::create_dir_all(&suppressed).expect("create suppressed dir");
    let file = suppressed.join("a.lisp");
    // One custom finding and one shipped finding, both under the suppressed
    // directory.
    fs::write(&file, "(print 1)\n(setq x x)\n").expect("write a.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--custom-rules"])
            .arg(&rules)
            .arg("--suppress-path")
            .arg(&suppressed)
            .args(["--output", "json"])
            .arg(&dir)
            .assert()
            .success(),
    );
    assert_eq!(value["finding_count"], 0, "{value}");
}

/// The mirror of the previous test: a file that is *not* under the suppressed
/// prefix still reports both the custom and the shipped finding, so
/// `--suppress-path` is narrowing by path and not accidentally disabling the
/// custom pass entirely.
#[test]
fn cli_lint_suppress_path_leaves_a_custom_rule_finding_outside_the_prefix_untouched() {
    let rules = rule_dir(
        "lint-custom-suppress-path-contract-control",
        r#"(defrule no-bare-print :pattern (print ?x) :message "m")"#,
    );
    let dir = fresh_temp_dir("lint-custom-suppress-path-contract-control-src");
    let suppressed = dir.join("legacy");
    fs::create_dir_all(&suppressed).expect("create suppressed dir");
    fs::write(suppressed.join("a.lisp"), "(print 1)\n").expect("write suppressed a.lisp");
    let kept = dir.join("kept.lisp");
    fs::write(&kept, "(print 1)\n(setq x x)\n").expect("write kept.lisp");

    let value = json_stdout(
        paredit()
            .args(["inspect", "lint", "--custom-rules"])
            .arg(&rules)
            .arg("--suppress-path")
            .arg(&suppressed)
            .args(["--output", "json"])
            .arg(&dir)
            .assert()
            .success(),
    );
    assert_eq!(value["finding_count"], 2, "{value}");
    let rules_reported: Vec<&str> = value["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .map(|finding| finding["rule"].as_str().expect("rule"))
        .collect();
    assert!(rules_reported.contains(&"no-bare-print"), "{value}");
    assert!(rules_reported.contains(&"self-assignment"), "{value}");
}

/// stdout parsed as JSON.
fn json_stdout(assert: assert_cmd::assert::Assert) -> serde_json::Value {
    serde_json::from_slice(&assert.get_output().stdout).expect("valid JSON")
}
