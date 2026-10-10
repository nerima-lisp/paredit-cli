use super::*;
use std::path::Path;

const SOURCE: &str = "; heading\r\n(foo bar)  ; keep\r\n(baz foo)\r\n";

fn prepare(name: &str, source: &str, operations: serde_json::Value) -> (PathBuf, PathBuf) {
    let directory = fresh_temp_dir(name);
    let file = directory.join("source.lisp");
    let request = directory.join("request.json");
    fs::write(&file, source).expect("source fixture");
    fs::write(
        &request,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "input_hash": paredit_core_cli::shared::stable_text_hash(source),
            "operations": operations,
        }))
        .expect("request JSON"),
    )
    .expect("request fixture");
    (file, request)
}

fn bulk(file: &Path, request: &Path, mode: &[&str]) -> assert_cmd::assert::Assert {
    paredit()
        .args(["refactor", "bulk"])
        .arg(file)
        .arg("--request")
        .arg(request)
        .args(mode)
        .assert()
}

fn mixed_operations() -> serde_json::Value {
    serde_json::json!([
        {"kind": "wrap", "path": "0", "delimiter": "paren"},
        {"kind": "rename-in-form", "path": "1", "from": "foo", "to": "qux"},
    ])
}

#[test]
fn cli_bulk_preview_uses_original_snapshot_and_preserves_trivia() {
    let (file, request) = prepare("bulk-preview", SOURCE, mixed_operations());
    let result = bulk(&file, &request, &[]).success();
    let report: serde_json::Value = serde_json::from_slice(&result.get_output().stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["version"], 1);
    assert_eq!(
        report["rewritten"],
        "; heading\r\n((foo bar))  ; keep\r\n(baz qux)\r\n"
    );
    assert_eq!(report["operation_count"], 2);
    assert_eq!(report["edit_count"], 2);
    assert_eq!(report["forms_before"], 2);
    assert_eq!(report["forms_after"], 2);
    assert_eq!(report["written"], false);
    assert_eq!(fs::read(&file).unwrap(), SOURCE.as_bytes());
}

#[test]
fn cli_bulk_diff_and_positive_write_share_validated_rewrite() {
    let (file, request) = prepare("bulk-write", SOURCE, mixed_operations());
    let diff = bulk(&file, &request, &["--diff"]).success();
    let output = String::from_utf8_lossy(&diff.get_output().stdout);
    assert!(output.contains("+((foo bar))  ; keep"), "{output}");
    assert!(output.contains("+(baz qux)"), "{output}");
    assert_eq!(fs::read(&file).unwrap(), SOURCE.as_bytes());
    let written = bulk(&file, &request, &["--write"]).success();
    let report: serde_json::Value = serde_json::from_slice(&written.get_output().stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["version"], 1);
    assert_eq!(report["written"], true);
    assert_eq!(
        fs::read(&file).unwrap(),
        b"; heading\r\n((foo bar))  ; keep\r\n(baz qux)\r\n"
    );
    assert_eq!(
        report["output_hash"],
        paredit_core_cli::shared::stable_text_hash(&fs::read_to_string(&file).unwrap())
    );
}

#[test]
fn cli_bulk_actual_disjoint_spans_allow_ancestor_rename_scope() {
    let (file, request) = prepare(
        "bulk-disjoint",
        "(foo (bar))",
        serde_json::json!([
            {"kind":"wrap","path":"0.1","delimiter":"paren"},
            {"kind":"rename-in-form","path":"0","from":"foo","to":"qux"}
        ]),
    );
    bulk(&file, &request, &["--write"]).success();
    assert_eq!(fs::read_to_string(&file).unwrap(), "(qux ((bar)))");
}

#[test]
fn cli_bulk_rename_does_not_match_an_intermediate_replacement() {
    let (file, request) = prepare(
        "bulk-original",
        "(foo)",
        serde_json::json!([
            {"kind":"rename-in-form","path":"0","from":"foo","to":"bar"},
            {"kind":"rename-in-form","path":"0","from":"bar","to":"baz"}
        ]),
    );
    bulk(&file, &request, &["--write"]).success();
    assert_eq!(fs::read_to_string(&file).unwrap(), "(bar)");
}

#[test]
fn cli_bulk_overlap_and_invalid_output_refuse_in_every_mode_without_write() {
    for operations in [
        serde_json::json!([
            {"kind":"wrap","path":"0","delimiter":"paren"},
            {"kind":"rename-in-form","path":"0","from":"foo","to":"bar"}
        ]),
        serde_json::json!([
            {"kind":"rename-in-form","path":"0","from":"foo","to":"|"}
        ]),
    ] {
        let (file, request) = prepare("bulk-invalid", "(foo)", operations);
        for mode in [&[][..], &["--diff"][..], &["--write"][..]] {
            bulk(&file, &request, mode).failure();
            assert_eq!(fs::read(&file).unwrap(), b"(foo)");
        }
    }
}

#[test]
fn cli_bulk_stale_hash_refuses_in_every_mode_without_write() {
    let (file, request) = prepare("bulk-stale", SOURCE, mixed_operations());
    let changed = format!("{SOURCE}; changed\n");
    fs::write(&file, &changed).unwrap();
    for mode in [&[][..], &["--diff"][..], &["--write"][..]] {
        bulk(&file, &request, mode).failure();
        assert_eq!(fs::read(&file).unwrap(), changed.as_bytes());
    }
}

