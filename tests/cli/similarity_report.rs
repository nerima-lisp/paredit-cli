use std::fs;

use predicates::prelude::*;
use serde_json::{Value, json};

use super::{fresh_temp_dir, paredit};

#[test]
fn cli_reports_cross_file_similarity_as_json() {
    let dir = fresh_temp_dir("similarity-json");
    let left = dir.join("left.lisp");
    let right = dir.join("right.lisp");
    fs::write(&left, "(defun alpha (x) (+ x 1))\n").unwrap();
    fs::write(&right, "(defun beta (y) (+ y 2))\n").unwrap();
    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold")
        .arg("0.8")
        .arg(&right)
        .arg(&left)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pairs = report["pairs"].as_array().unwrap();
    assert_eq!(report["pair_count"].as_u64().unwrap() as usize, pairs.len());
    assert!(!pairs.is_empty());
    assert!(
        pairs
            .iter()
            .all(|pair| pair["similarity"].as_f64().unwrap() >= 0.8)
    );
    assert_ne!(pairs[0]["left"]["path"], pairs[0]["right"]["path"]);
}

#[test]
fn cli_accepts_exact_similarity_threshold_endpoint() {
    let dir = fresh_temp_dir("similarity-threshold-one");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a b) (foo a b) (foo a c)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=1")
        .arg(&file)
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pairs = report["pairs"].as_array().unwrap();
    assert!(!pairs.is_empty());
    assert!(
        pairs
            .iter()
            .all(|pair| pair["similarity"].as_f64() == Some(1.0))
    );
}

#[cfg(unix)]
#[test]
fn cli_deduplicates_the_same_canonical_input_path() {
    let dir = fresh_temp_dir("similarity-deduplicate-input");
    let file = dir.join("single.lisp");
    let alias = dir.join("alias.lisp");
    fs::write(&file, "(foo a b)\n").unwrap();
    std::os::unix::fs::symlink(&file, &alias).unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg(&file)
        .arg(&alias)
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["pair_count"].as_u64(), Some(0));
    assert_eq!(report["pairs"].as_array().unwrap().len(), 0);
}

#[test]
fn cli_renders_text_report() {
    let dir = fresh_temp_dir("similarity-text");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a b) (foo x y)\n").unwrap();
    paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold")
        .arg("0.8")
        .arg("--output")
        .arg("text")
        .arg(file)
        .assert()
        .success()
        .stdout(predicate::str::contains("pair_count\t1"))
        .stdout(predicate::str::contains("similarity="));
}

#[test]
fn cli_rejects_invalid_thresholds() {
    let dir = fresh_temp_dir("similarity-invalid");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a b)\n").unwrap();
    paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold")
        .arg("1.1")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--threshold must be between"));
    paredit()
        .args(["inspect", "similarity"])
        .arg("--min-node-count")
        .arg("1")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--min-node-count must be at least 2",
        ));
}

#[test]
fn fail_on_duplicates_preserves_stdout_report() {
    let dir = fresh_temp_dir("similarity-policy");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a b) (foo x y)\n").unwrap();
    paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold")
        .arg("0.8")
        .arg("--fail-on-duplicates")
        .arg(file)
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"pair_count\": 1"))
        .stderr(predicate::str::contains("similarity-report policy failed:"));
}

#[test]
fn cli_recursively_discovers_sources_and_skips_hidden_and_generated_directories() {
    let dir = fresh_temp_dir("similarity-discovery");
    let nested = dir.join("src").join("nested");
    let hidden = dir.join(".private");
    let generated = dir.join("target");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all(&hidden).unwrap();
    fs::create_dir_all(&generated).unwrap();
    fs::write(nested.join("left.lisp"), "(foo a b)\n").unwrap();
    fs::write(dir.join("right.lisp"), "(foo x y)\n").unwrap();
    fs::write(hidden.join("hidden.lisp"), "(foo h i)\n").unwrap();
    fs::write(generated.join("generated.lisp"), "(foo g h)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold")
        .arg("0")
        .arg("--min-node-count")
        .arg("2")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["scanned_files"], 2);
    assert_eq!(report["summary"]["skipped_hidden"], 1);
    assert_eq!(report["summary"]["skipped_generated"], 1);
    assert_eq!(report["summary"]["matched_pairs"], 1);
}

