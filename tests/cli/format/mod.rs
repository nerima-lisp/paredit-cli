use super::*;
use std::path::Path;

mod binding_forms;
mod body_forms;
mod comment_alignment_and_blank_lines;
mod config_and_multi_file;
mod declaration_forms;
mod definition_forms;
mod indent_table_and_quote_style;

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

#[test]
fn cli_format_max_width_moves_the_definition_header_threshold() {
    // The whole-form fit test behind a `defsystem` header was still reading
    // the compiled-in 80-column constant, so `--max-width` moved every other
    // width decision in the formatter but not this one.
    let inline =
        "(defsystem \"foo\" :description \"short\" :version \"0.1.0\" :depends-on (:asdf))\n";
    let broken = "(defsystem \"foo\"\n  :description \"short\"\n  :version \"0.1.0\"\n  :depends-on (:asdf))\n";

    let dir = fresh_temp_dir("format-max-width-defsystem");
    let file = dir.join(Path::new("system.asd"));
    fs::write(&file, inline).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::eq(inline));

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--max-width")
        .arg("40")
        .assert()
        .success()
        .stdout(predicate::eq(broken));

    // And the other direction: a header past the default fits once widened.
    let long = "(defsystem \"foo\" :description \"a description that pushes this past eighty columns\" :version \"0.1.0\")\n";
    let long_file = dir.join(Path::new("long.asd"));
    fs::write(&long_file, long).expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&long_file)
        .assert()
        .success()
        .stdout(predicate::eq(long).not());

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&long_file)
        .arg("--max-width")
        .arg("120")
        .assert()
        .success()
        .stdout(predicate::eq(long));
}

#[test]
fn cli_format_diff_stat_reports_hunks_and_changed_lines() {
    let dir = fresh_temp_dir("format-diff-stat");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(defun foo   ()   1)\n").expect("write unformatted fixture");

    let output = paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--diff-stat")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value =
        serde_json::from_slice(&output).expect("--diff-stat emits valid JSON");
    assert_eq!(report["changed"], true);
    assert_eq!(report["hunks"], 1);
    assert!(report["added_lines"].as_u64().expect("added_lines") > 0);
    assert!(report["removed_lines"].as_u64().expect("removed_lines") > 0);
    // --diff-stat never writes.
    assert_eq!(
        fs::read_to_string(&file).expect("read fixture"),
        "(defun foo   ()   1)\n"
    );
}

#[test]
fn cli_format_diff_stat_reports_no_changes_for_a_canonical_file() {
    let dir = fresh_temp_dir("format-diff-stat-clean");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(defun foo ()\n  1)\n").expect("write canonical fixture");

    let output = paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--diff-stat")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value =
        serde_json::from_slice(&output).expect("--diff-stat emits valid JSON");
    assert_eq!(report["changed"], false);
    assert_eq!(report["hunks"], 0);
    assert_eq!(report["added_lines"], 0);
    assert_eq!(report["removed_lines"], 0);
}

#[test]
fn cli_format_diff_stat_rejects_write_and_diff_together() {
    let dir = fresh_temp_dir("format-diff-stat-conflicts");
    let file = dir.join(Path::new("source.lisp"));
    fs::write(&file, "(foo)\n").expect("write fixture");

    paredit()
        .arg("edit")
        .arg("format")
        .arg("--file")
        .arg(&file)
        .arg("--diff-stat")
        .arg("--diff")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}