#[test]
fn cli_bulk_strict_json_refuses_unknown_and_duplicate_fields_without_write() {
    let (file, request) = prepare("bulk-json", "(foo)", serde_json::json!([]));
    let hash = paredit_core_cli::shared::stable_text_hash("(foo)");
    for json in [
        format!(r#"{{"version":1,"version":1,"input_hash":"{hash}","operations":[]}}"#),
        format!(r#"{{"version":1,"input_hash":"{hash}","operations":[],"extra":0}}"#),
        format!(
            r#"{{"version":1,"input_hash":"{hash}","operations":[{{"kind":"kill","path":"0"}}]}}"#
        ),
        format!(
            r#"{{"version":1,"input_hash":"{hash}","operations":[{{"kind":"wrap","path":"0","path":"0","delimiter":"paren"}}]}}"#
        ),
        format!(
            r#"{{"version":1,"input_hash":"{hash}","operations":[{{"kind":"wrap","kind":"wrap","path":"0","delimiter":"paren"}}]}}"#
        ),
        format!(
            r#"{{"version":1,"input_hash":"{hash}","operations":[{{"kind":"wrap","path":"0","delimiter":"paren","extra":0}}]}}"#
        ),
    ] {
        fs::write(&request, json).unwrap();
        for mode in [&[][..], &["--diff"][..], &["--write"][..]] {
            bulk(&file, &request, mode).failure();
            assert_eq!(fs::read(&file).unwrap(), b"(foo)");
        }
    }
}

#[test]
fn cli_bulk_bad_version_hash_or_path_refuses_without_write() {
    let (file, request) = prepare("bulk-guards", "(foo)", serde_json::json!([]));
    let hash = paredit_core_cli::shared::stable_text_hash("(foo)");
    for value in [
        serde_json::json!({"version":2,"input_hash":hash,"operations":[{"kind":"wrap","path":"0","delimiter":"paren"}]}),
        serde_json::json!({"version":1,"input_hash":"bad","operations":[{"kind":"wrap","path":"0","delimiter":"paren"}]}),
        serde_json::json!({"version":1,"input_hash":hash,"operations":[]}),
        serde_json::json!({"version":1,"input_hash":hash,"operations":[{"kind":"wrap","path":"","delimiter":"paren"}]}),
        serde_json::json!({"version":1,"input_hash":hash,"operations":[{"kind":"wrap","path":"8","delimiter":"paren"}]}),
    ] {
        fs::write(&request, serde_json::to_vec(&value).unwrap()).unwrap();
        bulk(&file, &request, &["--write"]).failure();
        assert_eq!(fs::read(&file).unwrap(), b"(foo)");
    }
}

#[test]
fn cli_bulk_noop_reports_unchanged_and_does_not_write() {
    let (file, request) = prepare(
        "bulk-noop",
        "(foo)",
        serde_json::json!([
            {"kind":"rename-in-form","path":"0","from":"absent","to":"bar"}
        ]),
    );
    let result = bulk(&file, &request, &["--write"]).success();
    let report: serde_json::Value = serde_json::from_slice(&result.get_output().stdout).unwrap();
    assert_eq!(report["changed"], false);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["version"], 1);
    assert_eq!(report["written"], false);
    assert_eq!(report["edit_count"], 0);
    assert_eq!(fs::read(&file).unwrap(), b"(foo)");
}

#[test]
fn cli_bulk_rename_trailing_escape_refuses_without_consuming_an_adjacent_atom() {
    let (file, request) = prepare(
        "bulk-trailing-escape",
        "(foo bar)",
        serde_json::json!([
            {"kind":"rename-in-form","path":"0","from":"foo","to":"baz\\"}
        ]),
    );
    for mode in [&[][..], &["--diff"][..], &["--write"][..]] {
        bulk(&file, &request, mode).failure();
        assert_eq!(fs::read(&file).unwrap(), b"(foo bar)");
    }
}

#[test]
fn cli_bulk_rename_preserves_case_distinctions_outside_common_lisp() {
    for dialect in ["clojure", "emacs-lisp", "scheme", "racket"] {
        let (file, request) = prepare(
            "bulk-case-sensitive",
            "(foo Foo)",
            serde_json::json!([
                {"kind":"rename-in-form","path":"0","from":"foo","to":"bar"}
            ]),
        );
        bulk(&file, &request, &["--dialect", dialect, "--write"]).success();
        assert_eq!(fs::read_to_string(&file).unwrap(), "(bar Foo)", "{dialect}");
    }
}

#[test]
fn cli_bulk_rename_common_lisp_still_folds_unescaped_case() {
    let (file, request) = prepare(
        "bulk-case-folded",
        "(foo Foo |Foo|)",
        serde_json::json!([
            {"kind":"rename-in-form","path":"0","from":"foo","to":"bar"}
        ]),
    );
    bulk(&file, &request, &["--write"]).success();
    assert_eq!(fs::read_to_string(&file).unwrap(), "(bar bar |Foo|)");
}
