use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn capabilities_json(schema_version: &str) -> serde_json::Value {
    let output = paredit()
        .args([
            "inspect",
            "capabilities",
            "--schema-version",
            schema_version,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("capabilities emits valid JSON")
}

fn collect_leaf_paths(
    commands: &[serde_json::Value],
    prefix: &mut Vec<String>,
    leaves: &mut BTreeSet<String>,
) {
    for command in commands {
        let name = command["name"].as_str().expect("command name");
        prefix.push(name.to_owned());

        match command
            .get("commands")
            .and_then(serde_json::Value::as_array)
        {
            Some(children) if !children.is_empty() => {
                collect_leaf_paths(children, prefix, leaves);
            }
            _ => {
                assert!(leaves.insert(prefix.join(" ")), "duplicate Clap leaf path");
            }
        }

        prefix.pop();
    }
}

fn clap_contract_leaf_paths(report: &serde_json::Value) -> BTreeSet<String> {
    let mut leaves = BTreeSet::new();
    for namespace in report["commands"].as_array().expect("root commands") {
        let name = namespace["name"].as_str().expect("namespace name");
        if !matches!(name, "inspect" | "edit" | "refactor") {
            continue;
        }

        let mut prefix = vec![name.to_owned()];
        collect_leaf_paths(
            namespace["commands"]
                .as_array()
                .expect("namespace commands"),
            &mut prefix,
            &mut leaves,
        );
    }
    leaves
}

#[test]
fn schema_v2_registry_is_an_exact_bijection_with_clap_leaves() {
    let v1 = capabilities_json("1");
    let v2 = capabilities_json("2");
    let commands = v2["dialect_contract"]["commands"]
        .as_array()
        .expect("dialect contract commands");
    let registry_paths = commands
        .iter()
        .map(|command| command["path"].as_str().expect("registry command path"))
        .collect::<Vec<_>>();
    let unique_registry_paths = registry_paths.iter().copied().collect::<BTreeSet<_>>();

    assert_eq!(registry_paths.len(), unique_registry_paths.len());
    assert_eq!(registry_paths.len(), 331);
    assert_eq!(
        clap_contract_leaf_paths(&v1),
        unique_registry_paths
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );
}

const DIALECTS: [&str; 10] = [
    "common-lisp",
    "emacs-lisp",
    "lfe",
    "scheme",
    "racket",
    "clojure",
    "hy",
    "carp",
    "janet",
    "fennel",
];

fn dialect_columns() -> BTreeSet<&'static str> {
    DIALECTS.into_iter().collect()
}

/// Collects every cell of the matrix as `path|dialect -> status`.
fn support_cells(contract: &serde_json::Value) -> BTreeMap<String, String> {
    let mut cells = BTreeMap::new();
    for command in contract["commands"].as_array().expect("contract commands") {
        let path = command["path"].as_str().expect("command path");
        let support = command["support"].as_object().expect("support map");
        assert_eq!(
            support.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            dialect_columns(),
            "dialect columns for {path}"
        );
        for (dialect, status) in support {
            cells.insert(
                format!("{path}|{dialect}"),
                status.as_str().expect("support status").to_owned(),
            );
        }
    }
    cells
}

#[test]
fn schema_v2_keeps_its_three_value_vocabulary() {
    let report = capabilities_json("2");
    assert_eq!(report["schema_version"], 2);

    let contract = &report["dialect_contract"];
    assert_eq!(contract["command_count"], 331);
    assert_eq!(contract["dialect_count"], 10);
    assert_eq!(contract["cell_count"], 3310);
    assert_eq!(contract["dialects"], serde_json::json!(DIALECTS));
    assert_eq!(
        contract["statuses"],
        serde_json::json!(["supported", "unsupported", "unknown"])
    );
    // The tier and depth fields belong to version 3 and must not leak back.
    assert!(contract.get("tiers").is_none());
    assert!(contract.get("dialect_depth").is_none());
    assert!(contract["commands"][0].get("tier").is_none());

    let mut category_counts = BTreeMap::new();
    for command in contract["commands"].as_array().expect("commands") {
        *category_counts
            .entry(command["category"].as_str().expect("category"))
            .or_insert(0) += 1;
    }
    assert_eq!(
        category_counts,
        BTreeMap::from([
            ("format", 2),
            ("introspection", 220),
            ("semantic", 80),
            ("structural", 29),
        ])
    );

    let cells = support_cells(contract);
    assert_eq!(cells.len(), 3310);
    let vocabulary = cells.values().map(String::as_str).collect::<BTreeSet<_>>();
    assert!(
        vocabulary.is_subset(&BTreeSet::from(["supported", "unsupported"])),
        "version 2 emitted {vocabulary:?}"
    );
}

#[test]
fn schema_v3_answers_every_cell_and_names_the_tier_it_used() {
    let report = capabilities_json("3");
    assert_eq!(report["schema_version"], 3);

    let contract = &report["dialect_contract"];
    assert_eq!(contract["cell_count"], 3310);
    assert_eq!(
        contract["statuses"],
        serde_json::json!(["supported", "silent", "unsupported", "unknown"])
    );
    assert_eq!(
        contract["tiers"],
        serde_json::json!([
            "syntax",
            "scope",
            "common-lisp-family",
            "common-lisp-semantics"
        ])
    );

    let tiers = contract["tiers"]
        .as_array()
        .expect("tiers")
        .iter()
        .map(|tier| tier.as_str().expect("tier name"))
        .collect::<BTreeSet<_>>();
    for command in contract["commands"].as_array().expect("commands") {
        let path = command["path"].as_str().expect("path");
        let tier = command["tier"].as_str().unwrap_or_else(|| panic!("{path}"));
        assert!(tiers.contains(tier), "{path} claims unknown tier {tier}");
    }

    // The whole point of the matrix: no cell may answer "unknown".
    let cells = support_cells(contract);
    assert_eq!(cells.len(), 3310);
    let unanswered = cells
        .iter()
        .filter(|(_, status)| *status == "unknown")
        .map(|(cell, _)| cell.as_str())
        .collect::<Vec<_>>();
    assert!(unanswered.is_empty(), "unanswered cells: {unanswered:?}");

    // Common Lisp is the dialect the analysis layer is written against, so it
    // is the one dialect for which nothing is silent or unsupported — barring
    // the commands whose subject is another dialect entirely. `inspect
    // elisp-file` reads the `lexical-binding` header and the autoload cookies
    // of an Emacs Lisp file; there is nothing for it to say about a `.lisp`
    // one, and it says nothing rather than refusing.
    const DIALECT_SPECIFIC: [&str; 1] = ["inspect elisp-file"];

    for (cell, status) in &cells {
        let Some(path) = cell.strip_suffix("|common-lisp") else {
            continue;
        };
        if DIALECT_SPECIFIC.contains(&path) {
            assert_eq!(status, "silent", "{cell}");
            continue;
        }
        assert_eq!(status, "supported", "{cell}");
    }
}

#[test]
fn schema_v3_summarises_how_deep_each_dialect_goes() {
    let report = capabilities_json("3");
    let contract = &report["dialect_contract"];
    let cells = support_cells(contract);

    let depth = contract["dialect_depth"]
        .as_array()
        .expect("dialect depth entries");
    assert_eq!(depth.len(), 10);

    for entry in depth {
        let dialect = entry["dialect"].as_str().expect("dialect");
        assert!(dialect_columns().contains(dialect));
        assert!(entry["family"].as_str().is_some_and(|f| !f.is_empty()));
        assert!(entry["separates_function_namespace"].is_boolean());

        let by_status = entry["by_status"].as_object().expect("by_status");
        let total: u64 = by_status
            .values()
            .map(|count| count.as_u64().expect("count"))
            .sum();
        assert_eq!(total, 331, "{dialect} counts do not cover every command");

        // The summary has to agree with the matrix it summarises.
        for (status, count) in by_status {
            let observed = cells
                .iter()
                .filter(|(cell, cell_status)| {
                    cell.ends_with(&format!("|{dialect}")) && *cell_status == status
                })
                .count();
            assert_eq!(
                count.as_u64().expect("count"),
                observed as u64,
                "{dialect}/{status}"
            );
        }
    }

    // Lisp-2 dialects resolve a call head in its own namespace.
    let lisp2 = depth
        .iter()
        .filter(|entry| entry["separates_function_namespace"] == serde_json::json!(true))
        .map(|entry| entry["dialect"].as_str().expect("dialect"))
        .collect::<BTreeSet<_>>();
    assert_eq!(lisp2, BTreeSet::from(["common-lisp", "emacs-lisp", "lfe"]));
}

/// A source file per dialect that exercises the sampled commands: it defines a
/// function, binds a local, and contains a redundant sequencing form for the
/// lint rules to find.
fn fixture_source(dialect: &str) -> (&'static str, String) {
    match dialect {
        "common-lisp" | "emacs-lisp" => (
            "lisp",
            "(defun add (a b) (let ((s (+ a b))) (progn s)))\n".to_owned(),
        ),
        "lfe" => (
            "lfe",
            "(defun add (a b) (let ((s (+ a b))) (progn s)))\n".to_owned(),
        ),
        "scheme" | "racket" => (
            "scm",
            "(define (add a b) (let ((s (+ a b))) (begin s)))\n".to_owned(),
        ),
        "clojure" | "hy" | "carp" | "janet" => (
            "clj",
            "(defn add [a b] (let [s (+ a b)] (do s)))\n".to_owned(),
        ),
        "fennel" => (
            "fnl",
            "(fn add [a b] (let [s (+ a b)] (do s)))\n".to_owned(),
        ),
        other => panic!("no fixture for {other}"),
    }
}

/// Cells sampled for executable evidence, one per tier and category.
///
/// Each entry is the command and the arguments that select something in the
/// fixture, so a claim of `supported` is checked by running the command rather
/// than by trusting the table that produced it.
const SAMPLED_COMMANDS: [(&str, &[&str]); 8] = [
    ("inspect outline", &[]),
    ("inspect definitions", &[]),
    ("inspect lint", &[]),
    ("inspect redundant-progn", &[]),
    ("inspect packages", &[]),
    ("edit select", &["--path", "0"]),
    ("refactor convert-let-to-let-star", &["--path", "0.3"]),
    ("refactor convert-labels-to-flet", &["--path", "0"]),
];

#[test]
fn sampled_cells_behave_the_way_the_matrix_says_they_do() {
    let report = capabilities_json("3");
    let cells = support_cells(&report["dialect_contract"]);
    let root = fresh_temp_dir("dialect-contract");
    let mut checked = 0;

    for dialect in DIALECTS {
        let (extension, source) = fixture_source(dialect);
        let file = root.join(format!("{dialect}.{extension}"));
        fs::write(&file, source).expect("write fixture");

        for (command, extra) in SAMPLED_COMMANDS {
            let status = cells
                .get(&format!("{command}|{dialect}"))
                .unwrap_or_else(|| panic!("{command}|{dialect} missing from the matrix"));

            let mut invocation = paredit();
            invocation.args(command.split(' '));
            if command.starts_with("inspect ") {
                invocation.arg(&file);
            } else {
                invocation.args(["--file", file.to_str().expect("utf-8 path")]);
            }
            invocation.args(["--dialect", dialect, "--output", "json"]);
            invocation.args(extra);
            let output = invocation.output().expect("run paredit");
            let succeeded = output.status.success();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            match status.as_str() {
                // A supported cell must not refuse on dialect grounds. It may
                // still refuse because the sampled path selects the wrong form.
                "supported" | "silent" => assert!(
                    !stderr.contains("supports only")
                        && !stderr.contains("currently supports")
                        && !stderr.contains("requires a known dialect"),
                    "{command}/{dialect} is {status} but refused: {stderr}"
                ),
                "unsupported" => assert!(
                    !succeeded,
                    "{command}/{dialect} is unsupported but succeeded: {stdout}"
                ),
                other => panic!("{command}/{dialect} has status {other}"),
            }

            // A silent report succeeds and finds nothing; that is the whole
            // reason the status exists.
            if status == "silent" && command.starts_with("inspect ") {
                assert!(
                    succeeded,
                    "{command}/{dialect} is silent but failed: {stderr}"
                );
                let findings = ["\"finding_count\": 0", "\"violations\": []"];
                assert!(
                    findings.iter().any(|marker| stdout.contains(marker))
                        || stdout.contains("\"defpackage_count\": 0"),
                    "{command}/{dialect} claims silence but reported: {stdout}"
                );
            }
            checked += 1;
        }
    }

    assert_eq!(checked, DIALECTS.len() * SAMPLED_COMMANDS.len());
    let _ = fs::remove_dir_all(&root);
}

/// Reports whose `supported` claim means they saw the definitions, paired with
/// the JSON field that proves it.
const SCOPE_TIER_EVIDENCE: [(&str, &str); 4] = [
    ("inspect definitions", "definition_count"),
    ("inspect complexity", "definition_count"),
    ("inspect unused-parameters", "checked_definition_count"),
    ("inspect call-graph", "definition_count"),
];

#[test]
fn scope_tier_reports_actually_see_the_definitions_they_claim_to() {
    // Exiting zero is not enough: `inspect unused-parameters` reported
    // `checked_definition_count: 0` for Fennel while succeeding, so the matrix
    // called it supported when it had examined nothing at all.
    let report = capabilities_json("3");
    let cells = support_cells(&report["dialect_contract"]);
    let root = fresh_temp_dir("dialect-contract-scope");

    for dialect in DIALECTS {
        let (extension, source) = fixture_source(dialect);
        let file = root.join(format!("{dialect}.{extension}"));
        fs::write(&file, source).expect("write fixture");

        for (command, field) in SCOPE_TIER_EVIDENCE {
            let status = cells
                .get(&format!("{command}|{dialect}"))
                .unwrap_or_else(|| panic!("{command}|{dialect} missing from the matrix"));
            if status != "supported" {
                continue;
            }

            let stdout = paredit()
                .args(command.split(' '))
                .arg(&file)
                .args(["--dialect", dialect, "--output", "json"])
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();
            let observed = serde_json::from_slice::<serde_json::Value>(&stdout)
                .unwrap_or_else(|error| panic!("{command}/{dialect}: {error}"))[field]
                .as_u64()
                .unwrap_or_else(|| panic!("{command}/{dialect} has no {field}"));

            assert!(
                observed > 0,
                "{command}/{dialect} claims support but {field} is 0, so it examined nothing"
            );
        }
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn common_lisp_finds_what_the_silent_dialects_miss() {
    // The claim behind `silent` is that the rule exists and simply has no
    // dialect to apply to, so the same shape must be found in Common Lisp.
    let root = fresh_temp_dir("dialect-contract-silence");
    fs::write(
        root.join("a.lisp"),
        "(defun add (a b) (let ((s (+ a b))) (progn s)))\n",
    )
    .expect("write fixture");
    fs::write(
        root.join("a.fnl"),
        "(fn add [a b] (let [s (+ a b)] (do s)))\n",
    )
    .expect("write fixture");

    let findings = |name: &str| -> u64 {
        let output = paredit()
            .args(["inspect", "lint"])
            .arg(root.join(name))
            .args(["--output", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        serde_json::from_slice::<serde_json::Value>(&output).expect("lint json")["finding_count"]
            .as_u64()
            .expect("finding count")
    };

    assert!(
        findings("a.lisp") > 0,
        "the Common Lisp fixture should trip a lint rule"
    );
    assert_eq!(
        findings("a.fnl"),
        0,
        "Fennel has no rules, which is what `silent` records"
    );
    let _ = fs::remove_dir_all(&root);
}
