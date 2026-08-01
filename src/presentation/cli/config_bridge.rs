//! Turning configured keys into the flags the commands already understand.
//!
//! The alternative was making every one of ~275 argument structs read the
//! configuration, which means 275 places for "a flag beats a file" to be got
//! wrong. Instead the argument vector is rewritten once, before `clap` sees
//! it: for each binding below, if the command being run declares that flag and
//! the caller did not pass it, the configured value is appended.
//!
//! Three properties fall out of doing it here rather than in the argument
//! structs:
//!
//! - **A flag always wins.** Not by policy stated in 275 places, but because a
//!   flag that is already present is never injected over.
//! - **`--help` stays true.** The flag a command documents is the flag the
//!   configuration sets; there is no second, invisible way to set it.
//! - **It is verifiable.** The result is an argument vector, so a test can
//!   assert exactly what a configuration turns into.
//!
//! Two safeguards keep the rewrite from ever being the thing that breaks a
//! command. A configuration with errors injects nothing at all — a broken file
//! should send you to `paredit config check`, not to a puzzling `clap` error
//! about flags you never typed. And if the rewritten vector fails to parse for
//! any reason, the original is used instead, so the worst case is that the
//! configuration is ignored.

use clap::builder::Command as ClapCommand;

use super::argv::{declares_flag, long_flags_present, resolve_leaf};

use paredit_core_config::Settings;
use paredit_core_config::toml::Value;

/// How a configured value becomes argument tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// `--flag value`.
    Value,
    /// `--flag` when true, nothing when false.
    Switch,
    /// `--flag item` once per list entry.
    Repeated,
}

/// Which commands a binding applies to.
#[derive(Debug, Clone, Copy)]
enum Scope {
    /// Any command that declares the flag.
    ///
    /// Safe only where the flag means the same thing everywhere it appears.
    Anywhere,
    /// Only these command paths, space-separated as `capabilities` prints them.
    ///
    /// `--exclude` is the reason this exists: on `inspect lint` it names a
    /// rule, and on `inspect similarity` it names a path. One config key must
    /// not reach both.
    Only(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy)]
struct Binding {
    key: &'static str,
    flag: &'static str,
    shape: Shape,
    scope: Scope,
    /// A value that means "do not pass this flag at all".
    ///
    /// `lint.fail-on = "never"` is the case: the configuration spells "no
    /// gate" as a value, and the flag spells it as absence. Without this the
    /// key would inject `--fail-on never`, which is not a value `--fail-on`
    /// accepts, and the command would fail because of the configuration.
    omit_when: Option<&'static str>,
}

const LINT: &[&str] = &["inspect lint"];

/// Every command whose `--cache-dir` is `WorkspaceInputArgs`'s discovery
/// cache — "reuse a previous scan's file list from this directory".
///
/// `inspect lint` also declares a `--cache-dir`, but its own: a per-file
/// analysis cache keyed on file content, opened straight from
/// `LintReportArgs::cache_dir` rather than flattened from
/// `WorkspaceInputArgs`. One key reaching both would be the same trap
/// `--exclude` already warns about above — two different flags that happen
/// to share a spelling — so `inspect lint` is deliberately left off this
/// list rather than folded into `Scope::Anywhere`.
const WORKSPACE_CACHE: &[&str] = &[
    "inspect workspace",
    "inspect sources",
    "inspect clone-classes",
    "inspect clone-sequences",
    "inspect clone-external",
    "inspect clone-threshold",
    "inspect clone-genealogy",
    "inspect similarity",
    "refactor workspace-plan",
    "refactor workspace-preview",
    "refactor workspace-execute",
    "query find",
    "query count",
    "query replace",
    "migrate run",
];

