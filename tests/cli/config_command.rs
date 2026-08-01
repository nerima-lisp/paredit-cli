//! `paredit config`, exercised through the real binary and a real filesystem.
//!
//! Discovery *is* the feature here — which files are found, in which order —
//! so these run against actual directories rather than a stubbed layer stack.
//! `--from` lets each case pick its own start directory without the tests
//! having to change the process's working directory, which they share.

use super::*;

use std::collections::BTreeSet;
use std::path::Path;

/// A repository-shaped scratch directory: a `.git` marker so discovery has a
/// root to stop at, and nothing else until a test writes it.
fn repo(name: &str) -> PathBuf {
    let root = fresh_temp_dir(name);
    fs::create_dir_all(root.join(".git")).expect("create .git marker");
    root
}

fn write(root: &Path, relative: &str, contents: &str) -> PathBuf {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(&path, contents).expect("write config");
    path
}

/// Runs a config subcommand rooted at `start`, with the ambient environment
/// stripped so a developer's own `PAREDIT_*` cannot change the result.
fn config(start: &Path, args: &[&str]) -> Command {
    let mut command = paredit();
    command
        .arg("config")
        .args(args)
        .args(["--from", &start.display().to_string()])
        .env_remove("PAREDIT_CONFIG_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", start.display().to_string());
    command
}

fn json_of(mut command: Command) -> serde_json::Value {
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("config emits valid JSON")
}

#[test]
fn show_reports_the_built_in_defaults_when_nothing_is_configured() {
    let root = repo("config-defaults");
    let report = json_of(config(&root, &["show"]));

    assert_eq!(report["source_count"], 0);
    let indent = setting(&report, "format.indent");
    assert_eq!(indent["value"], 2);
    assert_eq!(indent["origin"]["layer"], "default");
}

#[test]
fn show_names_the_file_and_line_that_set_each_key() {
    let root = repo("config-provenance");
    write(&root, "paredit.toml", "[format]\nindent = 4\n");

    let report = json_of(config(&root, &["show"]));
    let indent = setting(&report, "format.indent");

    assert_eq!(indent["value"], 4);
    assert_eq!(indent["origin"]["layer"], "repository");
    assert_eq!(indent["origin"]["line"], 2);
    assert!(
        indent["origin"]["path"]
            .as_str()
            .expect("origin path")
            .ends_with("paredit.toml")
    );
}

/// The layering H2 asks for, end to end: a nested file replaces one key and
/// leaves the rest of the repository's file alone.
#[test]
fn a_nested_directory_file_overrides_only_the_keys_it_sets() {
    let root = repo("config-layers");
    write(
        &root,
        "paredit.toml",
        "[format]\nindent = 4\n\n[lint]\npreset = \"all\"\n",
    );
    write(&root, "src/paredit.toml", "[format]\nindent = 8\n");

    let report = json_of(config(&root.join("src"), &["show"]));

    assert_eq!(setting(&report, "format.indent")["value"], 8);
    assert_eq!(
        setting(&report, "format.indent")["origin"]["layer"],
        "directory"
    );
    assert_eq!(setting(&report, "lint.preset")["value"], "all");
    assert_eq!(
        setting(&report, "lint.preset")["origin"]["layer"],
        "repository"
    );
}

#[test]
fn extends_is_applied_beneath_the_file_that_names_it() {
    let root = repo("config-extends");
    write(
        &root,
        "shared.toml",
        "[format]\nindent = 4\n[lint]\npreset = \"all\"\n",
    );
    write(
        &root,
        "paredit.toml",
        "extends = [\"shared.toml\"]\n[format]\nindent = 8\n",
    );

    let report = json_of(config(&root, &["show"]));
    assert_eq!(setting(&report, "format.indent")["value"], 8);
    assert_eq!(setting(&report, "lint.preset")["value"], "all");

    let sources = report["sources"].as_array().expect("sources");
    assert_eq!(sources.len(), 2);
    assert!(sources[0]["extended_by"].is_string(), "{sources:?}");
    assert!(sources[1]["extended_by"].is_null(), "{sources:?}");
}

