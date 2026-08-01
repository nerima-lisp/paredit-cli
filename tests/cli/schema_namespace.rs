//! The `schema` namespace end to end: `paredit schema check`.
//!
//! The property worth a process boundary: `--schema-name` selection has to
//! survive from the schema file, through argument parsing, to the report —
//! and the disambiguation refusal has to happen before any instance work, so
//! a caller who forgot the flag is not left guessing which schema silently
//! ran.

use super::*;

fn workspace(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = fresh_temp_dir(name);
    for (file, source) in files {
        let path = dir.join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::write(path, source).expect("write fixture");
    }
    dir
}

const CONFIG_SCHEMA: &str = r#"
(defschema config
  (fields
    (:name (:type string))
    (:port (:type integer :min 1 :max 65535))
    (:mode (:type string :one-of ("dev" "staging" "prod")))
    (:label (:type string :matches "svc-*" :optional t))))
"#;

#[test]
fn a_valid_instance_reports_no_findings_and_exits_zero() {
    let dir = workspace(
        "schema-valid",
        &[
            ("schema.lisp", CONFIG_SCHEMA),
            (
                "good.lisp",
                r#"(:name "widget" :port 8080 :mode "dev" :label "svc-widget")"#,
            ),
        ],
    );

    let output = paredit()
        .args(["schema", "check"])
        .arg(dir.join("good.lisp"))
        .args(["--schema"])
        .arg(dir.join("schema.lisp"))
        .output()
        .expect("run schema check");
    assert!(output.status.success(), "{output:?}");

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(report["finding_count"], 0);
}

