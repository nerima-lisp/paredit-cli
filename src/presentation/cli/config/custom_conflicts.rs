//! Cross-checking a custom rule's own `:severity` against `lint.warn`/`lint.deny`.
//!
//! `paredit-core-config` cannot see `paredit-feature-lint-custom`: a core
//! package depending on a feature package would invert the boundary
//! `architecture_contract.rs::core_packages_never_depend_on_a_feature`
//! enforces, so `check_conflicts` in `paredit-core-config` cannot be the one
//! to notice that `(defrule foo :severity error ...)` disagrees with a
//! `lint.warn = ["foo"]` in `paredit.toml`. This lives here instead, in the
//! composition root, which already reads both packages for `config check`
//! and for `inspect lint`'s custom-rule support respectively.
//!
//! `lint.warn`/`lint.deny` are validated by `paredit-core-config` against the
//! *shipped* rule catalogue only (`ValueKind::RuleOrCategoryList` in
//! `schema.rs`, checked in `settings.rs`'s `check_list`) — it has no notion
//! of a project's own custom rules, and a value that earns any error-severity
//! diagnostic is dropped rather than recorded (`Settings::apply`'s doc
//! comment). That means a custom rule's name in `lint.warn`/`lint.deny` never
//! survives into `Settings`, and reading `settings.list("lint.warn")` here
//! would find it empty even when the file plainly names the rule. This reads
//! the raw TOML instead, replaying `loaded.sources` in application order —
//! the same "last file to set a key wins outright" rule the real loader
//! applies — so the comparison is against what the file actually says, not
//! against what the shipped-only vocabulary let through.
//!
//! A rule file this cannot parse is skipped rather than failing `config
//! check` outright: the file's own syntax problem is surfaced when whatever
//! actually loads the rule set for real (`inspect lint`) runs, and a check
//! that exists to compare *severities* has nothing useful to say about a rule
//! it could not read one from.

use std::path::{Path, PathBuf};

use paredit_core_config::error::{Diagnostic, DiagnosticCode};
use paredit_core_config::load::Loaded;
use paredit_core_config::settings::Origin;
use paredit_core_config::toml::{self, Value};
use paredit_core_lint_engine::model::Severity as RuleSeverity;
use paredit_feature_lint_custom::parse_ruleset;

/// One `lint.warn`/`lint.deny` list, as the file that set it last actually
/// wrote it, with the origin to blame a diagnostic on.
struct RawList {
    items: Vec<String>,
    origin: Origin,
}

/// Every conflict between a custom rule's declared `:severity` and the
/// `lint.warn`/`lint.deny` list that also names it.
///
/// Empty whenever `lint.custom-rules` is unset, names a directory that is not
/// there, or names a directory holding no rule whose name also appears in
/// `lint.warn` or `lint.deny`.
pub(super) fn conflicts(loaded: &Loaded) -> Vec<Diagnostic> {
    let Some(directory) = loaded.settings.text("lint.custom-rules") else {
        return Vec::new();
    };
    let directory = Path::new(directory);
    if !directory.is_dir() {
        return Vec::new();
    }

    let deny = raw_list(loaded, "lint.deny");
    let warn = raw_list(loaded, "lint.warn");
    if deny.is_none() && warn.is_none() {
        return Vec::new();
    }

    rule_files(directory)
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok().map(|text| (path, text)))
        .filter_map(|(path, text)| parse_ruleset(&path.display().to_string(), &text).ok())
        .flat_map(|ruleset| ruleset.rules)
        .filter_map(|rule| check_rule(&rule.name, rule.severity, deny.as_ref(), warn.as_ref()))
        .collect()
}

/// Every `*.lisp` file directly under `directory`, sorted for a reproducible
/// diagnostic order — the same ordering `inspect lint`'s own rule loader uses,
/// for the same reason.
fn rule_files(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "lisp")
        })
        .collect();
    paths.sort();
    paths
}

