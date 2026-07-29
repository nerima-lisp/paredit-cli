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
    report["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .find(|entry| entry["code"] == code)
        .unwrap_or_else(|| panic!("no {code} diagnostic in {report}"))
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
