use super::*;

/// A file with one of everything the report knows how to name.
const DEMO: &str = ";;; demo.el --- a demo -*- lexical-binding: t -*-\n\
                    (require 'subr-x)\n\
                    (autoload 'other-thing \"other-lib\")\n\
                    \n\
                    ;;;###autoload\n\
                    (defun demo-command ()\n\
                      \"Do the thing.\"\n\
                      (interactive)\n\
                      (message \"hi\"))\n\
                    \n\
                    (provide 'demo)\n";

fn write_demo(name: &str, contents: &str) -> std::path::PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("demo.el");
    fs::write(&file, contents).expect("write demo.el");
    file
}

#[test]
fn cli_reports_the_header_features_and_autoloads_of_one_file() {
    let file = write_demo("elisp-file-report", DEMO);

    paredit()
        .args(["inspect", "elisp-file", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"lexical_binding\": \"enabled\""))
        .stdout(predicate::str::contains("\"provides\": \"demo\""))
        .stdout(predicate::str::contains("\"designator\": \"subr-x\""))
        .stdout(predicate::str::contains("\"definition\": \"demo-command\""));
}

#[test]
fn cli_separates_an_eager_require_from_a_deferred_autoload() {
    let file = write_demo("elisp-file-report-eager", DEMO);

    let stdout = paredit()
        .args(["inspect", "elisp-file", "--output", "text"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(stdout).expect("stdout is UTF-8");

    // `require` loads the library now; `autoload` defers it until the
    // function is called. A dependency report that conflated them would say
    // this file loads `other-lib` at startup, which is the opposite of why
    // the `autoload` is there.
    assert!(text.contains("require\tsubr-x\teager=true"), "{text}");
    assert!(text.contains("autoload\tother-lib\teager=false"), "{text}");
}

#[test]
fn cli_gate_fails_for_a_file_without_a_lexical_binding_header() {
    let file = write_demo(
        "elisp-file-report-gate",
        ";;; demo.el --- a demo\n(provide 'demo)\n",
    );

    paredit()
        .args([
            "inspect",
            "elisp-file",
            "--fail-on-missing-lexical-binding",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"lexical_binding\": \"absent\""))
        .stdout(predicate::str::contains("\"passed\": false"));
}

#[test]
fn cli_gate_passes_when_every_file_declares_its_binding() {
    let file = write_demo("elisp-file-report-gate-ok", DEMO);

    paredit()
        .args([
            "inspect",
            "elisp-file",
            "--fail-on-missing-lexical-binding",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"passed\": true"));
}

#[test]
fn cli_invents_no_emacs_lisp_facts_for_another_dialect() {
    // The dialect override is what a user reaches for when a file has an
    // unusual extension; pointing it at Common Lisp must not produce a
    // `lexical-binding` answer for a dialect that has no such concept.
    let file = write_demo("elisp-file-report-dialect", DEMO);

    paredit()
        .args([
            "inspect",
            "elisp-file",
            "--dialect",
            "common-lisp",
            "--output",
            "json",
        ])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"lexical_binding\": \"absent\""))
        .stdout(predicate::str::contains("\"definition_count\": 0"));
}