/// `key`'s value as the last source in application order literally wrote it,
/// bypassing `Settings`' shipped-only vocabulary gate. `None` when no source
/// sets `key` at all.
fn raw_list(loaded: &Loaded, key: &str) -> Option<RawList> {
    let mut found = None;
    for source in &loaded.sources {
        let Ok(text) = std::fs::read_to_string(&source.path) else {
            continue;
        };
        let Ok(document) = toml::parse(&text) else {
            continue;
        };
        for entry in &document.entries {
            if entry.key != key {
                continue;
            }
            let items = match &entry.value {
                Value::Array(items) => items
                    .iter()
                    .filter_map(|item| match item {
                        Value::String(text) => Some(text.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            found = Some(RawList {
                items,
                origin: Origin::file(source.layer, source.path.clone(), entry.line),
            });
        }
    }
    found
}

/// Which list names `name`, and what severity it implies, checking
/// `lint.deny` first: a name in both lists is already reported by
/// `check_conflicts`' own overlap check, so which branch this takes for such
/// a name does not matter.
fn locate<'a>(
    name: &str,
    deny: Option<&'a RawList>,
    warn: Option<&'a RawList>,
) -> Option<(&'static str, &'static str, RuleSeverity, &'a Origin)> {
    if let Some(list) = deny {
        if list.items.iter().any(|item| item == name) {
            return Some(("lint.deny", "error", RuleSeverity::Error, &list.origin));
        }
    }
    if let Some(list) = warn {
        if list.items.iter().any(|item| item == name) {
            return Some(("lint.warn", "warning", RuleSeverity::Warning, &list.origin));
        }
    }
    None
}

/// One rule's diagnosis, when its name appears in `lint.deny` or `lint.warn`
/// and its own `:severity` disagrees with what that list implies.
fn check_rule(
    name: &str,
    severity: RuleSeverity,
    deny: Option<&RawList>,
    warn: Option<&RawList>,
) -> Option<Diagnostic> {
    let (list_key, implied, implied_severity, origin) = locate(name, deny, warn)?;
    if severity == implied_severity {
        return None;
    }

    Some(Diagnostic {
        code: DiagnosticCode::Conflict,
        path: origin.path.clone(),
        line: origin.line,
        key: list_key.to_owned(),
        message: format!(
            "custom rule \"{name}\" declares `:severity {}`, but `{list_key}` implies {implied}",
            severity.as_str()
        ),
        suggestion: Some(format!(
            "match the rule's own `:severity {}`, or remove \"{name}\" from `{list_key}`",
            severity.as_str()
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use paredit_core_config::load::{LoadOptions, load};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "paredit-config-custom-conflicts-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("create scratch root");
            Self(root)
        }

        fn write(&self, relative: &str, contents: &str) -> PathBuf {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent");
            }
            fs::write(&path, contents).expect("write file");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A real `Loaded`, discovered from `scratch` the same way the binary
    /// does — including a `.git` marker, so `lint.custom-rules` resolves the
    /// same relative path a real run would.
    fn loaded_from(scratch: &Scratch, toml: &str) -> Loaded {
        fs::create_dir_all(scratch.0.join(".git")).expect("create .git marker");
        scratch.write("paredit.toml", toml);
        load(
            &LoadOptions {
                start: scratch.0.clone(),
                ..LoadOptions::default()
            },
            // No vocabulary: a custom rule's name is never a shipped rule or
            // category, so passing the real one would only ever reject it —
            // exactly the failure mode `raw_list` exists to route around.
            // Leaving it `None` here isolates that from this test, the same
            // way `config_command.rs`'s end-to-end tests isolate it by
            // asserting on the `conflict` diagnostic specifically rather
            // than on the whole report's status.
            None,
        )
        .expect("loads")
    }

    #[test]
    fn a_rule_declared_error_but_listed_in_warn_is_a_conflict() {
        let scratch = Scratch::new("warn-mismatch");
        scratch.write(
            "rules/r.lisp",
            r#"(defrule r :severity error :pattern (f) :message "m")"#,
        );
        let loaded = loaded_from(
            &scratch,
            "[lint]\ncustom-rules = \"rules\"\nwarn = [\"r\"]\n",
        );

        let found = conflicts(&loaded);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, DiagnosticCode::Conflict);
        assert!(found[0].message.contains('r'));
        assert!(found[0].message.contains("lint.warn"));
    }

    #[test]
    fn a_rule_declared_error_and_listed_in_deny_agrees() {
        let scratch = Scratch::new("deny-match");
        scratch.write(
            "rules/r.lisp",
            r#"(defrule r :severity error :pattern (f) :message "m")"#,
        );
        let loaded = loaded_from(
            &scratch,
            "[lint]\ncustom-rules = \"rules\"\ndeny = [\"r\"]\n",
        );

        assert!(conflicts(&loaded).is_empty());
    }

    #[test]
    fn a_rule_named_in_neither_list_is_not_a_conflict() {
        let scratch = Scratch::new("unreferenced");
        scratch.write(
            "rules/r.lisp",
            r#"(defrule r :severity error :pattern (f) :message "m")"#,
        );
        let loaded = loaded_from(
            &scratch,
            "[lint]\ncustom-rules = \"rules\"\ndeny = [\"other\"]\n",
        );

        assert!(conflicts(&loaded).is_empty());
    }

    #[test]
    fn a_malformed_rule_file_does_not_crash_the_check() {
        let scratch = Scratch::new("malformed");
        scratch.write("rules/bad.lisp", "(defrule (((( broken");
        scratch.write(
            "rules/good.lisp",
            r#"(defrule good :severity error :pattern (f) :message "m")"#,
        );
        let loaded = loaded_from(
            &scratch,
            "[lint]\ncustom-rules = \"rules\"\nwarn = [\"good\"]\n",
        );

        let found = conflicts(&loaded);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("good"));
    }

    /// `raw_list` has to find the name even though `Settings` itself dropped
    /// the whole `lint.warn` entry as an `unknown-rule` — the scenario that
    /// made the first version of this check unreachable through a real file.
    #[test]
    fn the_conflict_is_found_even_though_settings_rejected_the_list_as_unknown_rules() {
        let scratch = Scratch::new("settings-rejected");
        scratch.write(
            "rules/r.lisp",
            r#"(defrule r :severity error :pattern (f) :message "m")"#,
        );
        fs::create_dir_all(scratch.0.join(".git")).expect(".git");
        scratch.write(
            "paredit.toml",
            "[lint]\ncustom-rules = \"rules\"\nwarn = [\"r\"]\n",
        );

        let loaded = load(
            &LoadOptions {
                start: scratch.0.clone(),
                ..LoadOptions::default()
            },
            Some(paredit_core_config::settings::Vocabulary {
                rules: &[],
                categories: &[],
            }),
        )
        .expect("loads");

        // Confirms the premise: `Settings` itself has nothing for `lint.warn`.
        assert!(loaded.settings.list("lint.warn").is_empty());

        let found = conflicts(&loaded);
        assert_eq!(found.len(), 1, "{found:?}");
    }
}