#[test]
fn cli_excludes_repeated_file_and_subtree_paths_without_prefix_false_positives() {
    let dir = fresh_temp_dir("similarity-exclude-json");
    let excluded_dir = dir.join("vendor");
    let prefix_sibling = dir.join("vendor-copy");
    let excluded_file = dir.join("excluded.lisp");
    fs::create_dir_all(&excluded_dir).unwrap();
    fs::create_dir_all(&prefix_sibling).unwrap();
    fs::write(dir.join("keep.lisp"), "(foo a b)\n").unwrap();
    fs::write(&excluded_file, "(foo excluded file)\n").unwrap();
    fs::write(excluded_dir.join("nested.lisp"), "(foo excluded dir)\n").unwrap();
    fs::write(prefix_sibling.join("keep.lisp"), "(foo x y)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=0")
        .arg("--exclude")
        .arg(&excluded_dir)
        .arg("--exclude")
        .arg(&excluded_file)
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["options"]["exclude"],
        serde_json::json!([
            excluded_dir.display().to_string(),
            excluded_file.display().to_string()
        ])
    );
    assert_eq!(report["summary"]["scanned_files"], 2);
    assert_eq!(report["summary"]["skipped_excluded"], 2);
    assert_eq!(report["pair_count"], 1);
}

#[test]
fn cli_resolves_relative_excludes_from_cwd_and_reports_text_summary() {
    let dir = fresh_temp_dir("similarity-exclude-text");
    fs::write(dir.join("keep.lisp"), "(foo a b)\n").unwrap();
    fs::write(dir.join("excluded.lisp"), "(foo x y)\n").unwrap();

    paredit()
        .current_dir(&dir)
        .args(["inspect", "similarity"])
        .arg("--output=text")
        .arg("--exclude=excluded.lisp")
        .arg(".")
        .assert()
        .success()
        .stdout(predicate::str::contains("scanned_files\t1"))
        .stdout(predicate::str::contains("skipped_excluded\t1"));
}

#[test]
fn cli_dialect_override_includes_unknown_extensions() {
    let dir = fresh_temp_dir("similarity-dialect-override");
    let left = dir.join("left.txt");
    let right = dir.join("right.data");
    fs::write(&left, "(foo a b)\n").unwrap();
    fs::write(&right, "(foo x y)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--threshold")
        .arg("0")
        .arg("--min-node-count")
        .arg("2")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["options"]["dialect"], "common-lisp");
    assert_eq!(report["summary"]["scanned_files"], 2);
    assert_eq!(report["summary"]["skipped_unknown"], 0);
    assert!(report["pairs"].as_array().unwrap().iter().all(|pair| {
        pair["left"]["dialect"] == "common-lisp" && pair["right"]["dialect"] == "common-lisp"
    }));
}

#[test]
fn cli_applies_detected_reader_policy_to_similarity_inputs() {
    let dir = fresh_temp_dir("similarity-reader-policy");
    let common_lisp_file = dir.join("common.lisp");
    let emacs_lisp_file = dir.join("emacs.el");
    let invalid_common_lisp_file = dir.join("invalid.lisp");
    fs::write(&common_lisp_file, "(list #\\))\n(list #\\))\n").unwrap();
    fs::write(&emacs_lisp_file, "(list ?\\))\n(list ?\\))\n").unwrap();
    fs::write(&invalid_common_lisp_file, "#?value\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=1")
        .arg("--min-node-count=2")
        .arg("--error-policy=skip")
        .arg(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["scanned_files"], 3);
    assert_eq!(report["summary"]["processed_files"], 2);
    assert_eq!(report["summary"]["skipped_error_files"], 1);
    let errors = report["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0]["path"],
        invalid_common_lisp_file.display().to_string()
    );
    assert_eq!(errors[0]["stage"], "parse");
    assert!(
        errors[0]["message"]
            .as_str()
            .unwrap()
            .contains("unsupported reader dispatch")
    );
}

#[test]
fn cli_json_contract_reports_options_summary_and_pair_count() {
    let dir = fresh_temp_dir("similarity-json-contract");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a) (foo b) (foo c)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold")
        .arg("0")
        .arg("--min-node-count")
        .arg("2")
        .arg("--overlap-policy")
        .arg("all")
        .arg("--max-results")
        .arg("1")
        .arg(&file)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["options"]["threshold"], 0.0);
    assert_eq!(report["options"]["min_node_count"], 2);
    assert_eq!(report["options"]["min_line_span"], 1);
    assert_eq!(report["options"]["comparison_scope"], "all");
    assert_eq!(report["options"]["form_scope"], "all");
    assert_eq!(report["options"]["overlap_policy"], "all");
    assert_eq!(report["options"]["max_candidates"], Value::Null);
    assert_eq!(report["options"]["max_comparisons"], Value::Null);
    assert_eq!(report["options"]["max_results"], 1);
    assert_eq!(report["options"]["error_policy"], "fail");
    assert_eq!(report["summary"]["processed_files"], 1);
    assert_eq!(report["summary"]["skipped_error_files"], 0);
    assert_eq!(report["summary"]["matched_pairs"], 3);
    assert_eq!(report["summary"]["reported_pairs"], 1);
    assert_eq!(report["summary"]["truncated"], true);
    assert_eq!(report["summary"]["candidate_limit_reached"], false);
    assert_eq!(report["summary"]["omitted_candidates"], 0);
    assert_eq!(report["summary"]["comparison_limit_reached"], false);
    assert_eq!(report["summary"]["unprocessed_pairs"], 0);
    assert_eq!(report["pair_count"], 1);
    assert_eq!(report["pairs"].as_array().unwrap().len(), 1);
    assert_eq!(report["errors"].as_array().unwrap().len(), 0);
}

