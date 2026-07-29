use super::*;

fn capabilities_json_with_args(args: &[&str]) -> serde_json::Value {
    let output = paredit()
        .args(["inspect", "capabilities"])
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("capabilities emits valid JSON")
}

fn capabilities_json() -> serde_json::Value {
    capabilities_json_with_args(&[])
}

#[test]
fn capabilities_defaults_to_unchanged_schema_v1_shape() {
    let report = capabilities_json();
    let explicit_v1 = capabilities_json_with_args(&["--schema-version", "1"]);

    assert_eq!(report["schema_version"], 1);
    assert_eq!(report, explicit_v1);
    assert!(report.get("dialect_contract").is_none());

    let mut keys = report
        .as_object()
        .expect("capabilities report is an object")
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["about", "commands", "name", "schema_version", "version"]
    );

    let capabilities = report["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .find(|command| command["name"] == "inspect")
        .and_then(|inspect| inspect["commands"].as_array())
        .and_then(|commands| {
            commands
                .iter()
                .find(|command| command["name"] == "capabilities")
        })
        .expect("inspect capabilities command");
    assert!(
        capabilities["args"]
            .as_array()
            .expect("inspect capabilities args")
            .iter()
            .all(|arg| arg["id"] != "schema_version")
    );
}

#[test]
fn capabilities_reports_the_canonical_namespaces_and_meta_commands() {
    let report = capabilities_json();
    assert_eq!(report["name"], "paredit");
    assert_eq!(report["version"], env!("CARGO_PKG_VERSION"));

    let commands = report["commands"]
        .as_array()
        .expect("commands is an array")
        .iter()
        .map(|command| command["name"].as_str().expect("command name"))
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        ["inspect", "edit", "refactor", "lsp", "mcp", "completions"]
    );
}

#[test]
fn capabilities_documents_edit_write_flag_and_enum_values() {
    let report = capabilities_json();
    let namespaces = report["commands"].as_array().expect("commands");

    let edit = &namespaces[1];
    let wrap = edit["commands"]
        .as_array()
        .expect("edit commands")
        .iter()
        .find(|command| command["name"] == "wrap")
        .expect("edit wrap is listed");
    let write = wrap["args"]
        .as_array()
        .expect("wrap args")
        .iter()
        .find(|arg| arg["id"] == "write")
        .expect("wrap documents --write");
    assert_eq!(write["kind"], "flag");
    assert_eq!(write["long"], "write");

    let inspect = &namespaces[0];
    let dialect = inspect["commands"]
        .as_array()
        .expect("inspect commands")
        .iter()
        .find(|command| command["name"] == "dialect")
        .expect("inspect dialect is listed");
    let dialect_arg = dialect["args"]
        .as_array()
        .expect("dialect args")
        .iter()
        .find(|arg| arg["id"] == "dialect")
        .expect("dialect documents --dialect");
    let possible = dialect_arg["possible_values"]
        .as_array()
        .expect("--dialect enumerates possible values")
        .iter()
        .map(|value| value.as_str().expect("possible value"))
        .collect::<Vec<_>>();
    assert!(possible.contains(&"common-lisp"), "{possible:?}");
    assert!(possible.contains(&"clojure"), "{possible:?}");
}

#[test]
fn command_reference_documents_every_leaf_command() {
    let report = capabilities_json();
    let reference = fs::read_to_string("docs/src/commands.md").expect("read docs/src/commands.md");

    let mut missing = Vec::new();
    for namespace in report["commands"].as_array().expect("commands") {
        let Some(subcommands) = namespace["commands"].as_array() else {
            continue;
        };
        for command in subcommands {
            let name = command["name"].as_str().expect("command name");
            if !reference.contains(&format!("`{name}`")) {
                missing.push(format!(
                    "{} {name}",
                    namespace["name"].as_str().expect("namespace name")
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "docs/src/commands.md is missing commands that exist in the CLI:\n{}",
        missing.join("\n")
    );
}

#[test]
fn capabilities_text_output_lists_full_command_paths() {
    paredit()
        .args(["inspect", "capabilities", "--output", "text"])
        .assert()
        .success()
        .stdout(predicate::str::contains("paredit inspect check"))
        .stdout(predicate::str::contains("paredit edit wrap"))
        .stdout(predicate::str::contains("paredit refactor rename-function"));
}

#[test]
fn exit_codes_distinguish_gate_failures_from_hard_and_usage_errors() {
    // 3: a requested policy gate tripped after the report was printed.
    paredit()
        .args([
            "inspect",
            "find-symbol",
            "--symbol",
            "absent",
            "--require-occurrences",
            "1",
        ])
        .write_stdin("(present)")
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "require-occurrences policy failed",
        ));

    // 1: hard operational failure (unreadable input).
    paredit()
        .args([
            "inspect",
            "outline",
            "--file",
            "/nonexistent/paredit-test.lisp",
        ])
        .assert()
        .code(1);

    // 2: usage error from argument parsing.
    paredit()
        .args(["inspect", "find-symbol", "--bogus-flag"])
        .assert()
        .code(2);
}
