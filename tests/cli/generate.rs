use super::*;

#[test]
fn cli_generate_defpackage_plans_without_writing() {
    let dir = fresh_temp_dir("generate-defpackage-plan");
    let file = dir.join("app.lisp");
    fs::write(
        &file,
        "(defun render (x) (alexandria:flatten x))\n(defun %helper (x) x)\n",
    )
    .expect("write fixture");

    let mut cmd = paredit();
    cmd.args(["generate", "defpackage"])
        .arg("--file")
        .arg(&file)
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"written\": false"))
        .stdout(predicate::str::contains("\"render\""))
        .stdout(predicate::str::contains("\"alexandria\""));

    assert_eq!(
        fs::read_to_string(&file).expect("fixture unchanged"),
        "(defun render (x) (alexandria:flatten x))\n(defun %helper (x) x)\n",
        "a plan without --write must not touch the file"
    );
}

#[test]
fn cli_generate_defpackage_writes_a_parseable_file() {
    let dir = fresh_temp_dir("generate-defpackage-write");
    let file = dir.join("app.lisp");
    fs::write(&file, "(defun render (x) x)\n").expect("write fixture");

    let mut cmd = paredit();
    cmd.args(["generate", "defpackage"])
        .arg("--file")
        .arg(&file)
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"written\": true"));

    let rewritten = fs::read_to_string(&file).expect("read rewritten file");
    assert!(rewritten.starts_with("(defpackage :app"), "{rewritten}");
    assert!(rewritten.contains("(defun render (x) x)"), "{rewritten}");

    paredit()
        .args(["inspect", "check"])
        .arg("--file")
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn cli_generate_defpackage_diff_leaves_the_file_untouched() {
    let dir = fresh_temp_dir("generate-defpackage-diff");
    let file = dir.join("app.lisp");
    fs::write(&file, "(defun render (x) x)\n").expect("write fixture");

    paredit()
        .args(["generate", "defpackage"])
        .arg("--file")
        .arg(&file)
        .arg("--diff")
        .assert()
        .success()
        .stdout(predicate::str::contains("defpackage"));

    assert_eq!(
        fs::read_to_string(&file).expect("fixture unchanged"),
        "(defun render (x) x)\n"
    );
}

#[test]
fn cli_generate_defpackage_refuses_a_non_common_lisp_file() {
    let dir = fresh_temp_dir("generate-defpackage-dialect");
    let file = dir.join("app.el");
    fs::write(&file, "(defun render (x) x)\n").expect("write fixture");

    paredit()
        .args(["generate", "defpackage"])
        .arg("--file")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Common Lisp"));
}

#[test]
fn cli_generate_defsystem_writes_a_parseable_asd_file() {
    let dir = fresh_temp_dir("generate-defsystem");
    fs::write(
        dir.join("a.lisp"),
        "(defun f () (alexandria:flatten '(1)))\n",
    )
    .expect("write a.lisp");
    fs::write(dir.join("b.lisp"), "(defun g () 2)\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.args(["generate", "defsystem"])
        .arg(&dir)
        .arg("--name")
        .arg("app")
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"written\": true"))
        .stdout(predicate::str::contains("\"alexandria\""));

    let asd_path = dir.join("app.asd");
    let contents = fs::read_to_string(&asd_path).expect("read generated .asd");
    assert!(
        contents.starts_with("(asdf:defsystem \"app\""),
        "{contents}"
    );
    assert!(contents.contains("(:file \"a\")"), "{contents}");
    assert!(contents.contains("(:file \"b\")"), "{contents}");

    paredit()
        .args(["inspect", "check"])
        .arg("--file")
        .arg(&asd_path)
        .assert()
        .success();
}

#[test]
fn cli_generate_defsystem_refuses_to_overwrite_without_force() {
    let dir = fresh_temp_dir("generate-defsystem-no-force");
    fs::write(dir.join("a.lisp"), "(defun f () 1)\n").expect("write a.lisp");
    fs::write(
        dir.join("app.asd"),
        "(asdf:defsystem \"app\" :components ())\n",
    )
    .expect("write pre-existing .asd");

    paredit()
        .args(["generate", "defsystem"])
        .arg(&dir)
        .arg("--name")
        .arg("app")
        .arg("--write")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--force"));
}

#[test]
fn cli_generate_tests_writes_stubs_for_untested_definitions() {
    let dir = fresh_temp_dir("generate-tests");
    let source = dir.join("app.lisp");
    let target = dir.join("app-tests.lisp");
    fs::write(&source, "(defun render (x) x)\n").expect("write fixture");

    let mut cmd = paredit();
    cmd.args(["generate", "tests"])
        .arg(&source)
        .arg("--into")
        .arg(&target)
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"written\": true"))
        .stdout(predicate::str::contains("\"subject\": \"render\""));

    let contents = fs::read_to_string(&target).expect("read generated test file");
    assert!(contents.contains("(deftest test-render ()"), "{contents}");

    paredit()
        .args(["inspect", "check"])
        .arg("--file")
        .arg(&target)
        .assert()
        .success();
}