/// Every configured key that maps onto an existing flag.
///
/// Keys absent from this table are not unconfigured: `[dialect]` and the rest
/// of `[paths]` act below the argument layer and are carried by
/// `paredit_core_cli::runtime` instead.
const BINDINGS: &[Binding] = &[
    Binding {
        key: "format.indent",
        flag: "indent",
        shape: Shape::Value,
        scope: Scope::Anywhere,
        omit_when: None,
    },
    Binding {
        key: "format.max-inline-width",
        flag: "max-width",
        shape: Shape::Value,
        scope: Scope::Anywhere,
        omit_when: None,
    },
    Binding {
        key: "format.block-comment-reindent",
        flag: "reindent-block-comments",
        shape: Shape::Switch,
        scope: Scope::Anywhere,
        omit_when: None,
    },
    Binding {
        key: "format.comment-column",
        flag: "comment-column",
        shape: Shape::Value,
        scope: Scope::Anywhere,
        omit_when: None,
    },
    Binding {
        key: "format.blank-lines-max-consecutive",
        flag: "max-blank-lines",
        shape: Shape::Value,
        scope: Scope::Anywhere,
        omit_when: None,
    },
    Binding {
        key: "paths.include-hidden",
        flag: "include-hidden",
        shape: Shape::Switch,
        scope: Scope::Anywhere,
        omit_when: None,
    },
    Binding {
        key: "paths.include-generated",
        flag: "include-generated",
        shape: Shape::Switch,
        scope: Scope::Anywhere,
        omit_when: None,
    },
    Binding {
        key: "paths.max-depth",
        flag: "max-depth",
        shape: Shape::Value,
        scope: Scope::Anywhere,
        omit_when: None,
    },
    Binding {
        key: "cache.dir",
        flag: "cache-dir",
        shape: Shape::Value,
        scope: Scope::Only(WORKSPACE_CACHE),
        omit_when: None,
    },
    Binding {
        key: "lint.preset",
        flag: "preset",
        shape: Shape::Value,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.enable",
        flag: "rule",
        shape: Shape::Repeated,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.disable",
        flag: "exclude",
        shape: Shape::Repeated,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.categories",
        flag: "category",
        shape: Shape::Repeated,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.tags",
        flag: "tag",
        shape: Shape::Repeated,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.deny",
        flag: "deny",
        shape: Shape::Repeated,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.warn",
        flag: "warn",
        shape: Shape::Repeated,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.experimental",
        flag: "experimental",
        shape: Shape::Switch,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.baseline",
        flag: "baseline",
        shape: Shape::Value,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.custom-rules",
        flag: "custom-rules",
        shape: Shape::Value,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.require-suppression-reason",
        flag: "require-suppression-reason",
        shape: Shape::Switch,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.suppress-paths",
        flag: "suppress-path",
        shape: Shape::Repeated,
        scope: Scope::Only(LINT),
        omit_when: None,
    },
    Binding {
        key: "lint.fail-on",
        flag: "fail-on",
        shape: Shape::Value,
        scope: Scope::Only(LINT),
        omit_when: Some("never"),
    },
    Binding {
        key: "output.format",
        flag: "output",
        shape: Shape::Value,
        scope: Scope::Anywhere,
        omit_when: None,
    },
    Binding {
        key: "output.verbosity",
        flag: "verbosity",
        shape: Shape::Value,
        scope: Scope::Anywhere,
        omit_when: None,
    },
    Binding {
        key: "output.max-tokens",
        flag: "max-tokens",
        shape: Shape::Value,
        scope: Scope::Anywhere,
        // Zero is how the schema spells "no budget", and the flag spells that
        // as absence.
        omit_when: Some("0"),
    },
];

/// One flag the configuration contributed, for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Injected {
    pub key: &'static str,
    pub flag: &'static str,
    pub values: Vec<String>,
}

/// The rewritten argument vector, and what was added to it.
#[derive(Debug, Clone)]
pub struct Augmented {
    pub argv: Vec<String>,
    pub injected: Vec<Injected>,
}