#[test]
fn an_extends_cycle_fails_rather_than_hanging() {
    let root = repo("config-cycle");
    write(&root, "paredit.toml", "extends = [\"b.toml\"]\n");
    write(&root, "b.toml", "extends = [\"paredit.toml\"]\n");

    config(&root, &["check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("extends itself"));
}

#[test]
fn check_passes_a_clean_configuration() {
    let root = repo("config-check-ok");
    write(&root, "paredit.toml", "[lint]\nfail-on = \"error\"\n");

    let report = json_of(config(&root, &["check"]));
    assert_eq!(report["status"], "ok");
    assert_eq!(report["error_count"], 0);
}

/// Every problem is reported, not just the first: whoever is fixing the file
/// should get one list rather than four runs.
#[test]
fn check_reports_every_problem_at_once_with_its_line() {
    let root = repo("config-check-bad");
    write(
        &root,
        "paredit.toml",
        "[format]\nindent = \"four\"\n\n[lint]\npreset = \"everything\"\nfail_on = \"error\"\n",
    );

    let output = config(&root, &["check"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");

    assert_eq!(report["status"], "error");
    assert_eq!(report["error_count"], 3);

    let codes = report["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|entry| entry["code"].as_str().expect("code").to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        codes,
        ["not-a-choice", "unknown-key", "wrong-type"]
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );

    let unknown = diagnostic(&report, "unknown-key");
    assert_eq!(unknown["line"], 6);
    assert_eq!(unknown["suggestion"], "did you mean `lint.fail-on`?");
}

/// The check that only a real registry can do: a rule name that does not
/// exist in *this* build is caught here rather than silently linting less
/// than the file asked for.
#[test]
fn check_validates_rule_names_against_the_registry() {
    let root = repo("config-rule-names");
    write(
        &root,
        "paredit.toml",
        "[lint]\ndisable = [\"nil-comparson\"]\n",
    );

    let output = config(&root, &["check"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    let finding = diagnostic(&report, "unknown-rule");
    assert_eq!(finding["suggestion"], "did you mean \"nil-comparison\"?");
}

#[test]
fn a_registered_rule_name_is_accepted() {
    let root = repo("config-rule-ok");
    write(
        &root,
        "paredit.toml",
        "[lint]\ndisable = [\"nil-comparison\"]\n",
    );
    let report = json_of(config(&root, &["check"]));
    assert_eq!(report["status"], "ok");
}

/// A custom rule's own `:severity` disagreeing with the `paredit.toml` list
/// that also names it is a conflict, reported the same way as `lint.deny`
/// and `lint.warn` overlapping.
#[test]
fn check_reports_a_custom_rule_whose_severity_disagrees_with_lint_warn() {
    let root = repo("config-custom-severity-conflict");
    write(
        &root,
        ".paredit/rules/house.lisp",
        "(defrule house-style :severity error :pattern (print ?x) :message \"no print\")\n",
    );
    write(
        &root,
        "paredit.toml",
        "[lint]\ncustom-rules = \".paredit/rules\"\nwarn = [\"house-style\"]\n",
    );

    let output = config(&root, &["check"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");

    assert_eq!(report["status"], "error");
    let found = diagnostic(&report, "conflict");
    assert_eq!(found["key"], "lint.warn");
    assert!(
        found["message"]
            .as_str()
            .expect("message")
            .contains("house-style"),
        "{found}"
    );
}

/// The same rule, agreeing with `lint.deny` this time, is not a conflict.
///
/// `lint.deny`/`lint.warn` are separately validated against the *shipped*
/// rule catalogue (`check_list` in `paredit-core-config`'s `settings.rs`),
/// which has no notion of a custom rule's name — that gate is unrelated to
/// this cross-check and out of scope here, so `"house-style"` still earns an
/// `unknown-rule` diagnostic of its own. What this test asserts is narrower
/// and exactly what Part A owns: no `conflict` diagnostic, because the
/// severities agree.
#[test]
fn check_reports_no_conflict_when_a_custom_rules_severity_agrees_with_lint_deny() {
    let root = repo("config-custom-severity-ok");
    write(
        &root,
        ".paredit/rules/house.lisp",
        "(defrule house-style :severity error :pattern (print ?x) :message \"no print\")\n",
    );
    write(
        &root,
        "paredit.toml",
        "[lint]\ncustom-rules = \".paredit/rules\"\ndeny = [\"house-style\"]\n",
    );

    let output = config(&root, &["check"])
        .assert()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert!(find_diagnostic(&report, "conflict").is_none(), "{report}");
}

/// A rule this cross-check never heard of, because it names neither list, is
/// not an error just for existing.
#[test]
fn check_does_not_flag_a_custom_rule_that_neither_list_names() {
    let root = repo("config-custom-severity-unreferenced");
    write(
        &root,
        ".paredit/rules/house.lisp",
        "(defrule house-style :severity error :pattern (print ?x) :message \"no print\")\n",
    );
    write(
        &root,
        "paredit.toml",
        "[lint]\ncustom-rules = \".paredit/rules\"\n",
    );

    let report = json_of(config(&root, &["check"]));
    assert_eq!(report["status"], "ok");
    assert!(find_diagnostic(&report, "conflict").is_none(), "{report}");
}

/// A rule file `config check` cannot parse must not take the whole command
/// down with it, and must not be treated as agreeing or conflicting with
/// anything — it contributes no `conflict` diagnostic either way.
#[test]
fn check_survives_a_malformed_custom_rule_file() {
    let root = repo("config-custom-severity-malformed");
    write(
        &root,
        ".paredit/rules/broken.lisp",
        "(defrule (((( broken\n",
    );
    write(
        &root,
        "paredit.toml",
        "[lint]\ncustom-rules = \".paredit/rules\"\nwarn = [\"house-style\"]\n",
    );

    let output = config(&root, &["check"])
        .assert()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert!(find_diagnostic(&report, "conflict").is_none(), "{report}");
}

#[test]
fn a_syntax_error_names_the_file_and_line_and_refuses_to_load() {
    let root = repo("config-syntax");
    write(&root, "paredit.toml", "[format]\nindent = 4\nbroken\n");

    config(&root, &["check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("line 3"));
}

/// A rejected value must leave the layer below it in force. Applying half a
/// configuration is worse than applying none of it.
#[test]
fn a_rejected_value_does_not_replace_the_one_below_it() {
    let root = repo("config-rejected");
    write(&root, "paredit.toml", "[format]\nindent = 99\n");

    let output = config(&root, &["show"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");

    assert_eq!(setting(&report, "format.indent")["value"], 2);
    assert_eq!(
        setting(&report, "format.indent")["origin"]["layer"],
        "default"
    );
    assert_eq!(diagnostic(&report, "out-of-range")["line"], 2);
}

#[test]
fn an_environment_variable_beats_every_file() {
    let root = repo("config-env");
    write(&root, "paredit.toml", "[format]\nindent = 4\n");

    let mut command = config(&root, &["show", "--key", "format.indent"]);
    command.env("PAREDIT_FORMAT_INDENT", "7");
    let report = json_of(command);

    assert_eq!(setting(&report, "format.indent")["value"], 7);
    assert_eq!(
        setting(&report, "format.indent")["origin"]["layer"],
        "environment"
    );
    assert_eq!(
        setting(&report, "format.indent")["origin"]["detail"],
        "PAREDIT_FORMAT_INDENT"
    );
}

#[test]
fn no_config_env_leaves_the_files_in_charge() {
    let root = repo("config-no-env");
    write(&root, "paredit.toml", "[format]\nindent = 4\n");

    let mut command = config(&root, &["show", "--no-config-env"]);
    command.env("PAREDIT_FORMAT_INDENT", "7");
    assert_eq!(setting(&json_of(command), "format.indent")["value"], 4);
}

#[test]
fn no_config_reads_nothing_at_all() {
    let root = repo("config-none");
    write(&root, "paredit.toml", "[format]\nindent = 4\n");

    let report = json_of(config(&root, &["show", "--no-config"]));
    assert_eq!(report["source_count"], 0);
    assert_eq!(setting(&report, "format.indent")["value"], 2);
}

#[test]
fn an_explicit_config_replaces_discovery() {
    let root = repo("config-explicit");
    write(&root, "paredit.toml", "[format]\nindent = 4\n");
    let named = write(&root, "other.toml", "[lint]\npreset = \"all\"\n");

    let report = json_of(config(
        &root,
        &["show", "--config", &named.display().to_string()],
    ));
    assert_eq!(report["source_count"], 1);
    assert_eq!(setting(&report, "format.indent")["value"], 2);
    assert_eq!(setting(&report, "lint.preset")["value"], "all");
}

#[test]
fn changed_only_hides_the_keys_no_layer_set() {
    let root = repo("config-changed-only");
    write(&root, "paredit.toml", "[format]\nindent = 4\n");

    let report = json_of(config(&root, &["show", "--changed-only"]));
    let keys = report["settings"]
        .as_array()
        .expect("settings")
        .iter()
        .map(|entry| entry["key"].as_str().expect("key").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(keys, vec!["format.indent".to_owned()]);
}

#[test]
fn show_rejects_a_key_that_is_not_in_the_schema() {
    let root = repo("config-bad-key");
    config(&root, &["show", "--key", "format.indnt"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("did you mean `format.indent`?"));
}

/// `config schema` is the discovery surface for agents, so its shape is
/// checked rather than assumed: every key must carry the four fields an agent
/// needs to construct a valid value without a second call.
#[test]
fn schema_describes_every_key_completely() {
    let output = paredit()
        .args(["config", "schema"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");

    let keys = report["keys"].as_array().expect("keys");
    assert_eq!(
        keys.len(),
        report["key_count"].as_u64().expect("count") as usize
    );
    assert!(!keys.is_empty());

    for key in keys {
        let name = key["key"].as_str().expect("key name");
        assert!(key["type"].is_string(), "{name} has no type");
        assert!(key["env"].is_string(), "{name} has no environment variable");
        assert!(
            key["summary"]
                .as_str()
                .is_some_and(|text| text.ends_with('.')),
            "{name} has no summary sentence"
        );
        assert!(
            key["env"].as_str().expect("env").starts_with("PAREDIT_"),
            "{name}'s variable is not namespaced"
        );
    }
}

#[test]
fn init_writes_a_file_that_check_then_accepts() {
    let root = repo("config-init");

    paredit()
        .args(["config", "init", "--path"])
        .arg(root.join("paredit.toml"))
        .assert()
        .success();

    assert!(root.join("paredit.toml").is_file());
    let report = json_of(config(&root, &["check"]));
    assert_eq!(report["status"], "ok");
}

#[test]
fn init_refuses_to_overwrite_an_existing_file() {
    let root = repo("config-init-clobber");
    write(&root, "paredit.toml", "[format]\nindent = 4\n");

    paredit()
        .args(["config", "init", "--path"])
        .arg(root.join("paredit.toml"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to overwrite"));

    // The refusal has to be total: the original must still be readable.
    assert_eq!(
        fs::read_to_string(root.join("paredit.toml")).expect("read"),
        "[format]\nindent = 4\n"
    );
}

#[test]
fn init_dry_run_writes_nothing() {
    let root = repo("config-init-dry");

    paredit()
        .args(["config", "init", "--dry-run", "--path"])
        .arg(root.join("paredit.toml"))
        .assert()
        .success()
        .stdout(predicate::str::contains("paredit configuration"));

    assert!(!root.join("paredit.toml").exists());
}

#[test]
fn text_output_is_tab_separated_like_every_other_report() {
    let root = repo("config-text");
    write(&root, "paredit.toml", "[format]\nindent = 4\n");

    config(&root, &["show", "--changed-only", "--output", "text"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("format.indent\t4\tdirectory")
                .or(predicate::str::contains("format.indent\t4\trepository")),
        );
}

fn setting<'a>(report: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    report["settings"]
        .as_array()
        .expect("settings")
        .iter()
        .find(|entry| entry["key"] == key)
        .unwrap_or_else(|| panic!("{key} is missing from the report"))
}

fn diagnostic<'a>(report: &'a serde_json::Value, code: &str) -> &'a serde_json::Value {
    find_diagnostic(report, code).unwrap_or_else(|| panic!("no {code} diagnostic in {report}"))
}

fn find_diagnostic<'a>(report: &'a serde_json::Value, code: &str) -> Option<&'a serde_json::Value> {
    report["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .find(|entry| entry["code"] == code)
}

// --- The configuration taking effect, rather than merely being reported. ---
//
// `config show` proving a key was read is not the same claim as a command
// behaving differently because of it. These run real commands.

/// Written to `paredit()` rather than `config()`: these invoke ordinary
/// commands, which have no `--from`, so the working directory does the
/// discovery.
fn in_repo(root: &Path, args: &[&str]) -> Command {
    let mut command = paredit();
    command
        .current_dir(root)
        .args(args)
        .env_remove("PAREDIT_CONFIG_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", root.display().to_string());
    command
}

#[test]
fn a_disabled_rule_stops_being_reported_by_lint() {
    let root = repo("config-effect-lint");
    write(&root, "a.lisp", "(defun f (x) (if (eq x nil) 1 2))\n");

    let before = in_repo(&root, &["inspect", "lint", "a.lisp"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let before: serde_json::Value = serde_json::from_slice(&before).expect("valid JSON");
    assert_eq!(before["finding_count"], 1);

    write(
        &root,
        "paredit.toml",
        "[lint]\ndisable = [\"nil-comparison\"]\n",
    );

    let after = in_repo(&root, &["inspect", "lint", "a.lisp"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let after: serde_json::Value = serde_json::from_slice(&after).expect("valid JSON");
    assert_eq!(after["finding_count"], 0);
}

// --- FR-E16: `lint.enable`/`lint.disable`/`lint.deny`/`lint.warn` also
// recognise a project's own loaded custom rules, not just the shipped
// catalogue. ---

/// A `.paredit/rules/*.lisp` file defining one rule, at the default directory
/// `custom::load` reads when nothing overrides it.
fn write_custom_rule(root: &Path, rule: &str) {
    write(root, ".paredit/rules/house.lisp", rule);
}

/// The literal scenario FR-E16 exists for: `lint.deny` naming a rule defined
/// by a loaded `.paredit/rules/*.lisp` file promotes it to error severity, and
/// that gates the run — observable via exit code — exactly the way naming a
/// shipped rule already does.
///
/// `--fail-on error` is passed as a plain flag rather than through
/// `paredit.toml`, so the JSON assertion below can run against a command that
/// is expected to *succeed* (a report is still printed on a failing gate, but
/// `json_of` asserts success — the exit-code assertion is the separate,
/// second command).
#[test]
fn a_custom_rule_named_in_lint_deny_gates_the_run() {
    let root = repo("config-effect-lint-deny-custom");
    write_custom_rule(
        &root,
        r#"(defrule my-custom-rule :pattern (print ?x) :message "m")"#,
    );
    write(&root, "a.lisp", "(print 1)\n");
    write(
        &root,
        "paredit.toml",
        "[lint]\ndeny = [\"my-custom-rule\"]\n",
    );

    // The custom rule ships at warning severity; `lint.deny` promotes it.
    let value = json_of(in_repo(&root, &["inspect", "lint", "a.lisp"]));
    assert_eq!(value["finding_count"], 1);
    assert_eq!(value["findings"][0]["rule"], "my-custom-rule");
    assert_eq!(value["findings"][0]["severity"], "error");

    // And that promotion is what turns `--fail-on error` into a failing exit
    // code — the whole point of extending `lint.deny` to a custom rule.
    in_repo(&root, &["inspect", "lint", "--fail-on", "error", "a.lisp"])
        .assert()
        .failure();
}

/// Without `lint.deny`, the same custom rule reports at its own (warning)
/// severity and does not gate an `error`-only run — the control case that
/// proves the failure above comes from `lint.deny`, not from the custom rule
/// itself.
#[test]
fn without_lint_deny_the_same_custom_rule_does_not_gate_an_error_only_run() {
    let root = repo("config-effect-lint-deny-custom-control");
    write_custom_rule(
        &root,
        r#"(defrule my-custom-rule :pattern (print ?x) :message "m")"#,
    );
    write(&root, "a.lisp", "(print 1)\n");

    let value = json_of(in_repo(&root, &["inspect", "lint", "a.lisp"]));
    assert_eq!(value["findings"][0]["severity"], "warning");

    in_repo(&root, &["inspect", "lint", "--fail-on", "error", "a.lisp"])
        .assert()
        .success();
}

/// `lint.disable` naming a custom rule works the same way it does for a
/// shipped one (mirrors `a_disabled_rule_stops_being_reported_by_lint`).
#[test]
fn lint_disable_stops_a_custom_rule_from_being_reported() {
    let root = repo("config-effect-lint-disable-custom");
    write_custom_rule(
        &root,
        r#"(defrule my-custom-rule :pattern (print ?x) :message "m")"#,
    );
    write(&root, "a.lisp", "(print 1)\n");

    let before = json_of(in_repo(&root, &["inspect", "lint", "a.lisp"]));
    assert_eq!(before["finding_count"], 1);

    write(
        &root,
        "paredit.toml",
        "[lint]\ndisable = [\"my-custom-rule\"]\n",
    );

    let after = json_of(in_repo(&root, &["inspect", "lint", "a.lisp"]));
    assert_eq!(after["finding_count"], 0);
}

/// `lint.enable` naming only a custom rule runs just that rule — a shipped
/// finding elsewhere in the same file is dropped, matching what `--rule` does
/// for a shipped name.
#[test]
fn lint_enable_can_select_only_a_custom_rule() {
    let root = repo("config-effect-lint-enable-custom");
    write_custom_rule(
        &root,
        r#"(defrule my-custom-rule :pattern (print ?x) :message "m")"#,
    );
    // `(list '5)` also trips the shipped `redundant-quote` rule.
    write(&root, "a.lisp", "(print 1)\n(list '5)\n");

    let value = json_of(in_repo(&root, &["inspect", "lint", "a.lisp"]));
    let rules: BTreeSet<&str> = value["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| finding["rule"].as_str().expect("rule"))
        .collect();
    assert!(rules.contains("my-custom-rule"));
    assert!(rules.contains("redundant-quote"));

    write(
        &root,
        "paredit.toml",
        "[lint]\nenable = [\"my-custom-rule\"]\n",
    );

    let value = json_of(in_repo(&root, &["inspect", "lint", "a.lisp"]));
    assert_eq!(value["finding_count"], 1);
    assert_eq!(value["findings"][0]["rule"], "my-custom-rule");
}

/// A name that matches neither the shipped catalogue nor any loaded custom
/// rule is still rejected outright by `config check` — FR-E16 widens what is
/// *accepted*, not what is validated away.
#[test]
fn lint_deny_still_rejects_a_name_matching_nothing_loaded() {
    let root = repo("config-effect-lint-deny-unknown");
    write_custom_rule(
        &root,
        r#"(defrule my-custom-rule :pattern (print ?x) :message "m")"#,
    );
    write(
        &root,
        "paredit.toml",
        "[lint]\ndeny = [\"not-a-real-rule\"]\n",
    );

    let output = config(&root, &["check"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    let finding = diagnostic(&report, "unknown-rule");
    assert!(
        finding["message"]
            .as_str()
            .unwrap_or_default()
            .contains("not-a-real-rule"),
        "{finding}"
    );
}

/// The other half of the same guarantee: naming a *loaded* custom rule in
/// `lint.deny` is exactly as valid as naming a shipped one — `config check`
/// reports no error for it.
#[test]
fn config_check_accepts_a_loaded_custom_rule_name_in_lint_deny() {
    let root = repo("config-check-custom-rule-name");
    write_custom_rule(
        &root,
        r#"(defrule my-custom-rule :pattern (print ?x) :message "m")"#,
    );
    write(
        &root,
        "paredit.toml",
        "[lint]\ndeny = [\"my-custom-rule\"]\n",
    );

    // `config check` reads `.paredit/rules` relative to the process's working
    // directory, exactly as `inspect lint` itself does — so it must run from
    // `root`, not merely be pointed at it with `--from`.
    let output = paredit()
        .current_dir(&root)
        .args(["config", "check"])
        .env_remove("PAREDIT_CONFIG_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", root.display().to_string())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["error_count"], 0);
}

/// The rule the whole bridge is built around.
#[test]
fn a_flag_still_beats_the_configuration() {
    let root = repo("config-flag-wins");
    write(&root, "a.lisp", "(defun f (x)\n(list x))\n");
    write(&root, "paredit.toml", "[format]\nindent = 8\n");

    let configured = in_repo(&root, &["edit", "format", "--file", "a.lisp"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        String::from_utf8_lossy(&configured).contains("\n        "),
        "the configured indent of 8 was not applied"
    );

    let flagged = in_repo(
        &root,
        &["edit", "format", "--file", "a.lisp", "--indent", "1"],
    )
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let flagged = String::from_utf8_lossy(&flagged);
    assert!(flagged.contains("\n "), "{flagged:?}");
    assert!(!flagged.contains("\n        "), "{flagged:?}");
}

/// `[dialect]` acts below the argument layer, so it is checked through a
/// command's own answer rather than through an injected flag.
#[test]
fn a_forced_dialect_overrides_extension_detection() {
    let root = repo("config-dialect-force");
    write(&root, "a.el", "(defun f () nil)\n");

    let detected = in_repo(
        &root,
        &["inspect", "dialect", "--file", "a.el", "--output", "json"],
    )
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let detected: serde_json::Value = serde_json::from_slice(&detected).expect("valid JSON");
    assert_eq!(detected["dialect"], "emacs-lisp");

    write(
        &root,
        "paredit.toml",
        "[dialect]\ndefault = \"common-lisp\"\nforce = true\n",
    );

    let forced = in_repo(
        &root,
        &["inspect", "dialect", "--file", "a.el", "--output", "json"],
    )
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let forced: serde_json::Value = serde_json::from_slice(&forced).expect("valid JSON");
    assert_eq!(forced["dialect"], "common-lisp");
}

/// Without `force`, a configured dialect is a fallback: a recognised
/// extension still wins, and only a file detection could not place picks it up.
#[test]
fn an_unforced_dialect_only_fills_in_for_an_undetected_file() {
    let root = repo("config-dialect-default");
    write(&root, "a.el", "(defun f () nil)\n");
    write(&root, "script", "(defun f () nil)\n");
    write(
        &root,
        "paredit.toml",
        "[dialect]\ndefault = \"common-lisp\"\n",
    );

    for (file, expected) in [("a.el", "emacs-lisp"), ("script", "common-lisp")] {
        let report = in_repo(
            &root,
            &["inspect", "dialect", "--file", file, "--output", "json"],
        )
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
        let report: serde_json::Value = serde_json::from_slice(&report).expect("valid JSON");
        assert_eq!(report["dialect"], expected, "for {file}");
    }
}

/// An explicit `--dialect` outranks even a forced configuration: it is the
/// most specific thing anyone said.
#[test]
fn an_explicit_dialect_flag_outranks_a_forced_configuration() {
    let root = repo("config-dialect-flag");
    write(&root, "a.el", "(defun f () nil)\n");
    write(
        &root,
        "paredit.toml",
        "[dialect]\ndefault = \"common-lisp\"\nforce = true\n",
    );

    let report = in_repo(
        &root,
        &[
            "inspect",
            "dialect",
            "--file",
            "a.el",
            "--dialect",
            "racket",
            "--output",
            "json",
        ],
    )
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let report: serde_json::Value = serde_json::from_slice(&report).expect("valid JSON");
    assert_eq!(report["dialect"], "racket");
}

/// A broken file must not become a broken tool. The command still runs; the
/// warning points at where the problem is diagnosed properly.
#[test]
fn a_configuration_with_errors_is_skipped_rather_than_fatal() {
    let root = repo("config-broken-skipped");
    write(&root, "a.lisp", "(defun f (x) x)\n");
    write(&root, "paredit.toml", "[lint]\npreset = \"everything\"\n");

    in_repo(&root, &["inspect", "lint", "a.lisp"])
        .assert()
        .success()
        .stderr(predicate::str::contains("paredit config check"));
}

#[test]
fn a_configuration_that_cannot_be_parsed_is_skipped_rather_than_fatal() {
    let root = repo("config-unparsable-skipped");
    write(&root, "a.lisp", "(defun f (x) x)\n");
    write(&root, "paredit.toml", "this is not toml\n");

    in_repo(&root, &["inspect", "lint", "a.lisp"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ignoring the configuration"));
}

/// `inspect lint` defaults to `--output json` with no flag typed, so a
/// caller reading only structured stderr should not have to fall back to
/// scraping English out of a `Warning: ` line to learn the configuration
/// was ignored — the same contract `report_failure` already keeps for
/// errors extends to this warning.
#[test]
fn a_skipped_configuration_is_a_json_warning_when_the_command_defaults_to_json() {
    let root = repo("config-unparsable-json-warning");
    write(&root, "a.lisp", "(defun f (x) x)\n");
    write(&root, "paredit.toml", "this is not toml\n");

    let stderr = in_repo(&root, &["inspect", "lint", "a.lisp"])
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();
    let text = String::from_utf8(stderr).expect("UTF-8");
    let warning: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text:?}"));
    assert_eq!(warning["status"], "warning");
    assert_eq!(warning["command"], "inspect lint");
    assert!(
        warning["warning"]["message"]
            .as_str()
            .expect("message")
            .contains("ignoring the configuration"),
        "{warning}"
    );
}

#[test]
fn the_no_config_variable_turns_the_whole_thing_off() {
    let root = repo("config-env-off");
    write(&root, "a.lisp", "(defun f (x) (if (eq x nil) 1 2))\n");
    write(
        &root,
        "paredit.toml",
        "[lint]\ndisable = [\"nil-comparison\"]\n",
    );

    let output = in_repo(&root, &["inspect", "lint", "a.lisp"])
        .env("PAREDIT_NO_CONFIG", "1")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(report["finding_count"], 1);
}

#[test]
fn show_for_reports_the_flags_a_configuration_would_add_to_one_command() {
    let root = repo("config-show-for");
    write(
        &root,
        "paredit.toml",
        "[lint]\ndisable = [\"nil-comparison\"]\n",
    );

    let report = json_of(config(&root, &["show", "--for", "inspect lint"]));
    let injections = report["injections"].as_array().expect("injections");
    assert_eq!(injections.len(), 1);
    assert_eq!(injections[0]["key"], "lint.disable");
    assert_eq!(injections[0]["flag"], "--exclude");
    assert_eq!(injections[0]["values"][0], "nil-comparison");
}

#[test]
fn show_for_rejects_something_that_is_not_a_command() {
    let root = repo("config-show-for-bad");
    config(&root, &["show", "--for", "inspect nonsense"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not a command"));
}

#[test]
fn show_without_for_reports_no_injections_at_all() {
    let root = repo("config-show-no-for");
    let report = json_of(config(&root, &["show"]));
    assert!(report["injections"].is_null());
}

// --- FR-012: `--cache-dir` config layering (arg > env > paredit.toml > no cache). ---
//
// `inspect sources` is the target rather than `inspect lint`: it reports its
// `WorkspaceInputArgs` discovery-cache outcome as a `cache` field in its own
// JSON (`missing`/`hit`/`stale`/`unusable`, or absent when no cache was
// resolved at all), which is a direct, unambiguous read of exactly the cache
// FR-012 configures. `inspect lint --cache-dir` looks identical but is a
// different flag — a per-file analysis cache — so it is deliberately not used
// here; see `cache_dir_does_not_reach_lints_unrelated_cache_dir_flag` in
// `config_bridge.rs` for that boundary.
//
// Cache directories are independent `fresh_temp_dir`s rather than paths under
// `root`: a cache directory created *inside* the scanned root invalidates its
// own first entry (creating it changes the root's mtime/entry count after the
// walk recorded them), which would make "was the cache resolved at all" and
// "did the first lookup already go stale" the same observation.

fn source_cache_outcome(root: &Path, extra: &[&str]) -> Option<String> {
    let mut command = in_repo(root, &["inspect", "sources", "."]);
    command.args(extra);
    let output = command.assert().success().get_output().stdout.clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    report["cache"].as_str().map(str::to_owned)
}

#[test]
fn with_nothing_set_no_cache_is_resolved_at_all() {
    let root = repo("cache-dir-default");
    write(&root, "a.lisp", "(defun f (x) (+ x 1))\n");

    assert_eq!(
        source_cache_outcome(&root, &[]),
        None,
        "no --cache-dir, PAREDIT_CACHE_DIR, or cache.dir means no cache at all"
    );
}

#[test]
fn the_environment_variable_activates_the_cache_with_no_flag() {
    let root = repo("cache-dir-env");
    write(&root, "a.lisp", "(defun f (x) (+ x 1))\n");
    let cache = fresh_temp_dir("cache-dir-env-store");

    let mut command = in_repo(&root, &["inspect", "sources", "."]);
    command.env("PAREDIT_CACHE_DIR", cache.display().to_string());
    let output = command.assert().success().get_output().stdout.clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");

    assert_eq!(
        report["cache"], "missing",
        "PAREDIT_CACHE_DIR alone must turn the discovery cache on: {report}"
    );
    assert!(
        fs::read_dir(&cache).is_ok_and(|mut entries| entries.next().is_some()),
        "the environment-selected directory must receive a cache entry"
    );
}

#[test]
fn a_configured_cache_dir_activates_the_cache_with_no_flag_or_variable() {
    let root = repo("cache-dir-config");
    write(&root, "a.lisp", "(defun f (x) (+ x 1))\n");
    let cache = fresh_temp_dir("cache-dir-config-store");
    // Absolute, so it resolves the same way regardless of where `paredit.toml`
    // sits; the relative case is covered separately below through `config show`.
    write(
        &root,
        "paredit.toml",
        &format!("[cache]\ndir = {:?}\n", cache.display().to_string()),
    );

    assert_eq!(
        source_cache_outcome(&root, &[]),
        Some("missing".to_owned()),
        "a configured cache.dir alone must turn the discovery cache on"
    );
}

/// The precedence FR-012 asks for, checked end to end against the running
/// binary: an explicit `--cache-dir` outranks both a `paredit.toml` and
/// `PAREDIT_CACHE_DIR`. Proven by pointing the flag at a directory that does
/// not exist as a cache-shaped directory yet and confirming *that* one — and
/// not the other two — receives the entry.
#[test]
fn an_explicit_flag_outranks_both_the_variable_and_the_configuration() {
    let root = repo("cache-dir-precedence");
    write(&root, "a.lisp", "(defun f (x) (+ x 1))\n");
    let from_config = fresh_temp_dir("cache-dir-precedence-config");
    let from_env = fresh_temp_dir("cache-dir-precedence-env");
    let from_flag = fresh_temp_dir("cache-dir-precedence-flag");
    write(
        &root,
        "paredit.toml",
        &format!("[cache]\ndir = {:?}\n", from_config.display().to_string()),
    );

    let mut command = in_repo(&root, &["inspect", "sources", "."]);
    command
        .env("PAREDIT_CACHE_DIR", from_env.display().to_string())
        .arg("--cache-dir")
        .arg(&from_flag);
    let output = command.assert().success().get_output().stdout.clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");

    assert_eq!(
        report["cache"], "missing",
        "the explicit flag must win: {report}"
    );
    assert!(
        fs::read_dir(&from_flag).is_ok_and(|mut entries| entries.next().is_some()),
        "the explicit flag's directory must receive the cache entry"
    );
    for untouched in [&from_config, &from_env] {
        assert!(
            fs::read_dir(untouched).is_ok_and(|mut entries| entries.next().is_none()),
            "{untouched:?} must not be touched once a flag is given"
        );
    }
}

/// The other half of the same precedence: with no flag, the environment beats
/// the file, exactly as it does for every other key (see
/// `an_environment_variable_beats_every_file` above, for `format.indent`).
#[test]
fn the_environment_variable_outranks_a_configured_cache_dir() {
    let root = repo("cache-dir-env-beats-config");
    write(&root, "a.lisp", "(defun f (x) (+ x 1))\n");
    let from_config = fresh_temp_dir("cache-dir-env-beats-config-config");
    let from_env = fresh_temp_dir("cache-dir-env-beats-config-env");
    write(
        &root,
        "paredit.toml",
        &format!("[cache]\ndir = {:?}\n", from_config.display().to_string()),
    );

    let mut command = in_repo(&root, &["inspect", "sources", "."]);
    command.env("PAREDIT_CACHE_DIR", from_env.display().to_string());
    let output = command.assert().success().get_output().stdout.clone();
    let report: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");

    assert_eq!(
        report["cache"], "missing",
        "the environment must win over the file: {report}"
    );
    assert!(
        fs::read_dir(&from_env).is_ok_and(|mut entries| entries.next().is_some()),
        "the environment directory must receive the cache entry"
    );
    assert!(
        fs::read_dir(&from_config).is_ok_and(|mut entries| entries.next().is_none()),
        "the outranked configuration directory must not be touched"
    );
}

/// `config show` proves the layer, independent of any command's behaviour —
/// the same shape as `an_environment_variable_beats_every_file` above.
#[test]
fn config_show_reports_the_environment_layer_for_cache_dir() {
    let root = repo("cache-dir-show-env");
    write(&root, "paredit.toml", "[cache]\ndir = \"from-config\"\n");

    let mut command = config(&root, &["show", "--key", "cache.dir"]);
    command.env("PAREDIT_CACHE_DIR", "/from/env");
    let report = json_of(command);

    assert_eq!(setting(&report, "cache.dir")["value"], "/from/env");
    assert_eq!(
        setting(&report, "cache.dir")["origin"]["layer"],
        "environment"
    );
}

/// `config show` proves the file layer resolves relative to the file that set
/// it, just like `lint.baseline` and `paths.exclude` already do.
#[test]
fn config_show_resolves_a_configured_cache_dir_relative_to_its_file() {
    let root = repo("cache-dir-show-config");
    write(&root, "nested/paredit.toml", "[cache]\ndir = \"store\"\n");

    let report = json_of(config(
        &root.join("nested"),
        &["show", "--key", "cache.dir"],
    ));
    let value = setting(&report, "cache.dir")["value"]
        .as_str()
        .expect("cache.dir value");
    assert!(
        value.ends_with("nested/store"),
        "expected a path under nested/, got {value}"
    );
}