#[test]
fn cli_generate_tests_requires_into_with_write() {
    let dir = fresh_temp_dir("generate-tests-requires-into");
    let source = dir.join("app.lisp");
    fs::write(&source, "(defun render (x) x)\n").expect("write fixture");

    paredit()
        .args(["generate", "tests"])
        .arg(&source)
        .arg("--write")
        .assert()
        .failure();
}

#[test]
fn cli_generate_accessors_writes_accessors_for_bare_slots() {
    let dir = fresh_temp_dir("generate-accessors");
    let file = dir.join("point.lisp");
    fs::write(&file, "(defclass point ()\n  (x\n   (y :initform 0)))\n").expect("write fixture");

    let mut cmd = paredit();
    cmd.args(["generate", "accessors"])
        .arg("--file")
        .arg(&file)
        .arg("--select")
        .arg("name:point")
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"written\": true"))
        .stdout(predicate::str::contains("\"status\": \"ready\""));

    let rewritten = fs::read_to_string(&file).expect("read rewritten file");
    assert!(rewritten.contains(":accessor point-x"), "{rewritten}");
    assert!(rewritten.contains(":accessor point-y"), "{rewritten}");

    paredit()
        .args(["inspect", "check"])
        .arg("--file")
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn cli_generate_accessors_reports_nothing_to_do_when_all_slots_have_one() {
    let dir = fresh_temp_dir("generate-accessors-nothing");
    let file = dir.join("point.lisp");
    fs::write(&file, "(defclass point () ((x :accessor point-x)))\n").expect("write fixture");

    paredit()
        .args(["generate", "accessors"])
        .arg("--file")
        .arg(&file)
        .arg("--select")
        .arg("name:point")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"nothing-to-do\""));
}

#[test]
fn cli_generate_defgeneric_writes_a_declaration_for_an_undeclared_method() {
    let dir = fresh_temp_dir("generate-defgeneric");
    let file = dir.join("app.lisp");
    fs::write(&file, "(defmethod speak ((x fish)) 1)\n").expect("write fixture");

    let mut cmd = paredit();
    cmd.args(["generate", "defgeneric"])
        .arg("--file")
        .arg(&file)
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"written\": true"))
        .stdout(predicate::str::contains("\"status\": \"ready\""));

    let rewritten = fs::read_to_string(&file).expect("read rewritten file");
    assert!(
        rewritten.contains("(defgeneric speak (arg1))"),
        "{rewritten}"
    );

    paredit()
        .args(["inspect", "check"])
        .arg("--file")
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn cli_generate_defgeneric_refuses_a_non_common_lisp_file() {
    let dir = fresh_temp_dir("generate-defgeneric-dialect");
    let file = dir.join("app.el");
    fs::write(&file, "(defun render (x) x)\n").expect("write fixture");

    paredit()
        .args(["generate", "defgeneric"])
        .arg("--file")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Common Lisp"));
}

#[test]
fn cli_generate_docstring_inserts_a_template_after_the_lambda_list() {
    let dir = fresh_temp_dir("generate-docstring");
    let file = dir.join("app.lisp");
    fs::write(&file, "(defun render (x) (+ x 1))\n").expect("write fixture");

    let mut cmd = paredit();
    cmd.args(["generate", "docstring"])
        .arg("--file")
        .arg(&file)
        .arg("--select")
        .arg("name:render")
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"written\": true"))
        .stdout(predicate::str::contains("\"status\": \"ready\""));

    let rewritten = fs::read_to_string(&file).expect("read rewritten file");
    assert!(
        rewritten.contains("\"TODO: document render. Parameters: x.\""),
        "{rewritten}"
    );
    assert!(rewritten.contains("(+ x 1)"), "{rewritten}");

    paredit()
        .args(["inspect", "check"])
        .arg("--file")
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn cli_generate_docstring_reports_already_documented_without_double_documenting() {
    let dir = fresh_temp_dir("generate-docstring-already");
    let file = dir.join("app.lisp");
    fs::write(&file, "(defun render (x) \"Already documented.\" x)\n").expect("write fixture");

    paredit()
        .args(["generate", "docstring"])
        .arg("--file")
        .arg(&file)
        .arg("--select")
        .arg("name:render")
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"status\": \"already-documented\"",
        ))
        .stdout(predicate::str::contains("\"written\": false"));

    assert_eq!(
        fs::read_to_string(&file).expect("fixture unchanged"),
        "(defun render (x) \"Already documented.\" x)\n"
    );
}

#[test]
fn cli_generate_docstring_refuses_a_non_common_lisp_file() {
    let dir = fresh_temp_dir("generate-docstring-dialect");
    let file = dir.join("app.el");
    fs::write(&file, "(defun render (x) x)\n").expect("write fixture");

    paredit()
        .args(["generate", "docstring"])
        .arg("--file")
        .arg(&file)
        .arg("--select")
        .arg("name:render")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Common Lisp"));
}