/// Rewrites `argv` with whatever the configuration sets and the command
/// accepts.
///
/// `argv` includes the program name, as `std::env::args` produces it.
#[must_use]
pub fn augment(argv: &[String], settings: &Settings, root: &ClapCommand) -> Augmented {
    let Some((leaf, path)) = resolve_leaf(argv, root) else {
        return unchanged(argv);
    };

    let injected = plan(leaf, &path, &long_flags_present(argv), settings);
    if injected.is_empty() {
        return unchanged(argv);
    }

    let mut additions = Vec::new();
    for entry in &injected {
        if entry.values.is_empty() {
            // A switch contributes the flag and no value.
            additions.push(format!("--{}", entry.flag));
            continue;
        }
        for value in &entry.values {
            additions.push(format!("--{}", entry.flag));
            additions.push(value.clone());
        }
    }

    // Everything after a `--` is positional by definition, so the additions go
    // in front of it or they would arrive as file names.
    let insert_at = argv
        .iter()
        .position(|token| token == "--")
        .unwrap_or(argv.len());
    let mut rewritten = argv[..insert_at].to_vec();
    rewritten.extend(additions);
    rewritten.extend_from_slice(&argv[insert_at..]);

    Augmented {
        argv: rewritten,
        injected,
    }
}

/// What the configuration would contribute to one command.
///
/// Split out from [`augment`] so `paredit config show --for <command>` can
/// answer "what will my configuration do to this command?" using the same code
/// that actually does it. An explanation produced by a second implementation
/// is an explanation that can be wrong.
fn plan(leaf: &ClapCommand, path: &str, present: &[String], settings: &Settings) -> Vec<Injected> {
    let mut injected = Vec::new();
    for binding in BINDINGS {
        if !binding.scope.admits(path) || present.contains(&binding.flag.to_owned()) {
            continue;
        }
        if !declares_flag(leaf, binding.flag) {
            continue;
        }
        let Some(values) = binding.tokens(settings) else {
            continue;
        };
        injected.push(Injected {
            key: binding.key,
            flag: binding.flag,
            values,
        });
    }
    injected
}

