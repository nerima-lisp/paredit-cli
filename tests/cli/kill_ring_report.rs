//! `inspect kill-ring`: diagnose the kill ring artifact without touching it,
//! except when `--repair-reset` is explicitly passed. See
//! `src/presentation/cli/kill_ring_report/domain.rs` for the unit-level
//! coverage of `classify` itself; this file exercises the command end to end.

use super::*;

#[test]
fn a_valid_ring_reports_its_entry_count_and_leaves_the_file_alone() {
    let dir = fresh_temp_dir("kill-ring-valid");
    let ring = dir.join("kill-ring.json");
    let before = serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": 1,
        "entries": [
            { "text": "(a)", "origin": null },
            { "text": "(b)", "origin": "f.lisp" },
        ],
    }))
    .expect("render fixture")
        + "\n";
    fs::write(&ring, &before).expect("write fixture ring");

    let output = paredit()
        .args(["inspect", "kill-ring", "--output", "json", "--ring"])
        .arg(&ring)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("kill-ring is JSON");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["entry_count"], 2);
    assert_eq!(report["repaired"], false);

    assert_eq!(
        fs::read_to_string(&ring).expect("read ring"),
        before,
        "a valid ring must not be rewritten"
    );
}

#[test]
fn a_missing_ring_file_is_valid_and_empty_not_corruption() {
    let dir = fresh_temp_dir("kill-ring-missing");
    let ring = dir.join("does-not-exist.json");

    let output = paredit()
        .args(["inspect", "kill-ring", "--output", "json", "--ring"])
        .arg(&ring)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("kill-ring is JSON");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["entry_count"], 0);
    assert!(
        !ring.exists(),
        "a plain check must not create the ring file"
    );
}

#[test]
fn malformed_json_without_repair_reset_is_reported_and_leaves_the_file_byte_identical() {
    let dir = fresh_temp_dir("kill-ring-malformed-no-repair");
    let ring = dir.join("kill-ring.json");
    let before = "{ this is not valid json";
    fs::write(&ring, before).expect("write malformed fixture");

    paredit()
        .args(["inspect", "kill-ring", "--output", "json", "--ring"])
        .arg(&ring)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("invalid-json"));

    assert_eq!(
        fs::read_to_string(&ring).expect("read ring"),
        before,
        "without --repair-reset the file must be untouched"
    );
}

#[test]
fn malformed_json_with_repair_reset_writes_a_fresh_empty_ring() {
    let dir = fresh_temp_dir("kill-ring-malformed-repair");
    let ring = dir.join("kill-ring.json");
    fs::write(&ring, "{ this is not valid json").expect("write malformed fixture");

    let output = paredit()
        .args([
            "inspect",
            "kill-ring",
            "--repair-reset",
            "--output",
            "json",
            "--ring",
        ])
        .arg(&ring)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("kill-ring is JSON");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["entry_count"], 0);
    assert_eq!(report["repaired"], true);

    let rewritten = fs::read_to_string(&ring).expect("read reset ring");
    let parsed: serde_json::Value =
        serde_json::from_str(&rewritten).expect("--repair-reset must leave the ring as valid JSON");
    assert_eq!(
        parsed,
        serde_json::json!({ "schema_version": 1, "entries": [] }),
        "must match write_ring's own empty-ring shape"
    );
}

#[test]
fn repair_reset_on_an_already_valid_ring_does_not_rewrite_it() {
    let dir = fresh_temp_dir("kill-ring-repair-on-valid");
    let ring = dir.join("kill-ring.json");
    let before = serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": 1,
        "entries": [{ "text": "(a)", "origin": null }],
    }))
    .expect("render fixture")
        + "\n";
    fs::write(&ring, &before).expect("write fixture ring");

    paredit()
        .args(["inspect", "kill-ring", "--repair-reset", "--ring"])
        .arg(&ring)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&ring).expect("read ring"),
        before,
        "--repair-reset must only ever act on a ring it has itself classified as corrupted"
    );
}

#[test]
fn a_schema_version_mismatch_is_reported_by_name() {
    let dir = fresh_temp_dir("kill-ring-schema-mismatch");
    let ring = dir.join("kill-ring.json");
    fs::write(&ring, r#"{"schema_version": 999, "entries": []}"#).expect("write fixture");

    let output = paredit()
        .args(["inspect", "kill-ring", "--output", "json", "--ring"])
        .arg(&ring)
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("kill-ring is JSON");
    assert_eq!(report["status"], "error");
    assert_eq!(report["corruption"]["code"], "schema-version-mismatch");
}
