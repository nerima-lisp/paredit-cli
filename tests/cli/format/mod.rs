use super::*;
use std::path::Path;

mod binding_forms;
mod body_forms;
mod declaration_forms;
mod definition_forms;

fn assert_format_output(fixture_name: &str, file_name: &str, input: &str, expected: &str) {
    let dir = fresh_temp_dir(fixture_name);
    let file = dir.join(Path::new(file_name));
    fs::write(&file, input).expect("write source fixture");

    let mut cmd = paredit();
    cmd.arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
}

#[test]
fn cli_formats_janet_hash_comment_without_changing_output() {
    let input = "# keep this comment\n(foo)\n";
    let dir = fresh_temp_dir("format-janet-hash-comment");
    let file = dir.join(Path::new("source.janet"));
    fs::write(&file, input).expect("write source fixture");

    let mut cmd = paredit();
    cmd.arg("edit")
        .arg("format")
        .arg("--dialect")
        .arg("janet")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(input));
}

#[test]
fn cli_format_check_passes_a_canonically_formatted_file() {
    let dir = fresh_temp_dir("format-check-clean");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(defun foo ()\n  1)\n").expect("write canonical fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--check")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    // --check never writes.
    assert_eq!(
        fs::read_to_string(&file).expect("read fixture"),
        "(defun foo ()\n  1)\n"
    );
}

#[test]
fn cli_format_check_fails_a_file_that_needs_reformatting() {
    let dir = fresh_temp_dir("format-check-dirty");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(defun foo   ()   1)\n").expect("write unformatted fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not formatted"));
    // --check never writes, even on failure.
    assert_eq!(
        fs::read_to_string(&file).expect("read fixture"),
        "(defun foo   ()   1)\n"
    );
}

#[test]
fn cli_format_check_rejects_write_and_diff_together() {
    let dir = fresh_temp_dir("format-check-conflicts");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(foo)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--check")
        .arg("--write")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn cli_format_max_width_widens_the_inline_fit_threshold() {
    let input = "(some-function-name argument-one argument-two argument-three argument-four argument-five)\n";
    let dir = fresh_temp_dir("format-max-width");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, input).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains('\n').and(predicate::str::diff(input).not()));

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--max-width")
        .arg("200")
        .assert()
        .success()
        .stdout(predicate::eq(input));
}
