//! `--encoding` with `--write`: the one combination that is refused.
//!
//! The tool decodes to UTF-8 on the way in and works in UTF-8 throughout, so
//! writing back out under a non-UTF-8 label without re-encoding would silently
//! replace the file's bytes. That refusal cannot be reached from a unit test —
//! `--encoding` lives in the process-wide runtime, which a `OnceLock` pins on
//! first read — so it is exercised here, through the binary.

use super::*;

/// `(defun f () "日本")` with the two kanji encoded as Shift_JIS.
const SHIFT_JIS_SOURCE: &[u8] = b"(defun f () \"\x93\xfa\x96\x7b\")\n";

#[test]
fn a_shift_jis_source_is_decoded_for_reading() {
    let dir = fresh_temp_dir("encoding-read");
    let file = dir.join("source.lisp");
    fs::write(&file, SHIFT_JIS_SOURCE).expect("write shift_jis fixture");

    paredit()
        .args(["--encoding", "shift_jis", "edit", "format", "--file"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("日本"));
}

#[test]
fn a_shift_jis_source_refuses_to_be_written_back() {
    let dir = fresh_temp_dir("encoding-write-refusal");
    let file = dir.join("source.lisp");
    fs::write(&file, SHIFT_JIS_SOURCE).expect("write shift_jis fixture");

    paredit()
        .args([
            "--encoding",
            "shift_jis",
            "edit",
            "format",
            "--write",
            "--file",
        ])
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "refusing to write: --encoding Shift_JIS does not support --write yet",
        ))
        .stderr(predicate::str::contains(
            "drop --encoding to write in UTF-8",
        ));

    assert_eq!(
        fs::read(&file).expect("read fixture"),
        SHIFT_JIS_SOURCE,
        "the refusal must leave the file's bytes exactly as they were"
    );
}

/// The code is the part an agent branches on, so it is asserted exactly.
/// `edit format` has no `--output` of its own, so its diagnosis envelope is
/// always the text one.
#[test]
fn the_encoding_write_refusal_carries_its_own_error_code() {
    let dir = fresh_temp_dir("encoding-write-refusal-code");
    let file = dir.join("source.lisp");
    fs::write(&file, SHIFT_JIS_SOURCE).expect("write shift_jis fixture");

    paredit()
        .args([
            "--encoding",
            "shift_jis",
            "edit",
            "format",
            "--write",
            "--file",
        ])
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::starts_with(
            "Error [refusal.encoding-write]:",
        ));
}
