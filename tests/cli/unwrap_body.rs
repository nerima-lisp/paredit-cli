use super::*;

#[test]
fn cli_previews_all_body_forms_without_writing() {
    let dir = fresh_temp_dir("unwrap-body-preview");
    let file = dir.join("wrapper.el");
    let original = "(with-current-buffer buf ; body\n (a) ; between\n (b))\n";
    fs::write(&file, original).unwrap();
    let output = paredit()
        .args([
            "refactor",
            "unwrap-body",
            "--body-start",
            "1",
            "--path",
            "0",
            "--function",
            "with-current-buffer",
        ])
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(plan["replacement"], "; body\n (a) ; between\n (b)");
    assert_eq!(plan["bodyStart"], 1);
    assert_eq!(plan["bodyCount"], 2);
    assert_eq!(plan["callArgumentCount"], 3);
    assert_eq!(plan["written"], false);
    assert_eq!(plan["semantics"], "structural-only");
    assert_eq!(fs::read(&file).unwrap(), original.as_bytes());
}

#[test]
fn cli_writes_supported_sequence_body() {
    let dir = fresh_temp_dir("unwrap-body-write");
    let file = dir.join("wrapper.lisp");
    fs::write(&file, "(CL:PROGN (before) (wrap config (a) (b)) (after))\n").unwrap();
    let output = paredit()
        .args([
            "refactor",
            "unwrap-body",
            "--body-start",
            "1",
            "--path",
            "0.2",
            "--write",
        ])
        .arg("--file")
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(plan["written"], true);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "(CL:PROGN (before) (a) (b) (after))\n"
    );
}

#[test]
fn cli_refusals_preserve_file_bytes() {
    let dir = fresh_temp_dir("unwrap-body-refusals");
    let file = dir.join("wrapper.lisp");
    for (input, path, start, function) in [
        ("(consume (wrap a b))", "0.1", "0", "wrap"),
        ("'(progn (wrap a))", "0.1", "0", "wrap"),
        ("(quote (wrap a))", "0.1", "0", "wrap"),
        ("#.(wrap a)", "0", "0", "wrap"),
        ("(wrap a)", "0", "1", "wrap"),
        ("(wrap a)", "0", "0", "other"),
        ("(wrap ; header\n config a)", "0", "1", "wrap"),
        ("(wrap a b)tail", "0", "0", "wrap"),
    ] {
        fs::write(&file, input).unwrap();
        paredit()
            .args([
                "refactor",
                "unwrap-body",
                "--path",
                path,
                "--body-start",
                start,
                "--function",
                function,
                "--write",
            ])
            .arg("--file")
            .arg(&file)
            .assert()
            .failure();
        assert_eq!(fs::read(&file).unwrap(), input.as_bytes(), "{input}");
    }
}

#[test]
fn cli_requires_explicit_body_start() {
    paredit()
        .args([
            "refactor",
            "unwrap-body",
            "--dialect",
            "common-lisp",
            "--path",
            "0",
        ])
        .write_stdin("(wrap a b)")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--body-start"));
}

#[test]
fn cli_supports_compact_offset_selector_and_text_preview() {
    paredit()
        .args([
            "refactor",
            "unwrap-body",
            "--dialect",
            "common-lisp",
            "--at",
            "0",
            "--body-start",
            "1",
            "--output",
            "text",
        ])
        .write_stdin("(wrap config (a) (b))")
        .assert()
        .success()
        .stdout(predicate::str::contains("body_count\t2"))
        .stdout(predicate::str::contains("semantics\tstructural-only"));
}

#[cfg(unix)]
#[test]
fn cli_write_failure_preserves_bytes_and_cleans_temporary_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = fresh_temp_dir("unwrap-body-write-failure");
    let readonly_dir = dir.join("readonly");
    fs::create_dir_all(&readonly_dir).unwrap();
    let file = readonly_dir.join("wrapper.el");
    let input = "(wrap (a) (b))\n";
    fs::write(&file, input).unwrap();
    fs::set_permissions(&readonly_dir, fs::Permissions::from_mode(0o555)).unwrap();
    let result = paredit()
        .args([
            "refactor",
            "unwrap-body",
            "--body-start",
            "0",
            "--path",
            "0",
            "--write",
        ])
        .arg("--file")
        .arg(&file)
        .assert();
    fs::set_permissions(&readonly_dir, fs::Permissions::from_mode(0o755)).unwrap();
    result
        .failure()
        .stderr(predicate::str::contains("Permission denied"));
    assert_eq!(fs::read(&file).unwrap(), input.as_bytes());
    assert!(fs::read_dir(&readonly_dir).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".paredit-")
    }));
}
