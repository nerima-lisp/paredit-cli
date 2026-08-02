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
        [
            "inspect",
            "edit",
            "refactor",
            "query",
            "fix",
            "migrate",
            "schema",
            "config",
            "generate",
            "lsp",
            "mcp",
            "serve",
            "tui",
            "completions"
        ]
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
    let reference =
        fs::read_to_string("docs/src/reference/api.md").expect("read docs/src/reference/api.md");

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
        "docs/src/reference/api.md is missing commands that exist in the CLI:\n{}",
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

/// Every leaf command under `report["commands"]`, keyed by its full
/// space-separated path (`"edit wrap"`) — the same spelling the dialect
/// contract's own `commands[].path` uses, so the two can be cross-referenced
/// directly.
fn leaf_commands_by_path(
    report: &serde_json::Value,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    fn walk(
        commands: &[serde_json::Value],
        prefix: &[&str],
        out: &mut std::collections::BTreeMap<String, serde_json::Value>,
    ) {
        for command in commands {
            let name = command["name"].as_str().expect("command name");
            let mut path = prefix.to_vec();
            path.push(name);
            match command["commands"].as_array() {
                Some(nested) if !nested.is_empty() => walk(nested, &path, out),
                _ => {
                    out.insert(path.join(" "), command.clone());
                }
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(
        report["commands"].as_array().expect("commands"),
        &[],
        &mut out,
    );
    out
}

/// FR-011: every command the dialect contract counts (the 452 commands
/// across `inspect`/`edit`/`refactor`/`query`/`fix`/`migrate`) carries a
/// `writes` flag and a non-empty `possible_error_codes` set. A missing or
/// empty set would mean a command nobody classified — the exact drift this
/// contract exists to catch.
#[test]
fn every_dialect_contract_command_has_a_writes_flag_and_a_non_empty_error_code_set() {
    let report = capabilities_json_with_args(&["--schema-version", "3"]);
    let leaves = leaf_commands_by_path(&report);

    let dialect_paths: Vec<String> = report["dialect_contract"]["commands"]
        .as_array()
        .expect("dialect_contract.commands")
        .iter()
        .map(|command| command["path"].as_str().expect("path").to_owned())
        .collect();
    assert_eq!(
        dialect_paths.len(),
        452,
        "the dialect contract's own command inventory changed size; update this test's \
         expectation alongside it"
    );

    let mut problems = Vec::new();
    for path in &dialect_paths {
        let Some(command) = leaves.get(path) else {
            problems.push(format!("{path}: not found in the capabilities tree"));
            continue;
        };
        if !command["writes"].is_boolean() {
            problems.push(format!("{path}: writes is missing or not a boolean"));
        }
        let non_empty = command["possible_error_codes"]
            .as_array()
            .is_some_and(|codes| !codes.is_empty());
        if !non_empty {
            problems.push(format!("{path}: possible_error_codes is missing or empty"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The baseline codes (`environment.io`, `input.unparsable`, `input.not-utf8`,
/// `environment.timeout`, `argument.no-input`, `internal.unclassified`) come
/// from the shared read/parse/serialize plumbing every command goes through,
/// so they have to appear for a representative command from every family —
/// not just the ones the mechanical classifier was built by staring at.
#[test]
fn baseline_read_codes_reach_a_representative_of_every_command_family() {
    let report = capabilities_json();
    let leaves = leaf_commands_by_path(&report);
    let representatives = [
        "inspect check",
        "inspect lint",
        "edit wrap",
        "refactor rename-function",
        "fix apply",
        "query replace",
        "migrate run",
        "generate accessors",
        "config init",
    ];
    let baseline = [
        "environment.io",
        "input.unparsable",
        "input.not-utf8",
        "environment.timeout",
        "argument.no-input",
        "internal.unclassified",
        "refusal.input-too-large",
    ];

    for path in representatives {
        let command = leaves
            .get(path)
            .unwrap_or_else(|| panic!("{path} is a real command"));
        let codes: Vec<&str> = command["possible_error_codes"]
            .as_array()
            .unwrap_or_else(|| panic!("{path} has possible_error_codes"))
            .iter()
            .map(|value| value.as_str().expect("code label is a string"))
            .collect();
        for code in baseline {
            assert!(
                codes.contains(&code),
                "{path} is missing baseline code {code:?}: {codes:?}"
            );
        }
    }
}

/// `writes` is derived from the command's own `--write`/`--fix`/`--apply`/
/// `--in-place` flags plus a short verb-named allowlist, not a hand-kept
/// per-command table — these are the load-bearing cases that convention has
/// to get right, including two that only reading the command's own source
/// settles: `refactor rename-symbol` never touches disk at all (it only
/// prints), and `config init` writes unconditionally with no `--write` flag
/// of its own.
#[test]
fn writes_matches_the_write_flag_and_verb_convention() {
    let report = capabilities_json();
    let leaves = leaf_commands_by_path(&report);
    let expectations = [
        ("fix apply", true),
        ("fix check", false),
        ("fix plan", false),
        ("fix list", false),
        ("inspect check", false),
        ("inspect lint", true),
        ("edit wrap", true),
        ("edit select", false),
        ("edit copy", false),
        ("edit navigate", false),
        ("refactor rename-symbol", false),
        ("refactor rename-symbols", true),
        ("refactor apply", true),
        ("refactor plan", false),
        ("query find", false),
        ("query replace", true),
        ("migrate run", true),
        ("config init", true),
        ("config schema", false),
        ("config show", false),
        ("refactor create-checkpoint", true),
        ("refactor delete-checkpoint", true),
        ("refactor restore-checkpoint", true),
        ("refactor list-checkpoints", false),
    ];
    for (path, expected) in expectations {
        let command = leaves
            .get(path)
            .unwrap_or_else(|| panic!("{path} is a real command"));
        assert_eq!(command["writes"], expected, "{path}");
    }
}

/// A namespace node (`edit`, `refactor`) carries neither field, the same way
/// it carries no `gates`: "cannot write" and "we did not classify this,
/// because it is not invocable" have to stay distinguishable.
#[test]
fn a_namespace_node_carries_neither_field() {
    let report = capabilities_json();
    let edit = report["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .find(|command| command["name"] == "edit")
        .expect("edit namespace");
    assert!(edit.get("writes").is_none(), "{edit}");
    assert!(edit.get("possible_error_codes").is_none(), "{edit}");
}