#[test]
fn cli_fails_on_malformed_input_by_default() {
    let dir = fresh_temp_dir("similarity-error-fail");
    let invalid = dir.join("invalid.lisp");
    fs::write(&invalid, "(").unwrap();

    paredit()
        .args(["inspect", "similarity"])
        .arg(&invalid)
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to parse"))
        .stderr(predicate::str::contains(invalid.display().to_string()));
}

#[test]
fn cli_skips_malformed_input_and_reports_stable_json_errors() {
    let dir = fresh_temp_dir("similarity-error-skip");
    let valid = dir.join("valid.lisp");
    let invalid = dir.join("invalid.lisp");
    fs::write(&valid, "(foo a b) (foo a b)\n").unwrap();
    fs::write(&invalid, "(").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=1")
        .arg("--error-policy=skip")
        .arg(&valid)
        .arg(&invalid)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["options"]["error_policy"], "skip");
    assert_eq!(report["summary"]["scanned_files"], 2);
    assert_eq!(report["summary"]["processed_files"], 1);
    assert_eq!(report["summary"]["skipped_error_files"], 1);
    assert_eq!(report["pair_count"], 1);
    let errors = report["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["path"], invalid.display().to_string());
    assert_eq!(errors[0]["stage"], "parse");
    assert!(
        !errors[0]["message"]
            .as_str()
            .unwrap()
            .contains(&invalid.display().to_string())
    );
}

#[test]
fn cli_skip_succeeds_with_an_empty_report_when_all_inputs_are_invalid() {
    let dir = fresh_temp_dir("similarity-error-all-invalid");
    let first = dir.join("a.lisp");
    let second = dir.join("b.lisp");
    fs::write(&first, "(").unwrap();
    fs::write(&second, "(").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--error-policy=skip")
        .arg(&second)
        .arg(&first)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["processed_files"], 0);
    assert_eq!(report["summary"]["skipped_error_files"], 2);
    assert_eq!(report["pair_count"], 0);
    let errors = report["errors"].as_array().unwrap();
    assert_eq!(errors[0]["path"], first.display().to_string());
    assert_eq!(errors[1]["path"], second.display().to_string());
}

#[test]
fn cli_skip_still_applies_fail_on_duplicates_to_successful_files() {
    let dir = fresh_temp_dir("similarity-error-policy-duplicates");
    let valid = dir.join("valid.lisp");
    let invalid = dir.join("invalid.lisp");
    fs::write(&valid, "(foo a b) (foo a b)\n").unwrap();
    fs::write(&invalid, "(").unwrap();

    paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=1")
        .arg("--error-policy=skip")
        .arg("--fail-on-duplicates")
        .arg(&invalid)
        .arg(&valid)
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"skipped_error_files\": 1"))
        .stdout(predicate::str::contains("\"pair_count\": 1"))
        .stderr(predicate::str::contains("similarity-report policy failed:"));
}

#[test]
fn fail_on_duplicates_rejects_skipped_files_as_indeterminate() {
    let dir = fresh_temp_dir("similarity-error-policy-indeterminate");
    let valid = dir.join("valid.lisp");
    let invalid = dir.join("invalid.lisp");
    fs::write(&valid, "(foo a b)\n").unwrap();
    fs::write(&invalid, [0xff, 0xfe]).unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--error-policy=skip")
        .arg("--fail-on-duplicates")
        .arg(&valid)
        .arg(&invalid)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["pair_count"], 0);
    assert_eq!(report["summary"]["skipped_error_files"], 1);
    let errors = report["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["path"], invalid.display().to_string());
    assert_eq!(errors[0]["stage"], "read");
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "similarity-report policy indeterminate: 1 file(s) skipped due to processing errors"
    ));
}

#[test]
fn cli_text_report_includes_skipped_file_summary_and_error() {
    let dir = fresh_temp_dir("similarity-error-text");
    let invalid = dir.join("invalid.lisp");
    fs::write(&invalid, "(").unwrap();

    paredit()
        .args(["inspect", "similarity"])
        .arg("--error-policy=skip")
        .arg("--output=text")
        .arg(&invalid)
        .assert()
        .success()
        .stdout(predicate::str::contains("processed_files\t0"))
        .stdout(predicate::str::contains("skipped_error_files\t1"))
        .stdout(predicate::str::contains(format!(
            "error\t{}\tparse\t",
            invalid.display()
        )));
}

