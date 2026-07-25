use super::*;

#[test]
fn cli_reports_workspace_inventory_from_directory_roots() {
    let dir = fresh_temp_dir("workspace report");
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).expect("create source dir");

    let lisp_file = src_dir.join("core.lisp");
    let elisp_file = src_dir.join("init.el");
    let scheme_file = src_dir.join("broken.scm");
    let unknown_file = src_dir.join("notes.txt");
    fs::write(
        &lisp_file,
        "(in-package #:demo)\n(defun area (width height) (* width height))\n",
    )
    .expect("write common lisp fixture");
    fs::write(&elisp_file, "(defun draw () (area 5 6))\n").expect("write elisp fixture");
    fs::write(&scheme_file, "(define (broken x)").expect("write scheme fixture");
    fs::write(&unknown_file, "not lisp").expect("write unknown fixture");

    let mut cmd = paredit();
    cmd.args(["inspect", "workspace"])
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file_count\": 3"))
        .stdout(predicate::str::contains("\"parsed_count\": 2"))
        .stdout(predicate::str::contains("\"parse_error_count\": 1"))
        .stdout(predicate::str::contains("\"definition_count\": 2"))
        .stdout(predicate::str::contains("\"max_complexity_score\":"))
        .stdout(predicate::str::contains("\"unknown\": 1"))
        .stdout(predicate::str::contains("\"dialect\": \"common-lisp\""))
        .stdout(predicate::str::contains("\"dialect\": \"emacs-lisp\""))
        .stdout(predicate::str::contains("\"dialect\": \"scheme\""))
        .stdout(predicate::str::contains("\"status\": \"parse-error\""));
}

#[test]
fn cli_reports_workspace_inventory_with_include_flags() {
    let dir = fresh_temp_dir("workspace report-include-flags");
    let hidden_dir = dir.join(".hidden");
    let generated_dir = dir.join("target");
    fs::create_dir_all(&hidden_dir).expect("create hidden dir");
    fs::create_dir_all(&generated_dir).expect("create generated dir");

    let lisp_file = dir.join("core.lisp");
    let hidden_file = hidden_dir.join("secret.el");
    let generated_file = generated_dir.join("generated.scm");
    let unknown_file = dir.join("notes.txt");
    fs::write(&lisp_file, "(defun area (width height) (* width height))\n")
        .expect("write common lisp fixture");
    fs::write(&hidden_file, "(defun secret () (message \"hidden\"))\n")
        .expect("write hidden fixture");
    fs::write(&generated_file, "(define generated #t)\n").expect("write generated fixture");
    fs::write(&unknown_file, "plain-text-note\n").expect("write unknown fixture");

    let mut cmd = paredit();
    cmd.args(["inspect", "workspace"])
        .arg("--include-hidden")
        .arg("--include-generated")
        .arg("--include-unknown")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file_count\": 4"))
        .stdout(predicate::str::contains("\"parsed_count\": 4"))
        .stdout(predicate::str::contains("\"parse_error_count\": 0"))
        .stdout(predicate::str::contains("\"hidden\": 0"))
        .stdout(predicate::str::contains("\"generated\": 0"))
        .stdout(predicate::str::contains("\"unknown\": 0"))
        .stdout(predicate::str::contains("\"dialect\": \"common-lisp\""))
        .stdout(predicate::str::contains("\"dialect\": \"emacs-lisp\""))
        .stdout(predicate::str::contains("\"dialect\": \"scheme\""))
        .stdout(predicate::str::contains("\"dialect\": \"unknown\""));
}

