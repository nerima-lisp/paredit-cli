//! FEATURE-CANDIDATES.md section P4: `docs/src/commands.md` drifts from the
//! actual command surface silently, because nothing reads both. This asserts
//! every leaf command under `inspect`/`edit`/`refactor`/`config` — as
//! `paredit inspect capabilities` reports them, the same source of truth
//! `tests/cli/dialect_contract.rs` uses — has a row in that namespace's table
//! in `docs/src/commands.md`.

use std::collections::BTreeMap;

use super::*;

fn capabilities_json() -> serde_json::Value {
    let output = paredit()
        .args(["inspect", "capabilities"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("capabilities emits valid JSON")
}

/// Every leaf command name under `command` (its own name, if it has no
/// `commands` array of its own).
fn leaf_names(command: &serde_json::Value) -> Vec<String> {
    match command["commands"].as_array() {
        Some(subcommands) if !subcommands.is_empty() => {
            subcommands.iter().flat_map(leaf_names).collect()
        }
        _ => vec![command["name"].as_str().expect("command name").to_owned()],
    }
}

/// `docs/src/commands.md`'s `## <Heading>` sections, each mapped to the text
/// between it and the next `## ` heading (or end of file).
fn doc_sections() -> BTreeMap<String, String> {
    let text = std::fs::read_to_string("docs/src/commands.md").expect("read docs/src/commands.md");

    let headings: Vec<(usize, &str)> = text
        .lines()
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            Some((start, line))
        })
        .filter_map(|(start, line)| line.strip_prefix("## ").map(|heading| (start, heading)))
        .collect();

    let mut sections = BTreeMap::new();
    for (index, (start, heading)) in headings.iter().enumerate() {
        let body_start = start + 3 + heading.len();
        let body_end = headings
            .get(index + 1)
            .map_or(text.len(), |(next, _)| *next);
        sections.insert((*heading).to_owned(), text[body_start..body_end].to_owned());
    }
    sections
}

#[test]
fn every_inspect_edit_refactor_config_leaf_has_a_commands_md_row() {
    let capabilities = capabilities_json();
    let sections = doc_sections();

    // `lsp`/`mcp`/`serve` are documented in integrations.md instead (see
    // docs/src/architecture.md on why protocol servers bypass this dispatch),
    // and `completions` has no per-shell table to keep a row in sync with.
    let documented_namespaces = ["inspect", "edit", "refactor", "config"];

    let mut missing = Vec::new();
    for namespace in capabilities["commands"]
        .as_array()
        .expect("capabilities commands")
    {
        let namespace_name = namespace["name"].as_str().expect("namespace name");
        if !documented_namespaces.contains(&namespace_name) {
            continue;
        }
        let heading = titlecase(namespace_name);
        let section = sections
            .get(&heading)
            .unwrap_or_else(|| panic!("docs/src/commands.md has no `## {heading}` section"));

        for leaf in leaf_names(namespace) {
            let cell = format!("`{leaf}`");
            if !section.contains(&cell) {
                missing.push(format!("{namespace_name} {leaf}"));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "docs/src/commands.md is missing a row for: {missing:#?}\n\
         Add one to the matching table — see docs/src/development.md's \
         \"Documentation is tested\" section."
    );
}

fn titlecase(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