#[test]
fn cli_filters_pairs_by_comparison_scope() {
    let dir = fresh_temp_dir("similarity-comparison-scope");
    let left = dir.join("left.lisp");
    let right = dir.join("right.lisp");
    fs::write(&left, "(foo a b) (foo a b)\n").unwrap();
    fs::write(&right, "(foo a b) (foo a b)\n").unwrap();

    for (scope, same_file) in [("same-file", true), ("cross-file", false)] {
        let output = paredit()
            .args(["inspect", "similarity"])
            .arg("--threshold=1")
            .arg("--min-node-count=2")
            .arg("--overlap-policy=all")
            .arg(format!("--comparison-scope={scope}"))
            .arg(&left)
            .arg(&right)
            .output()
            .unwrap();
        assert!(output.status.success());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["options"]["comparison_scope"], scope);
        let pairs = report["pairs"].as_array().unwrap();
        assert!(!pairs.is_empty());
        assert!(
            pairs
                .iter()
                .all(|pair| { (pair["left"]["path"] == pair["right"]["path"]) == same_file })
        );
    }
}

#[test]
fn cli_top_level_form_scope_excludes_nested_candidates() {
    let dir = fresh_temp_dir("similarity-form-scope");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(outer (foo a b) (foo a b))\n").unwrap();

    let all_output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=1")
        .arg("--min-node-count=2")
        .arg("--overlap-policy=all")
        .arg("--form-scope=all")
        .arg(&file)
        .output()
        .unwrap();
    let top_level_output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=1")
        .arg("--min-node-count=2")
        .arg("--overlap-policy=all")
        .arg("--form-scope=top-level")
        .arg(&file)
        .output()
        .unwrap();

    assert!(all_output.status.success());
    assert!(top_level_output.status.success());
    let all: Value = serde_json::from_slice(&all_output.stdout).unwrap();
    let top_level: Value = serde_json::from_slice(&top_level_output.stdout).unwrap();
    assert!(all["pair_count"].as_u64().unwrap() > 0);
    assert_eq!(top_level["pair_count"], 0);
    assert_eq!(top_level["options"]["form_scope"], "top-level");
}

#[test]
fn cli_min_line_span_keeps_only_multiline_candidates() {
    let dir = fresh_temp_dir("similarity-min-line-span");
    let file = dir.join("lines.lisp");
    fs::write(&file, "(foo a b) (foo a b)\n(foo\n a\n b)\n(foo\n a\n b)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=1")
        .arg("--min-node-count=2")
        .arg("--min-line-span=2")
        .arg("--overlap-policy=all")
        .arg(&file)
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["options"]["min_line_span"], 2);
    assert_eq!(report["pair_count"], 1);
    assert!(report["pairs"].as_array().unwrap().iter().all(|pair| {
        pair["left"]["text"].as_str().unwrap().contains('\n')
            && pair["right"]["text"].as_str().unwrap().contains('\n')
    }));
}

#[test]
fn max_results_does_not_hide_duplicates_from_failure_policy() {
    let dir = fresh_temp_dir("similarity-truncated-policy");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a) (foo b) (foo c)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold")
        .arg("0")
        .arg("--min-node-count")
        .arg("2")
        .arg("--overlap-policy")
        .arg("all")
        .arg("--max-results")
        .arg("1")
        .arg("--fail-on-duplicates")
        .arg(&file)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["matched_pairs"], 3);
    assert_eq!(report["pair_count"], 1);
    assert!(String::from_utf8_lossy(&output.stderr).contains("3 duplicate pair(s) found"));
}

#[test]
fn max_candidates_reports_precise_omissions_and_fails_closed() {
    let dir = fresh_temp_dir("similarity-candidate-limit");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a) (bar b) (baz c)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=1")
        .arg("--min-node-count=2")
        .arg("--form-scope=top-level")
        .arg("--max-candidates=2")
        .arg("--fail-on-duplicates")
        .arg(&file)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["options"]["max_candidates"], 2);
    assert_eq!(report["summary"]["candidate_limit_reached"], true);
    assert_eq!(report["summary"]["omitted_candidates"], 1);
    assert_eq!(report["summary"]["matched_pairs"], 0);
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "similarity-report policy indeterminate: candidate limit reached with 1 candidate(s) omitted"
    ));
}