#[test]
fn cli_applies_detected_reader_policy_and_preserves_unknown_parsing() {
    let dir = fresh_temp_dir("workspace report-reader-policy");
    let common_lisp_file = dir.join("common.lisp");
    let emacs_lisp_file = dir.join("emacs.el");
    let invalid_common_lisp_file = dir.join("invalid.lisp");
    let unknown_file = dir.join("generic.txt");
    fs::write(&common_lisp_file, "(defun cl-char () #\\))\n").expect("write common lisp fixture");
    fs::write(&emacs_lisp_file, "(defun el-char () ?\\))\n").expect("write emacs lisp fixture");
    fs::write(&invalid_common_lisp_file, "#?value\n").expect("write invalid common lisp fixture");
    fs::write(&unknown_file, "#?value\n").expect("write unknown fixture");

    let mut cmd = paredit();
    cmd.args(["inspect", "workspace"])
        .arg("--include-unknown")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file_count\": 4"))
        .stdout(predicate::str::contains("\"parsed_count\": 3"))
        .stdout(predicate::str::contains("\"parse_error_count\": 1"))
        .stdout(predicate::str::contains("\"definition_count\": 2"))
        .stdout(predicate::str::contains(
            invalid_common_lisp_file.display().to_string(),
        ))
        .stdout(predicate::str::contains(unknown_file.display().to_string()))
        .stdout(predicate::str::contains("\"dialect\": \"unknown\""));
}

#[test]
fn cli_reports_workspace_inventory_for_binary_unknown_files() {
    let dir = fresh_temp_dir("workspace report-binary");

    let lisp_file = dir.join("core.lisp");
    let binary_file = dir.join("compiled.fasl");
    fs::write(&lisp_file, "(defun area (width height) (* width height))\n")
        .expect("write common lisp fixture");
    fs::write(&binary_file, [0xff, 0xfe, 0x00]).expect("write binary fixture");

    let mut cmd = paredit();
    cmd.args(["inspect", "workspace"])
        .arg("--include-unknown")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file_count\": 2"))
        .stdout(predicate::str::contains("\"parsed_count\": 1"))
        .stdout(predicate::str::contains("\"parse_error_count\": 1"))
        .stdout(predicate::str::contains(binary_file.display().to_string()))
        .stdout(predicate::str::contains("\"status\": \"parse-error\""));
}

#[test]
fn cli_reports_workspace_inventory_with_max_depth_limit() {
    let dir = fresh_temp_dir("workspace report-max-depth");
    let nested_dir = dir.join("nested");
    fs::create_dir_all(&nested_dir).expect("create nested dir");

    let root_file = dir.join("root.lisp");
    let nested_file = nested_dir.join("deep.lisp");
    fs::write(&root_file, "(defun root () t)\n").expect("write root fixture");
    fs::write(&nested_file, "(defun deep () t)\n").expect("write nested fixture");

    let mut cmd = paredit();
    cmd.args(["inspect", "workspace"])
        .arg("--max-depth")
        .arg("1")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file_count\": 1"))
        .stdout(predicate::str::contains("\"parsed_count\": 1"))
        .stdout(predicate::str::contains("\"parse_error_count\": 0"))
        .stdout(predicate::str::contains(root_file.display().to_string()))
        .stdout(predicate::str::contains(nested_file.display().to_string()).not());
}

#[test]
fn cli_workspace_max_complexity_score_tracks_the_deepest_definition() {
    let dir = fresh_temp_dir("workspace report-complexity");
    let shallow_file = dir.join("shallow.lisp");
    let deep_file = dir.join("deep.lisp");
    fs::write(&shallow_file, "(defun shallow (x) (+ x 1))\n").expect("write shallow fixture");
    fs::write(
        &deep_file,
        "(defun deep (x)\n\
           (if x\n\
             (let ((y (+ x 1)))\n\
               (if y (+ y 1) y))\n\
             x))\n",
    )
    .expect("write deep fixture");

    let mut cmd = paredit();
    let output = cmd
        .args(["inspect", "workspace"])
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid json");

    let files = report["files"].as_array().expect("files array");
    let shallow_score = files
        .iter()
        .find(|file| file["path"] == shallow_file.display().to_string())
        .expect("shallow file report")["max_complexity_score"]
        .as_u64()
        .expect("shallow max_complexity_score");
    let deep_score = files
        .iter()
        .find(|file| file["path"] == deep_file.display().to_string())
        .expect("deep file report")["max_complexity_score"]
        .as_u64()
        .expect("deep max_complexity_score");

    assert!(deep_score > shallow_score);
    assert_eq!(report["max_complexity_score"], deep_score);
}