/// The flags the configuration would add to `command_path`, for reporting.
///
/// Returns `None` when the path does not name a command, so the caller can say
/// so rather than reporting an empty plan that looks like "nothing to do".
#[must_use]
pub fn plan_for(
    command_path: &str,
    settings: &Settings,
    root: &ClapCommand,
) -> Option<Vec<Injected>> {
    let argv: Vec<String> = std::iter::once("paredit".to_owned())
        .chain(command_path.split_whitespace().map(str::to_owned))
        .collect();
    let (leaf, resolved) = resolve_leaf(&argv, root)?;
    (resolved
        == command_path
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "))
    .then(|| plan(leaf, &resolved, &[], settings))
}

fn unchanged(argv: &[String]) -> Augmented {
    Augmented {
        argv: argv.to_vec(),
        injected: Vec::new(),
    }
}

impl Scope {
    fn admits(self, path: &str) -> bool {
        match self {
            Self::Anywhere => true,
            Self::Only(paths) => paths.contains(&path),
        }
    }
}

impl Binding {
    /// The values this binding contributes, or `None` when it contributes
    /// nothing — unset, left at its default, a switch that is off, or the
    /// value that means "do not pass this flag".
    fn tokens(&self, settings: &Settings) -> Option<Vec<String>> {
        // A key nobody set is a key the command's own default already covers.
        // Injecting it would restate the default on every command line, and
        // where the two spellings differ — `lint.fail-on = "never"` against a
        // `--fail-on` that takes only a severity — it would break the command.
        if !settings.is_customised(self.key) {
            return None;
        }
        let resolved = settings.resolved(self.key)?;
        if let Some(sentinel) = self.omit_when {
            let written = match &resolved.value {
                Value::String(text) => text.clone(),
                Value::Integer(number) => number.to_string(),
                other => other.to_string(),
            };
            if written == sentinel {
                return None;
            }
        }
        match (self.shape, &resolved.value) {
            (Shape::Switch, Value::Boolean(true)) => Some(Vec::new()),
            (Shape::Switch, _) => None,
            (Shape::Value, Value::String(text)) => Some(vec![text.clone()]),
            (Shape::Value, Value::Integer(number)) => Some(vec![number.to_string()]),
            (Shape::Repeated, Value::Array(items)) if !items.is_empty() => Some(
                items
                    .iter()
                    .filter_map(|item| match item {
                        Value::String(text) => Some(text.clone()),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    use clap::{CommandFactory, Parser};

    use paredit_core_config::Origin;
    use paredit_core_config::settings::Layer;

    use crate::presentation::cli::Cli;

    fn root() -> ClapCommand {
        Cli::command()
    }

    fn argv(tokens: &[&str]) -> Vec<String> {
        std::iter::once("paredit")
            .chain(tokens.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    fn settings(pairs: &[(&'static str, Value)]) -> Settings {
        let mut settings = Settings::with_defaults();
        for (key, value) in pairs {
            settings.set(
                key,
                value.clone(),
                Origin::file(Layer::Repository, "p.toml", 1),
            );
        }
        settings
    }

    fn run(tokens: &[&str], pairs: &[(&'static str, Value)]) -> Vec<String> {
        augment(&argv(tokens), &settings(pairs), &root()).argv
    }

    #[test]
    fn a_configured_value_becomes_the_flag_the_command_declares() {
        let result = run(
            &["edit", "format", "--file", "a.lisp"],
            &[("format.indent", Value::Integer(4))],
        );
        assert!(
            result.windows(2).any(|pair| pair == ["--indent", "4"]),
            "{result:?}"
        );
    }

    /// FR-012: a configured `cache.dir` reaches a command that flattens
    /// `WorkspaceInputArgs`, the same way `format.indent` reaches `--indent`.
    #[test]
    fn a_configured_cache_dir_becomes_the_flag_the_workspace_discovery_cache_declares() {
        let result = run(
            &["inspect", "workspace", "."],
            &[("cache.dir", Value::String("/tmp/paredit-cache".to_owned()))],
        );
        assert!(
            result
                .windows(2)
                .any(|pair| pair == ["--cache-dir", "/tmp/paredit-cache"]),
            "{result:?}"
        );
    }

    /// `inspect lint --cache-dir` is a different flag wearing the same name —
    /// a per-file analysis cache, not `WorkspaceInputArgs`'s discovery cache —
    /// so `cache.dir` must not reach it. The same shape as
    /// `a_scoped_binding_does_not_reach_a_command_outside_its_scope` above,
    /// for `--exclude`.
    #[test]
    fn cache_dir_does_not_reach_lints_unrelated_cache_dir_flag() {
        let result = run(
            &["inspect", "lint", "a.lisp"],
            &[("cache.dir", Value::String("/tmp/paredit-cache".to_owned()))],
        );
        assert!(!result.contains(&"--cache-dir".to_owned()), "{result:?}");
    }

    /// The same guarantee `--indent` gets: a `--cache-dir` already on the
    /// command line is never injected over.
    #[test]
    fn an_explicit_cache_dir_flag_beats_the_configuration() {
        let result = run(
            &["inspect", "workspace", ".", "--cache-dir", "/explicit"],
            &[("cache.dir", Value::String("/configured".to_owned()))],
        );
        assert_eq!(
            result
                .iter()
                .filter(|token| *token == "--cache-dir")
                .count(),
            1
        );
        assert!(result.contains(&"/explicit".to_owned()));
        assert!(!result.contains(&"/configured".to_owned()));
    }

    /// The property the whole mechanism exists to guarantee.
    #[test]
    fn a_flag_on_the_command_line_is_never_injected_over() {
        let result = run(
            &["edit", "format", "--indent", "8"],
            &[("format.indent", Value::Integer(4))],
        );
        assert_eq!(
            result.iter().filter(|token| *token == "--indent").count(),
            1
        );
        assert!(result.contains(&"8".to_owned()));
        assert!(!result.contains(&"4".to_owned()));
    }

    #[test]
    fn the_equals_spelling_counts_as_present_too() {
        let result = run(
            &["edit", "format", "--indent=8"],
            &[("format.indent", Value::Integer(4))],
        );
        assert!(!result.contains(&"--indent".to_owned()), "{result:?}");
    }

    #[test]
    fn a_command_that_does_not_declare_the_flag_is_left_alone() {
        let result = run(
            &["inspect", "check", "--file", "a.lisp"],
            &[("format.indent", Value::Integer(4))],
        );
        assert_eq!(result, argv(&["inspect", "check", "--file", "a.lisp"]));
    }

    /// `--exclude` names a rule on `inspect lint` and a path elsewhere. One
    /// key reaching both would silently mean two different things.
    #[test]
    fn a_scoped_binding_does_not_reach_a_command_outside_its_scope() {
        let pairs = [(
            "lint.disable",
            Value::Array(vec![Value::String("nil-comparison".to_owned())]),
        )];
        let lint = run(&["inspect", "lint", "a.lisp"], &pairs);
        assert!(
            lint.windows(2)
                .any(|p| p == ["--exclude", "nil-comparison"])
        );

        let similarity = run(&["inspect", "similarity", "a.lisp"], &pairs);
        assert!(
            !similarity.contains(&"--exclude".to_owned()),
            "{similarity:?}"
        );
    }

    #[test]
    fn a_list_becomes_one_flag_per_item() {
        let result = run(
            &["inspect", "lint", "a.lisp"],
            &[(
                "lint.deny",
                Value::Array(vec![
                    Value::String("one".to_owned()),
                    Value::String("two".to_owned()),
                ]),
            )],
        );
        assert_eq!(result.iter().filter(|token| *token == "--deny").count(), 2);
    }

    #[test]
    fn an_empty_list_contributes_nothing() {
        let result = run(
            &["inspect", "lint", "a.lisp"],
            &[("lint.deny", Value::Array(Vec::new()))],
        );
        assert_eq!(result, argv(&["inspect", "lint", "a.lisp"]));
    }

    #[test]
    fn a_switch_that_is_off_contributes_nothing() {
        let result = run(
            &["inspect", "lint", "a.lisp"],
            &[("lint.experimental", Value::Boolean(false))],
        );
        assert!(!result.contains(&"--experimental".to_owned()));
    }

    #[test]
    fn a_switch_that_is_on_contributes_the_bare_flag() {
        let result = run(
            &["inspect", "lint", "a.lisp"],
            &[("lint.experimental", Value::Boolean(true))],
        );
        assert!(result.contains(&"--experimental".to_owned()));
        // A switch takes no value, so nothing may follow it.
        let index = result
            .iter()
            .position(|t| t == "--experimental")
            .expect("present");
        assert!(
            result
                .get(index + 1)
                .is_none_or(|next| next.starts_with("--") || next == "a.lisp")
        );
    }

    /// Injecting after `--` would turn a flag into a file name.
    #[test]
    fn injection_goes_in_front_of_a_double_dash() {
        let result = run(
            &["edit", "format", "--", "weird--name.lisp"],
            &[("format.indent", Value::Integer(4))],
        );
        let separator = result
            .iter()
            .position(|t| t == "--")
            .expect("separator kept");
        let indent = result
            .iter()
            .position(|t| t == "--indent")
            .expect("injected");
        assert!(indent < separator, "{result:?}");
    }

    #[test]
    fn a_bare_invocation_with_no_subcommand_is_untouched() {
        let result = run(&[], &[("format.indent", Value::Integer(4))]);
        assert_eq!(result, argv(&[]));
        assert_eq!(run(&["--help"], &[]), argv(&["--help"]));
    }

    #[test]
    fn an_unknown_subcommand_is_untouched() {
        let result = run(&["nonsense"], &[("format.indent", Value::Integer(4))]);
        assert_eq!(result, argv(&["nonsense"]));
    }

    /// Defaults are recorded in `Settings` like anything else, and injecting
    /// them would fill every command line with values that were already the
    /// command's own defaults. Only what a layer actually set is injected.
    #[test]
    fn built_in_defaults_are_not_injected() {
        let result = augment(
            &argv(&["inspect", "lint", "a.lisp"]),
            &Settings::with_defaults(),
            &root(),
        );
        assert!(result.injected.is_empty(), "{:?}", result.injected);
        assert_eq!(result.argv, argv(&["inspect", "lint", "a.lisp"]));
    }

    /// The default is not merely noise. `lint.fail-on` defaults to `"never"`,
    /// and `--fail-on` accepts only a severity, so injecting the default would
    /// make every `inspect lint` run fail on a flag nobody typed.
    #[test]
    fn a_defaulted_key_whose_spelling_the_flag_rejects_is_never_injected() {
        let result = augment(
            &argv(&["inspect", "lint", "a.lisp"]),
            &Settings::with_defaults(),
            &root(),
        );
        assert!(
            !result.argv.contains(&"--fail-on".to_owned()),
            "{:?}",
            result.argv
        );
        assert!(Cli::try_parse_from(&result.argv).is_ok());
    }

    /// Even written out explicitly, the value that means "no gate" must become
    /// the flag's absence rather than a value the flag cannot take.
    #[test]
    fn an_explicit_never_still_omits_the_gate_flag() {
        let result = run(
            &["inspect", "lint", "a.lisp"],
            &[("lint.fail-on", Value::String("never".to_owned()))],
        );
        assert!(!result.contains(&"--fail-on".to_owned()), "{result:?}");
    }

    /// The sentinel has to work for whatever type the key holds, not only for
    /// a string: `output.max-tokens = 0` spells "no budget" as an integer.
    #[test]
    fn an_integer_sentinel_is_recognised_too() {
        let result = run(
            &["inspect", "agent-report", "--file", "a.lisp"],
            &[("output.max-tokens", Value::Integer(0))],
        );
        assert!(!result.contains(&"--max-tokens".to_owned()), "{result:?}");

        let result = run(
            &["inspect", "agent-report", "--file", "a.lisp"],
            &[("output.max-tokens", Value::Integer(4096))],
        );
        assert!(
            result
                .windows(2)
                .any(|pair| pair == ["--max-tokens", "4096"])
        );
    }

    #[test]
    fn a_real_gate_severity_is_injected() {
        let result = run(
            &["inspect", "lint", "a.lisp"],
            &[("lint.fail-on", Value::String("error".to_owned()))],
        );
        assert!(result.windows(2).any(|pair| pair == ["--fail-on", "error"]));
    }

    /// Whatever the configuration says, the rewritten vector has to be
    /// something `clap` accepts — the bridge must never be the reason a
    /// command fails.
    #[test]
    fn a_fully_populated_configuration_still_parses() {
        let settings = settings(&[
            ("format.indent", Value::Integer(4)),
            ("lint.preset", Value::String("all".to_owned())),
            ("lint.fail-on", Value::String("error".to_owned())),
            ("lint.experimental", Value::Boolean(true)),
            (
                "lint.disable",
                Value::Array(vec![Value::String("nil-comparison".to_owned())]),
            ),
            ("output.format", Value::String("text".to_owned())),
        ]);
        for tokens in [
            vec!["inspect", "lint", "a.lisp"],
            vec!["edit", "format", "--file", "a.lisp"],
            vec!["inspect", "workspace", "."],
            vec!["inspect", "check", "--file", "a.lisp"],
        ] {
            let result = augment(&argv(&tokens), &settings, &root());
            assert!(
                Cli::try_parse_from(&result.argv).is_ok(),
                "{tokens:?} became unparsable: {:?}",
                result.argv
            );
        }
    }

    #[test]
    fn the_report_names_the_key_and_the_flag_it_became() {
        let result = augment(
            &argv(&["edit", "format", "--file", "a.lisp"]),
            &settings(&[("format.indent", Value::Integer(4))]),
            &root(),
        );
        assert_eq!(
            result.injected,
            vec![Injected {
                key: "format.indent",
                flag: "indent",
                values: vec!["4".to_owned()],
            }]
        );
    }

    /// Every binding has to name a flag that some command actually declares,
    /// or it is a configuration key that silently does nothing.
    #[test]
    fn every_binding_names_a_flag_that_exists_somewhere() {
        let root = root();
        for binding in BINDINGS {
            assert!(
                any_command_declares(&root, binding.flag),
                "no command declares --{}, so `{}` configures nothing",
                binding.flag,
                binding.key
            );
        }
    }

    /// A scoped binding must name a command path that exists, or the scope
    /// silently excludes everything.
    #[test]
    fn every_scoped_binding_names_a_real_command() {
        let root = root();
        for binding in BINDINGS {
            let Scope::Only(paths) = binding.scope else {
                continue;
            };
            for path in paths {
                let tokens: Vec<String> = std::iter::once("paredit".to_owned())
                    .chain(path.split(' ').map(str::to_owned))
                    .collect();
                let (leaf, resolved) = resolve_leaf(&tokens, &root)
                    .unwrap_or_else(|| panic!("{path} is not a command"));
                assert_eq!(&resolved, path);
                assert!(
                    declares_flag(leaf, binding.flag),
                    "`{path}` does not declare --{}",
                    binding.flag
                );
            }
        }
    }

    /// `Scope::Anywhere` is a claim, and the claim that matters is narrower
    /// than "same flag everywhere": it is *every value this key can inject is
    /// a value every command declaring the flag accepts*.
    ///
    /// The distinction is live. `--output` accepts `text` and `json`
    /// everywhere, and additionally `sarif`, `junit`, `csv` and the rest on
    /// the reports that carry CI interchange formats. That is not two flags —
    /// it is one flag with a wider vocabulary in some places — and
    /// `output.format` is a `Choice` of `text` and `json`, so nothing it can
    /// produce is ever rejected.
    ///
    /// What would be two flags is a `--flag` that takes a value here and none
    /// there, so that is checked too.
    #[test]
    fn an_anywhere_binding_can_only_inject_values_every_command_accepts() {
        let root = root();
        for binding in BINDINGS {
            if !matches!(binding.scope, Scope::Anywhere) {
                continue;
            }
            let mut declarations = Vec::new();
            collect_flag_declarations(&root, binding.flag, &mut declarations);
            assert!(
                !declarations.is_empty(),
                "--{} is bound Anywhere but no command declares it",
                binding.flag
            );

            let takes_value: BTreeSet<bool> =
                declarations.iter().map(|(takes, _)| *takes).collect();
            assert_eq!(
                takes_value.len(),
                1,
                "--{} takes a value on some commands and not others, so `Anywhere` \
                 is claiming two flags are one",
                binding.flag
            );

            let Some(choices) = paredit_core_config::schema::lookup(binding.key)
                .and_then(|entry| entry.kind.choices())
            else {
                continue;
            };
            for (_, accepted) in &declarations {
                if accepted.is_empty() {
                    // The command constrains nothing, so it accepts anything.
                    continue;
                }
                for choice in choices {
                    assert!(
                        accepted.iter().any(|value| value == choice),
                        "`{}` can be set to \"{choice}\", which --{} does not accept \
                         on every command ({accepted:?})",
                        binding.key,
                        binding.flag
                    );
                }
            }
        }
    }

    /// `(takes a value, accepted values)` for each command declaring `flag`.
    fn collect_flag_declarations(
        command: &ClapCommand,
        flag: &str,
        found: &mut Vec<(bool, Vec<String>)>,
    ) {
        for argument in command.get_arguments() {
            if argument.get_long() != Some(flag) {
                continue;
            }
            let takes_value = matches!(
                argument.get_action(),
                clap::ArgAction::Set | clap::ArgAction::Append
            );
            let values = argument
                .get_possible_values()
                .iter()
                .map(|value| value.get_name().to_owned())
                .collect();
            found.push((takes_value, values));
        }
        for subcommand in command.get_subcommands() {
            collect_flag_declarations(subcommand, flag, found);
        }
    }

    /// Each key may appear once. Two bindings for one key would inject twice.
    #[test]
    fn no_key_is_bound_twice() {
        let mut keys: Vec<&str> = BINDINGS.iter().map(|binding| binding.key).collect();
        keys.sort_unstable();
        let count = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), count);
    }

    /// Every bound key has to be a key the schema declares, or the binding
    /// reads a value that can never be set.
    #[test]
    fn every_binding_names_a_schema_key() {
        for binding in BINDINGS {
            assert!(
                paredit_core_config::schema::lookup(binding.key).is_some(),
                "`{}` is not in the configuration schema",
                binding.key
            );
        }
    }

    fn any_command_declares(command: &ClapCommand, flag: &str) -> bool {
        declares_flag(command, flag)
            || command
                .get_subcommands()
                .any(|subcommand| any_command_declares(subcommand, flag))
    }
}