#[test]
fn a_type_mismatch_and_a_missing_field_are_both_reported() {
    let dir = workspace(
        "schema-invalid",
        &[
            ("schema.lisp", CONFIG_SCHEMA),
            ("bad.lisp", r#"(:port "not-a-number" :mode "dev")"#),
        ],
    );

    let output = paredit()
        .args(["schema", "check"])
        .arg(dir.join("bad.lisp"))
        .args(["--schema"])
        .arg(dir.join("schema.lisp"))
        .output()
        .expect("run schema check");
    assert!(
        output.status.success(),
        "reports, does not gate, by default"
    );

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    let findings = report["files"][0]["findings"].as_array().expect("findings");
    let kinds: Vec<&str> = findings
        .iter()
        .map(|finding| finding["kind"].as_str().expect("kind"))
        .collect();
    assert!(kinds.contains(&"missing-required-field"), "{kinds:?}");
    assert!(kinds.contains(&"type-mismatch"), "{kinds:?}");
}

#[test]
fn fail_on_violation_gates_the_run() {
    let dir = workspace(
        "schema-gate",
        &[
            ("schema.lisp", CONFIG_SCHEMA),
            ("bad.lisp", r#"(:port "not-a-number" :mode "dev")"#),
        ],
    );

    paredit()
        .args(["schema", "check"])
        .arg(dir.join("bad.lisp"))
        .args(["--schema"])
        .arg(dir.join("schema.lisp"))
        .arg("--fail-on-violation")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("schema check policy failed"));
}

#[test]
fn a_single_schema_file_needs_no_schema_name_flag() {
    let dir = workspace(
        "schema-implicit",
        &[
            (
                "schema.lisp",
                "(defschema only (fields (:x (:type string))))",
            ),
            ("instance.lisp", r#"(:x "hi")"#),
        ],
    );

    paredit()
        .args(["schema", "check"])
        .arg(dir.join("instance.lisp"))
        .args(["--schema"])
        .arg(dir.join("schema.lisp"))
        .assert()
        .success();
}

const MULTI_SCHEMA: &str = r#"
(defschema a (fields (:x (:type string))))
(defschema b (fields (:y (:type integer))))
"#;

#[test]
fn several_schemas_with_no_schema_name_flag_refuses_and_lists_the_names() {
    let dir = workspace(
        "schema-ambiguous",
        &[
            ("schema.lisp", MULTI_SCHEMA),
            ("instance.lisp", r#"(:x "hi")"#),
        ],
    );

    paredit()
        .args(["schema", "check"])
        .arg(dir.join("instance.lisp"))
        .args(["--schema"])
        .arg(dir.join("schema.lisp"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("--schema-name"))
        .stderr(predicate::str::contains('a'))
        .stderr(predicate::str::contains('b'));
}

#[test]
fn schema_name_selects_among_several_schemas() {
    let dir = workspace(
        "schema-selected",
        &[("schema.lisp", MULTI_SCHEMA), ("instance.lisp", "(:y 42)")],
    );

    let output = paredit()
        .args(["schema", "check"])
        .arg(dir.join("instance.lisp"))
        .args(["--schema"])
        .arg(dir.join("schema.lisp"))
        .args(["--schema-name", "b"])
        .output()
        .expect("run schema check");
    assert!(output.status.success(), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(report["finding_count"], 0);
}

#[test]
fn an_unknown_schema_name_names_the_ones_that_exist() {
    let dir = workspace(
        "schema-unknown-name",
        &[
            ("schema.lisp", MULTI_SCHEMA),
            ("instance.lisp", r#"(:x "hi")"#),
        ],
    );

    paredit()
        .args(["schema", "check"])
        .arg(dir.join("instance.lisp"))
        .args(["--schema"])
        .arg(dir.join("schema.lisp"))
        .args(["--schema-name", "no-such-schema"])
        .assert()
        .failure()
        .stderr(predicate::str::contains('a'))
        .stderr(predicate::str::contains('b'));
}

#[test]
fn an_instance_that_is_not_alist_or_plist_shaped_is_refused_clearly() {
    let dir = workspace(
        "schema-shape",
        &[("schema.lisp", CONFIG_SCHEMA), ("scalar.lisp", "42")],
    );

    paredit()
        .args(["schema", "check"])
        .arg(dir.join("scalar.lisp"))
        .args(["--schema"])
        .arg(dir.join("schema.lisp"))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "not shaped like an alist or a plist",
        ));
}

#[test]
fn a_schema_file_that_does_not_parse_as_lisp_is_refused_not_panicked() {
    let dir = workspace(
        "schema-malformed",
        &[
            ("schema.lisp", "(defschema s (fields"),
            ("instance.lisp", r#"(:x "hi")"#),
        ],
    );

    paredit()
        .args(["schema", "check"])
        .arg(dir.join("instance.lisp"))
        .args(["--schema"])
        .arg(dir.join("schema.lisp"))
        .assert()
        .failure();
}

#[test]
fn a_schema_file_with_a_duplicate_schema_name_is_refused() {
    let dir = workspace(
        "schema-duplicate",
        &[
            (
                "schema.lisp",
                "(defschema s (fields (:x (:type string)))) (defschema s (fields (:y (:type integer))))",
            ),
            ("instance.lisp", r#"(:x "hi")"#),
        ],
    );

    paredit()
        .args(["schema", "check"])
        .arg(dir.join("instance.lisp"))
        .args(["--schema"])
        .arg(dir.join("schema.lisp"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("s"));
}

#[test]
fn text_output_lists_the_findings_as_tab_separated_rows() {
    let dir = workspace(
        "schema-text",
        &[
            ("schema.lisp", CONFIG_SCHEMA),
            ("bad.lisp", r#"(:port 99999 :mode "dev")"#),
        ],
    );

    paredit()
        .args(["schema", "check"])
        .arg(dir.join("bad.lisp"))
        .args(["--schema"])
        .arg(dir.join("schema.lisp"))
        .args(["--output", "text"])
        .assert()
        .success()
        .stdout(predicate::str::contains("max-violation"));
}

/// Safety invariant: this crate never evaluates the data it inspects. Its
/// only non-workspace dependencies are `clap` (argument parsing), `serde_json`
/// (report rendering), and `thiserror` (typed errors) — none of which spawns
/// a process, loads a dynamic library, or interprets Lisp as code. A
/// dependency on any code-execution facility would show up here as a new
/// line in the manifest, which this test would catch.
#[test]
fn the_schema_check_crate_declares_no_code_execution_dependency() {
    let manifest = fs::read_to_string("packages/feature/schema-check/Cargo.toml")
        .expect("read schema-check manifest");
    let dependencies_start = manifest
        .find("[dependencies]")
        .expect("manifest has a [dependencies] section")
        + "[dependencies]".len();
    let dependencies_end = manifest[dependencies_start..]
        .find("\n[")
        .map_or(manifest.len(), |offset| dependencies_start + offset);
    let dependencies = &manifest[dependencies_start..dependencies_end];

    let declared: Vec<&str> = dependencies
        .lines()
        .filter_map(|line| line.split('=').next())
        .map(str::trim)
        // `clap.workspace = true` names the dependency before the dot.
        .map(|name| name.split('.').next().unwrap_or(name))
        .filter(|name| !name.is_empty() && !name.starts_with('#'))
        .collect();

    assert_eq!(
        declared,
        [
            "paredit-core-syntax",
            "paredit-core-cli",
            "clap",
            "serde_json",
            "thiserror"
        ],
        "schema-check's dependency list changed; if a new dependency is a code-execution \
         facility (a process spawner, a dynamic loader, an eval-capable regex/template engine), \
         it would violate this tool's \"never evaluate what it inspects\" invariant"
    );

    // No `std::process::Command` (or any other process-spawning call) is
    // reachable from this crate's own source either.
    for entry in walk_rust_sources("packages/feature/schema-check/src") {
        let source = fs::read_to_string(&entry).expect("read schema-check source");
        assert!(
            !source.contains("process::Command") && !source.contains("Command::new"),
            "{} spawns a process, which this crate must never do",
            entry.display()
        );
    }
}

fn walk_rust_sources(root: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found
}
