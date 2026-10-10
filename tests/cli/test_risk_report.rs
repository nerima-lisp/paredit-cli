use super::*;

fn write_fixture(dir: &std::path::Path, name: &str, source: &str) -> PathBuf {
    let file = dir.join(name);
    fs::write(&file, source).unwrap();
    file
}

fn json_report(files: &[&PathBuf]) -> serde_json::Value {
    let output = paredit()
        .args(["inspect", "test-risk", "--output", "json"])
        .args(files)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn test_risk_json_ranks_unlinked_functions_and_excludes_linked_control() {
    let dir = fresh_temp_dir("test-risk-json");
    let source = "(defun linked () (list (list (list 1))))\n(deftest test-linked () 1)\n(defun easy () 1)\n(defun deep () (list (list 1)))\n(defmacro ignored () 1)\n";
    let file = write_fixture(&dir, "app.lisp", source);
    let json = json_report(&[&file]);
    assert_eq!(json["command"], "inspect test-risk");
    assert_eq!(json["coverage_basis"], "static-test-name-linkage");
    assert_eq!(json["analysis_complete"], true);
    assert_eq!(json["modelled_function_count"], 3);
    assert_eq!(json["linked_function_count"], 1);
    assert_eq!(json["unlinked_function_count"], 2);
    assert_eq!(json["ranked"][0]["name"], "DEEP");
    assert_eq!(json["ranked"][1]["name"], "EASY");
    assert!(
        json["ranked"][0]["complexity_score"].as_u64().unwrap()
            > json["ranked"][1]["complexity_score"].as_u64().unwrap()
    );
    assert_eq!(json["ranked"][0]["line"], 4);
    let start = json["ranked"][0]["span"]["start"].as_u64().unwrap() as usize;
    let end = json["ranked"][0]["span"]["end"].as_u64().unwrap() as usize;
    assert_eq!(&source[start..end], "(defun deep () (list (list 1)))");
    assert_eq!(fs::read_to_string(file).unwrap(), source);
}

#[test]
fn test_risk_package_aware_cross_file_linkage_and_input_order_invariance() {
    let dir = fresh_temp_dir("test-risk-packages");
    let app = write_fixture(&dir, "app.lisp", "(in-package :app) (defun work () 1)");
    let other = write_fixture(&dir, "other.lisp", "(in-package :other) (defun work () 1)");
    let tests = write_fixture(
        &dir,
        "tests.lisp",
        "(in-package :app) (deftest test-work () 1)",
    );
    let json = json_report(&[&app, &other, &tests]);
    assert_eq!(json["linked_function_count"], 1);
    assert_eq!(json["unlinked_function_count"], 1);
    assert_eq!(json["ranked"][0]["file"], other.to_string_lossy().as_ref());
    let forward = paredit()
        .args(["inspect", "test-risk"])
        .args([&app, &other, &tests])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let reverse = paredit()
        .args(["inspect", "test-risk"])
        .args([&tests, &other, &app])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(forward, reverse);
    let directory = json_report(&[&dir]);
    assert_eq!(directory, json);
}

#[test]
fn test_risk_ambiguous_names_remain_unlinked() {
    let dir = fresh_temp_dir("test-risk-ambiguous");
    write_fixture(&dir, "a.lisp", "(in-package :app) (defun work () 1)");
    write_fixture(&dir, "b.lisp", "(in-package :app) (defun work () 1)");
    write_fixture(
        &dir,
        "tests.lisp",
        "(in-package :app) (deftest test-work () 1)",
    );
    let json = json_report(&[&dir]);
    assert_eq!(json["linked_function_count"], 0);
    assert_eq!(json["unlinked_function_count"], 2);
    assert_eq!(json["ranked"].as_array().unwrap().len(), 2);
    assert!(
        json["ranked"][0]["file"].as_str().unwrap() < json["ranked"][1]["file"].as_str().unwrap()
    );
}

#[test]
fn test_risk_mixed_dialects_label_unmodelled_files_with_unknown_counts() {
    let dir = fresh_temp_dir("test-risk-dialects");
    let cl = write_fixture(&dir, "a.lisp", "(defun work () 1)");
    let el = write_fixture(
        &dir,
        "b.el",
        "(defun work () 1) (ert-deftest test-work () 1)",
    );
    let scheme = write_fixture(&dir, "c.scm", "(define (work) 1)");
    let json = json_report(&[&cl, &el, &scheme]);
    assert_eq!(json["analysis_complete"], false);
    assert_eq!(json["file_count"], 3);
    assert_eq!(json["modelled_function_count"], 2);
    assert_eq!(json["linked_function_count"], 1);
    assert_eq!(json["unlinked_function_count"], 1);
    assert_eq!(json["files"][2]["modelled"], false);
    assert!(json["files"][2]["function_count"].is_null());
    assert!(json["files"][2]["linked_function_count"].is_null());
    assert!(json["files"][2]["unmodelled_reason"].is_string());
}

#[test]
fn test_risk_top_limits_display_but_not_gate_and_accepts_large_bound() {
    let dir = fresh_temp_dir("test-risk-gate");
    let file = write_fixture(&dir, "a.lisp", "(defun a () 1) (defun b () 1)");
    for top in ["0", "1"] {
        let output = paredit()
            .args(["inspect", "test-risk", "--fail-on-untested", "--top", top])
            .arg(&file)
            .assert()
            .code(3)
            .get_output()
            .stdout
            .clone();
        let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(json["unlinked_function_count"], 2);
        assert_eq!(json["policy"]["finding_count"], 2);
        assert_eq!(json["policy"]["passed"], false);
        assert_eq!(json["policy"]["scope"], "modelled-functions");
        assert_eq!(
            json["ranked"].as_array().unwrap().len(),
            top.parse::<usize>().unwrap()
        );
    }
    let maximum = usize::MAX.to_string();
    let output = paredit()
        .args(["inspect", "test-risk", "--top", &maximum])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["displayed_count"], 2);
}