#[test]
fn max_candidates_not_reached_reports_complete_collection() {
    let dir = fresh_temp_dir("similarity-candidate-limit-complete");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a) (bar b) (baz c)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=1")
        .arg("--min-node-count=2")
        .arg("--form-scope=top-level")
        .arg("--max-candidates=3")
        .arg("--fail-on-duplicates")
        .arg(&file)
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["candidate_limit_reached"], false);
    assert_eq!(report["summary"]["omitted_candidates"], 0);
}

#[test]
fn max_comparisons_is_reported_and_failure_uses_processed_matches() {
    let dir = fresh_temp_dir("similarity-comparison-limit");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a) (foo b) (foo c)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=0")
        .arg("--min-node-count=2")
        .arg("--overlap-policy=all")
        .arg("--max-comparisons=1")
        .arg("--fail-on-duplicates")
        .arg(&file)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["options"]["max_comparisons"], 1);
    assert_eq!(report["summary"]["possible_pairs"], 3);
    assert_eq!(report["summary"]["evaluated_pairs"], 1);
    assert_eq!(report["summary"]["unprocessed_pairs"], 2);
    assert_eq!(report["summary"]["comparison_limit_reached"], true);
    assert_eq!(report["summary"]["matched_pairs"], 1);
    assert!(String::from_utf8_lossy(&output.stderr).contains("1 duplicate pair(s) found"));
}

#[test]
fn fail_on_duplicates_rejects_an_indeterminate_comparison_limit() {
    let dir = fresh_temp_dir("similarity-comparison-limit-indeterminate");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a) (bar b) (foo a)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=1")
        .arg("--min-node-count=2")
        .arg("--form-scope=top-level")
        .arg("--overlap-policy=all")
        .arg("--max-comparisons=1")
        .arg("--fail-on-duplicates")
        .arg(&file)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["matched_pairs"], 1);
    assert_eq!(report["summary"]["comparison_limit_reached"], false);
    assert_eq!(report["summary"]["unprocessed_pairs"], 0);
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("similarity-report policy failed: 1 duplicate pair(s) found")
    );
}

#[test]
fn fail_on_duplicates_succeeds_when_comparison_limit_is_not_reached() {
    let dir = fresh_temp_dir("similarity-comparison-limit-complete");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a) (bar b) (baz c)\n").unwrap();

    let output = paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=1")
        .arg("--min-node-count=2")
        .arg("--form-scope=top-level")
        .arg("--overlap-policy=all")
        .arg("--max-comparisons=3")
        .arg("--fail-on-duplicates")
        .arg(&file)
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["matched_pairs"], 0);
    assert_eq!(report["summary"]["comparison_limit_reached"], false);
    assert_eq!(report["summary"]["unprocessed_pairs"], 0);
}

#[test]
fn text_output_reports_max_comparisons_summary() {
    let dir = fresh_temp_dir("similarity-comparison-limit-text");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a) (foo b) (foo c)\n").unwrap();

    paredit()
        .args(["inspect", "similarity"])
        .arg("--threshold=0")
        .arg("--min-node-count=2")
        .arg("--overlap-policy=all")
        .arg("--max-comparisons=1")
        .arg("--output=text")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("max_comparisons\t1"))
        .stdout(predicate::str::contains("comparison_limit_reached\ttrue"))
        .stdout(predicate::str::contains("unprocessed_pairs\t2"));
}

#[test]
fn cli_rejects_non_finite_thresholds_and_zero_max_results() {
    let dir = fresh_temp_dir("similarity-invalid-numeric-options");
    let file = dir.join("suite.lisp");
    fs::write(&file, "(foo a b)\n").unwrap();

    for threshold in ["NaN", "inf", "-inf"] {
        paredit()
            .args(["inspect", "similarity"])
            .arg(format!("--threshold={threshold}"))
            .arg(&file)
            .assert()
            .failure()
            .stderr(predicate::str::contains("--threshold must be between"));
    }
    paredit()
        .args(["inspect", "similarity"])
        .arg("--min-line-span=0")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--min-line-span must be at least 1",
        ));
    paredit()
        .args(["inspect", "similarity"])
        .arg("--max-results")
        .arg("0")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--max-results must be at least 1"));
    paredit()
        .args(["inspect", "similarity"])
        .arg("--max-comparisons=0")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--max-comparisons must be at least 1",
        ));
    paredit()
        .args(["inspect", "similarity"])
        .arg("--max-candidates=0")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--max-candidates must be at least 1",
        ));
}

// --- FR-007: the result cache ---------------------------------------------
//
// The report is quadratic in the candidate count, so a repeated run over an
// unchanged corpus is the expensive case worth avoiding. The property every
// test below is really checking is the one that makes the cache safe rather
// than fast: a reused answer must be one that a fresh run would have given.

