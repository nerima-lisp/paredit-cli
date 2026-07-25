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
    assert_eq!(registry_paths.len(), 243);
    assert_eq!(
        clap_contract_leaf_paths(&v1),
        unique_registry_paths
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );
}

#[test]
fn schema_v2_reports_the_complete_dialect_matrix() {
    let report = capabilities_json("2");
    assert_eq!(report["schema_version"], 2);

    let contract = &report["dialect_contract"];
    assert_eq!(contract["command_count"], 243);
    assert_eq!(contract["dialect_count"], 10);
    assert_eq!(contract["cell_count"], 2430);
    assert_eq!(
        contract["dialects"],
        serde_json::json!([
            "common-lisp",
            "emacs-lisp",
            "lfe",
            "scheme",
            "racket",
            "clojure",
            "hy",
            "carp",
            "janet",
            "fennel"
        ])
    );
    assert_eq!(
        contract["statuses"],
        serde_json::json!(["supported", "unsupported", "unknown"])
    );

    let commands = contract["commands"].as_array().expect("contract commands");
    let mut category_counts = BTreeMap::new();
    let mut cell_count = 0;
    let mut supported_cells = BTreeSet::new();
    let mut unsupported_cells = BTreeSet::new();
    let expected_dialects = [
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
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let valid_statuses = ["supported", "unsupported", "unknown"]
        .into_iter()
        .collect::<BTreeSet<_>>();

    for command in commands {
        let path = command["path"].as_str().expect("command path");
        let category = command["category"].as_str().expect("command category");
        *category_counts.entry(category).or_insert(0) += 1;

        let support = command["support"].as_object().expect("support map");
        assert_eq!(
            support.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            expected_dialects,
            "dialect columns for {path}"
        );
        cell_count += support.len();

        for (dialect, status) in support {
            let status = status.as_str().expect("support status");
            assert!(
                valid_statuses.contains(status),
                "status for {path}/{dialect}"
            );
            match status {
                "supported" => {
                    supported_cells.insert(format!("{path}|{dialect}"));
                }
                "unsupported" => {
                    unsupported_cells.insert(format!("{path}|{dialect}"));
                }
                _ => {}
            }
        }
    }

    assert_eq!(
        category_counts,
        BTreeMap::from([
            ("format", 2),
            ("introspection", 146),
            ("semantic", 78),
            ("structural", 17),
        ])
    );
    assert_eq!(cell_count, 2430);
    assert_eq!(
        supported_cells,
        [
            "refactor inline-function|common-lisp",
            "refactor inline-function|emacs-lisp",
            "refactor inline-let|carp",
            "refactor inline-let|clojure",
            "refactor inline-let|common-lisp",
            "refactor inline-let|emacs-lisp",
            "refactor inline-let|fennel",
            "refactor inline-let|hy",
            "refactor inline-let|janet",
            "refactor inline-let|lfe",
            "refactor inline-let|racket",
            "refactor inline-let|scheme",
            "refactor rename-at|common-lisp",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        unsupported_cells,
        [
            "refactor inline-function|carp",
            "refactor inline-function|clojure",
            "refactor inline-function|fennel",
            "refactor inline-function|hy",
            "refactor inline-function|janet",
            "refactor inline-function|lfe",
            "refactor inline-function|racket",
            "refactor inline-function|scheme",
            "refactor rename-at|carp",
            "refactor rename-at|clojure",
            "refactor rename-at|emacs-lisp",
            "refactor rename-at|fennel",
            "refactor rename-at|hy",
            "refactor rename-at|janet",
            "refactor rename-at|lfe",
            "refactor rename-at|racket",
            "refactor rename-at|scheme",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
    );
}