#[test]
fn test_risk_linked_control_passes_gate() {
    let dir = fresh_temp_dir("test-risk-linked-gate");
    let file = write_fixture(&dir, "a.lisp", "(defun a () 1) (deftest test-a () 1)");
    let output = paredit()
        .args(["inspect", "test-risk", "--fail-on-untested"])
        .arg(file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["linked_function_count"], 1);
    assert_eq!(json["ranked"], serde_json::json!([]));
    assert_eq!(json["policy"]["passed"], true);
}

#[test]
fn test_risk_text_identifies_heuristic_and_ranking() {
    let dir = fresh_temp_dir("test-risk-text");
    let file = write_fixture(&dir, "a.lisp", "(defun work () 1)");
    let output = paredit()
        .args(["inspect", "test-risk", "--output", "text"])
        .arg(file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines[0], "coverage_basis\tstatic-test-name-linkage");
    assert!(lines[1].contains("not execution coverage or safety"));
    assert_eq!(lines[3], "modelled_functions\t1\tlinked=0\tunlinked=1");
    let rank = lines
        .iter()
        .find(|line| line.starts_with("rank\t"))
        .unwrap()
        .split('\t')
        .collect::<Vec<_>>();
    assert_eq!(rank[1], "1");
    assert_eq!(rank[4], "WORK");
    assert_eq!(rank[5], "line=1");
}

#[test]
fn test_risk_input_budget_and_input_errors_fail_without_report_or_writes() {
    let dir = fresh_temp_dir("test-risk-input-errors");
    let source = "(defun work () 1)";
    let file = write_fixture(&dir, "a.lisp", source);
    paredit()
        .args(["--max-input-bytes", "4", "inspect", "test-risk"])
        .arg(&file)
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("input exceeds 4 bytes"));
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
    paredit()
        .args(["inspect", "test-risk"])
        .arg(dir.join("missing.lisp"))
        .assert()
        .failure()
        .stdout("");
    let malformed = write_fixture(&dir, "malformed.lisp", "(defun bad ()");
    paredit()
        .args(["inspect", "test-risk"])
        .arg(malformed)
        .assert()
        .failure()
        .stdout("");
    let invalid = dir.join("invalid.lisp");
    fs::write(&invalid, [0xff]).unwrap();
    paredit()
        .args(["inspect", "test-risk"])
        .arg(invalid)
        .assert()
        .failure()
        .stdout("");
}