/// A corpus varied enough that changing one file changes the answer.
///
/// The forms differ in size on purpose, so the reported similarities are
/// repeating fractions like `0.8545454545454545` rather than the round `1.0`
/// that near-identical forms produce.
///
/// That widens what these tests cover but does not on its own catch a lossy
/// float encoding: whether a given ratio survives an approximate JSON parser
/// depends on the ratio, and no corpus reliably produces one that does not.
/// The guard for that is a unit test over the encoding itself — see
/// `a_similarity_survives_the_entry_bit_for_bit`.
fn similarity_corpus(label: &str) -> std::path::PathBuf {
    let dir = fresh_temp_dir(label);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("alpha.lisp"),
        "(defun alpha (x) (+ x 1))\n\
         (defun alpha-two (x y) (list (+ x 1) (- y 2) (* x y)))\n\
         (defun alpha-three (a b c) (when (and a b) (list a b c (cons a b))))\n",
    )
    .unwrap();
    fs::write(
        dir.join("beta.lisp"),
        "(defun beta (y) (+ y 1))\n\
         (defun beta-two (p q) (list (+ p 3) (- q 4) (* p q) q))\n\
         (defun beta-three (a b) (when (or a b) (list a b (cons b a))))\n",
    )
    .unwrap();
    dir
}

/// Runs the report over `dir`, with `cache` given as `--cache-dir` when set.
fn similarity_run(dir: &std::path::Path, cache: Option<&std::path::Path>, extra: &[&str]) -> Value {
    let mut command = paredit();
    command.args(["inspect", "similarity", "--threshold", "0.6"]);
    if let Some(cache) = cache {
        command.arg("--cache-dir").arg(cache);
    }
    let output = command.args(extra).arg(dir).output().unwrap();
    assert!(
        output.status.success(),
        "inspect similarity failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// The report without the field that says how it was obtained.
///
/// Comparing whole documents rather than a pair count is deliberate: a cache
/// that dropped a form's text or rounded a similarity would still agree on
/// every count in the summary.
fn without_cache_field(mut report: Value) -> Value {
    report.as_object_mut().unwrap().remove("cache");
    report
}

#[test]
fn cli_similarity_reuses_a_cached_report_for_an_unchanged_corpus() {
    let dir = similarity_corpus("similarity-cache");
    // Outside the corpus: a cache written inside it would change the tree it
    // describes.
    let cache = fresh_temp_dir("similarity-cache-store");

    let first = similarity_run(&dir, Some(&cache), &[]);
    assert_eq!(first["cache"], "missing");

    let second = similarity_run(&dir, Some(&cache), &[]);
    assert_eq!(
        second["cache"], "hit",
        "an unchanged corpus must not be recomputed"
    );
    assert_eq!(
        without_cache_field(second),
        without_cache_field(first),
        "a reused report must be identical to the one it replaces"
    );
}

/// The worst failure this cache could have: serving the previous answer after
/// a file changed. Everything else here is a performance nit; this would be a
/// wrong report at exit 0.
#[test]
fn cli_similarity_recomputes_after_one_file_changes() {
    let dir = similarity_corpus("similarity-cache-stale");
    let cache = fresh_temp_dir("similarity-cache-stale-store");

    let cached = similarity_run(&dir, Some(&cache), &[]);
    assert_eq!(cached["cache"], "missing");
    assert_eq!(similarity_run(&dir, Some(&cache), &[])["cache"], "hit");

    // Rewritten to something structurally unlike alpha.lisp, so the answer
    // genuinely differs rather than merely being recomputed.
    fs::write(dir.join("beta.lisp"), "(list 1 2 3)\n").unwrap();

    let recomputed = similarity_run(&dir, Some(&cache), &[]);
    assert_eq!(recomputed["cache"], "missing");
    assert_ne!(
        recomputed["pairs"], cached["pairs"],
        "a changed file must not be reported through the old answer"
    );

    // And the recomputed answer is the *correct* one: identical to what a run
    // with no cache at all gives over the same tree.
    assert_eq!(
        without_cache_field(recomputed),
        similarity_run(&dir, None, &[]),
        "the recomputed report must match an uncached run of the same tree"
    );
}

/// Same file set, different question. The corpus hash cannot catch this, so
/// each option has to be in the key on its own account.
#[test]
fn cli_similarity_does_not_share_an_entry_between_different_options() {
    let dir = similarity_corpus("similarity-cache-options");
    let cache = fresh_temp_dir("similarity-cache-options-store");

    assert_eq!(similarity_run(&dir, Some(&cache), &[])["cache"], "missing");
    assert_eq!(similarity_run(&dir, Some(&cache), &[])["cache"], "hit");

    for flag in [
        ["--overlap-policy", "all"],
        ["--min-line-span", "2"],
        ["--max-results", "3"],
        ["--min-node-count", "3"],
        ["--form-scope", "top-level"],
        ["--comparison-scope", "cross-file"],
    ] {
        assert_eq!(
            similarity_run(&dir, Some(&cache), &flag)["cache"],
            "missing",
            "changing {} must not reuse the previous entry",
            flag[0]
        );
    }

    // The original question is still answered from its own entry.
    assert_eq!(similarity_run(&dir, Some(&cache), &[])["cache"], "hit");
}

/// Caching is opt-in: without the flag there is no cache field, nothing is
/// stored, and the output is what it was before FR-007.
#[test]
fn cli_similarity_without_a_cache_dir_always_recomputes() {
    let dir = similarity_corpus("similarity-cache-optout");

    let first = similarity_run(&dir, None, &[]);
    let second = similarity_run(&dir, None, &[]);

    assert_eq!(first, second);
    assert!(
        first.get("cache").is_none(),
        "a run that asked for no cache must not report one"
    );
}

/// `--clear-cache` has to reach these entries too. They live in a
/// subdirectory of `--cache-dir` that the discovery cache's own clear does not
/// walk, so this is the regression guard for that split.
#[test]
fn cli_similarity_clear_cache_discards_the_stored_report() {
    let dir = similarity_corpus("similarity-cache-clear");
    let cache = fresh_temp_dir("similarity-cache-clear-store");

    assert_eq!(similarity_run(&dir, Some(&cache), &[])["cache"], "missing");
    assert_eq!(similarity_run(&dir, Some(&cache), &[])["cache"], "hit");
    assert_eq!(
        similarity_run(&dir, Some(&cache), &["--clear-cache"])["cache"],
        "missing"
    );
}

/// The text renderer reports the outcome too, and only when asked for.
#[test]
fn cli_similarity_text_output_reports_the_cache_outcome() {
    let dir = similarity_corpus("similarity-cache-text");
    let cache = fresh_temp_dir("similarity-cache-text-store");

    let run = |cached: bool| -> String {
        let mut command = paredit();
        command.args(["inspect", "similarity", "--output", "text"]);
        if cached {
            command.arg("--cache-dir").arg(&cache);
        }
        let output = command.arg(&dir).output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };

    assert!(run(true).contains("cache\tmissing"));
    assert!(run(true).contains("cache\thit"));
    assert!(!run(false).contains("cache\t"));
}

/// A corrupt entry is a slow run, not a wrong one.
#[test]
fn cli_similarity_treats_an_unreadable_entry_as_a_miss() {
    let dir = similarity_corpus("similarity-cache-corrupt");
    let cache = fresh_temp_dir("similarity-cache-corrupt-store");

    let fresh = similarity_run(&dir, Some(&cache), &[]);
    assert_eq!(fresh["cache"], "missing");

    let mut corrupted = 0;
    for shard in fs::read_dir(cache.join("similarity")).unwrap() {
        let shard = shard.unwrap().path();
        if shard.is_dir() {
            for entry in fs::read_dir(&shard).unwrap() {
                fs::write(entry.unwrap().path(), "{ not json").unwrap();
                corrupted += 1;
            }
        }
    }
    assert_eq!(corrupted, 1, "the run must have stored exactly one entry");

    let recomputed = similarity_run(&dir, Some(&cache), &[]);
    assert_eq!(recomputed["cache"], "missing");
    assert_eq!(
        without_cache_field(recomputed),
        without_cache_field(fresh),
        "a corrupt entry must produce the same report, not a broken one"
    );
}

/// The one entry a run over the corpus stores.
fn sole_entry(cache: &std::path::Path) -> std::path::PathBuf {
    let mut found = Vec::new();
    for shard in fs::read_dir(cache.join("similarity")).unwrap() {
        let shard = shard.unwrap().path();
        // The signing key is a file at this level, not a shard.
        if shard.is_dir() {
            for entry in fs::read_dir(&shard).unwrap() {
                found.push(entry.unwrap().path());
            }
        }
    }
    assert_eq!(found.len(), 1, "the run must have stored exactly one entry");
    found.pop().unwrap()
}

/// Rewrites a stored answer, leaving its authenticator exactly as written.
///
/// This is what an attacker with write access to the cache directory can
/// actually do: they hold the entry but not the secret that signed it, and
/// re-signing is what they cannot do.
fn forge_entry(path: &std::path::Path, edit: impl FnOnce(&mut Value)) {
    let stored = fs::read_to_string(path).unwrap();
    let (tag, payload) = stored
        .split_once('\n')
        .expect("an entry is tag then answer");
    let mut entry: Value = serde_json::from_str(payload).unwrap();
    edit(&mut entry);
    fs::write(
        path,
        format!("{tag}\n{}", serde_json::to_string(&entry).unwrap()),
    )
    .unwrap();
}

/// A corpus of two files that are one duplicate pair.
fn duplicate_corpus(label: &str) -> std::path::PathBuf {
    let dir = fresh_temp_dir(label);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.lisp"), "(foo a b)\n").unwrap();
    fs::write(dir.join("b.lisp"), "(foo x y)\n").unwrap();
    dir
}

/// The attack the authentication exists for.
///
/// A `--cache-dir` restored from a shared CI artifact is attacker-controlled
/// input. Before entries carried a tag, zeroing the stored counts turned a
/// failing `--fail-on-duplicates` into a passing one — a CI gate defeated by
/// editing a JSON file. The forged entry no longer verifies, so it costs a
/// recompute and the gate still fails.
#[test]
fn cli_similarity_a_forged_entry_cannot_pass_the_duplicate_gate() {
    let dir = duplicate_corpus("similarity-cache-gate-forgery");
    let cache = fresh_temp_dir("similarity-cache-gate-forgery-store");
    let gate = || {
        paredit()
            .args([
                "inspect",
                "similarity",
                "--threshold",
                "0.8",
                "--fail-on-duplicates",
            ])
            .arg("--cache-dir")
            .arg(&cache)
            .arg(&dir)
            .output()
            .unwrap()
    };

    assert!(!gate().status.success(), "the corpus is a duplicate pair");
    let warm = gate();
    assert!(!warm.status.success());
    assert!(
        String::from_utf8_lossy(&warm.stdout).contains("\"cache\": \"hit\""),
        "the second run must be served from the cache, or the forgery below \
         would be testing nothing"
    );

    forge_entry(&sole_entry(&cache), |entry| {
        entry["summary"]["matched_pairs"] = json!(0);
        entry["summary"]["suppressed_pairs"] = json!(0);
        entry["summary"]["reported_pairs"] = json!(0);
        entry["pairs"] = json!([]);
    });

    let forged = gate();
    assert!(
        !forged.status.success(),
        "a forged entry must not turn a failing gate into a passing one"
    );
    let report: Value = serde_json::from_slice(&forged.stdout).unwrap();
    assert_eq!(
        report["cache"], "missing",
        "the forgery must read as a miss"
    );
    assert_eq!(report["pair_count"], 1);
}

/// The other half of the same attack: a pair's `text` is printed verbatim and
/// attributed to a real source path, so an accepted forgery puts
/// attacker-chosen content into a report about code the attacker never touched.
#[test]
fn cli_similarity_a_forged_entry_cannot_inject_report_text() {
    let dir = duplicate_corpus("similarity-cache-text-forgery");
    let cache = fresh_temp_dir("similarity-cache-text-forgery-store");
    let fabricated = "(defun TOTALLY-FABRICATED (evil) (delete-everything))";

    let honest = similarity_run(&dir, Some(&cache), &["--output", "json"]);
    assert_eq!(honest["cache"], "missing");
    assert_eq!(
        similarity_run(&dir, Some(&cache), &["--output", "json"])["cache"],
        "hit"
    );

    forge_entry(&sole_entry(&cache), |entry| {
        entry["pairs"][0]["left"]["text"] = json!(fabricated);
    });

    let recomputed = similarity_run(&dir, Some(&cache), &["--output", "json"]);
    assert_eq!(recomputed["cache"], "missing");
    assert!(
        !recomputed.to_string().contains("TOTALLY-FABRICATED"),
        "a forged entry must not put its own text into the report: {recomputed}"
    );
    assert_eq!(
        without_cache_field(recomputed),
        without_cache_field(honest),
        "the recomputed report must be the honest one"
    );
}

/// A cache directory written before entries were authenticated has no tags. It
/// is stale rather than trusted: one extra recompute, then caching resumes.
#[test]
fn cli_similarity_an_unauthenticated_entry_is_stale() {
    let dir = similarity_corpus("similarity-cache-legacy");
    let cache = fresh_temp_dir("similarity-cache-legacy-store");

    let fresh = similarity_run(&dir, Some(&cache), &[]);
    assert_eq!(fresh["cache"], "missing");

    // Strip the tag line, leaving exactly what the previous format wrote.
    let entry = sole_entry(&cache);
    let stored = fs::read_to_string(&entry).unwrap();
    let (_, payload) = stored.split_once('\n').unwrap();
    fs::write(&entry, payload).unwrap();

    let recomputed = similarity_run(&dir, Some(&cache), &[]);
    assert_eq!(recomputed["cache"], "missing");
    assert_eq!(
        without_cache_field(recomputed),
        without_cache_field(fresh),
        "an unauthenticated entry must be recomputed, not served"
    );
    assert_eq!(
        similarity_run(&dir, Some(&cache), &[])["cache"],
        "hit",
        "caching must resume after the one stale round"
    );
}
