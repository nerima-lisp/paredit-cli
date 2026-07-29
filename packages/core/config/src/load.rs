//! Finding the configuration files, in order, and merging what they say.
//!
//! The order is the design. Five layers, lowest precedence first — built-in
//! defaults, the user's own file, the repository's, every directory between
//! the repository root and where the command was run, and finally the
//! `PAREDIT_*` environment. Each layer replaces the previous key by key, never
//! merging *within* a key: a nested `paredit.toml` that sets `lint.disable`
//! replaces the repository's list rather than adding to it, because a list
//! assembled from three files is a list nobody can predict.
//!
//! Impurity is confined to [`LoadOptions::from_process`] and the file reads.
//! Everything else takes the environment as data, which is what makes the
//! layering testable without a process-wide mutation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{ConfigError, ConfigResult, Diagnostic, DiagnosticCode};
use crate::schema;
use crate::settings::{Layer, Origin, Settings, Vocabulary};
use crate::toml::{self, Value};

/// The names a configuration file may have, in the order they are tried.
pub const CONFIG_FILE_NAMES: [&str; 2] = ["paredit.toml", ".paredit.toml"];

/// The environment variable that relocates the user layer wholesale.
pub const CONFIG_HOME_VAR: &str = "PAREDIT_CONFIG_HOME";

/// A configuration file this big is a mistake, not a configuration.
pub const MAX_CONFIG_BYTES: u64 = 1 << 20;

/// How deep an `extends` chain may go before it is treated as runaway.
pub const MAX_EXTENDS_DEPTH: usize = 16;

/// Everything the loader needs, taken as data so the merge is testable.
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// The directory the command was invoked in. Discovery walks up from here.
    pub start: PathBuf,
    /// A file named outright. Replaces the user, repository, and directory
    /// layers: naming a file means that file, not that file and whatever else
    /// discovery happened to turn up.
    pub explicit: Option<PathBuf>,
    /// Read no files at all. The escape hatch for a reproducible CI run.
    pub skip_files: bool,
    /// Ignore `PAREDIT_*`.
    pub skip_environment: bool,
    /// A snapshot of the environment, taken once by the caller.
    pub environment: Vec<(String, String)>,
    /// Where the user layer lives, already resolved.
    pub user_config_dir: Option<PathBuf>,
}

impl LoadOptions {
    /// Reads the ambient environment once, and never again.
    ///
    /// `PAREDIT_CONFIG_HOME` wins outright, then `XDG_CONFIG_HOME/paredit`,
    /// then `~/.config/paredit`. A user with none of the three simply has no
    /// user layer, which is not an error.
    #[must_use]
    pub fn from_process(start: impl Into<PathBuf>) -> Self {
        let environment: Vec<(String, String)> = std::env::vars().collect();
        let lookup = |name: &str| {
            environment
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| PathBuf::from(value))
        };
        let user_config_dir = lookup(CONFIG_HOME_VAR)
            .or_else(|| lookup("XDG_CONFIG_HOME").map(|dir| dir.join("paredit")))
            .or_else(|| lookup("HOME").map(|home| home.join(".config").join("paredit")));

        Self {
            start: start.into(),
            explicit: None,
            skip_files: false,
            skip_environment: false,
            environment,
            user_config_dir,
        }
    }

    fn variable(&self, name: &str) -> Option<&str> {
        self.environment
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// One file that contributed to the merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub layer: Layer,
    pub path: PathBuf,
    /// The file that pulled this one in with `extends`, when it was not
    /// discovered on its own.
    pub extended_by: Option<PathBuf>,
    /// How many keys it set, after its own `extends` line is discounted.
    pub entry_count: usize,
}

/// The outcome of a load: what is configured, what is wrong with it, and every
/// file that was read to decide.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub settings: Settings,
    pub diagnostics: Vec<Diagnostic>,
    /// In application order, lowest precedence first.
    pub sources: Vec<Source>,
    /// The repository root discovery found, when there was one.
    pub repository_root: Option<PathBuf>,
}

impl Loaded {
    /// Whether anything error-severity was found. The `config check` verdict.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == crate::error::Severity::Error)
    }
}

/// Discovers, reads, merges, and validates the configuration.
///
/// # Errors
///
/// Fails only when a file cannot be read or does not parse, or when `extends`
/// loops. A file that parses but says something unrecognised produces
/// diagnostics and a usable [`Settings`], not an error.
pub fn load(options: &LoadOptions, vocabulary: Option<Vocabulary<'_>>) -> ConfigResult<Loaded> {
    let mut settings = Settings::with_defaults();
    let mut diagnostics = Vec::new();
    let mut sources = Vec::new();

    let repository_root = if options.skip_files {
        None
    } else {
        find_repository_root(&options.start)
    };

    for (layer, path) in discover(options, repository_root.as_deref())? {
        let mut visiting = Vec::new();
        apply_file(
            &path,
            layer,
            None,
            0,
            &mut visiting,
            &mut settings,
            &mut diagnostics,
            &mut sources,
            vocabulary,
        )?;
    }

    if !options.skip_environment {
        apply_environment(options, &mut settings, &mut diagnostics, vocabulary);
    }

    diagnostics.extend(check_conflicts(&settings));

    Ok(Loaded {
        settings,
        diagnostics,
        sources,
        repository_root,
    })
}

/// The files to read, in application order.
fn discover(
    options: &LoadOptions,
    repository_root: Option<&Path>,
) -> ConfigResult<Vec<(Layer, PathBuf)>> {
    if let Some(explicit) = &options.explicit {
        if !explicit.is_file() {
            return Err(ConfigError::ExplicitMissing {
                path: explicit.clone(),
            });
        }
        return Ok(vec![(Layer::Explicit, explicit.clone())]);
    }
    if options.skip_files {
        return Ok(Vec::new());
    }

    let mut found = Vec::new();
    let mut claimed = Vec::new();

    if let Some(user_dir) = &options.user_config_dir {
        if let Some(path) = config_file_in(user_dir) {
            claimed.push(canonical(&path));
            found.push((Layer::User, path));
        }
    }

    if let Some(root) = repository_root {
        if let Some(path) = config_file_in(root) {
            claimed.push(canonical(&path));
            found.push((Layer::Repository, path));
        }
    }

    // Shallowest first, so a deeper directory's file replaces a shallower
    // one's — the same direction as the layers above it.
    for directory in directories_between(repository_root, &options.start) {
        let Some(path) = config_file_in(&directory) else {
            continue;
        };
        // The repository root's own file is already in, as the repository
        // layer. Reading it again as a directory layer would only relabel it.
        if claimed.contains(&canonical(&path)) {
            continue;
        }
        claimed.push(canonical(&path));
        found.push((Layer::Directory, path));
    }

    Ok(found)
}

/// Every directory from just below the repository root down to `start`.
///
/// With no repository root there is nothing to bound the walk, so only `start`
/// itself is considered: walking to the filesystem root would let a stray
/// `/tmp/paredit.toml` configure an unrelated project.
fn directories_between(repository_root: Option<&Path>, start: &Path) -> Vec<PathBuf> {
    let Some(root) = repository_root else {
        return vec![start.to_path_buf()];
    };

    let mut chain = Vec::new();
    let mut current = Some(start);
    while let Some(directory) = current {
        if directory == root {
            break;
        }
        chain.push(directory.to_path_buf());
        current = directory.parent();
    }
    chain.reverse();
    chain
}

fn config_file_in(directory: &Path) -> Option<PathBuf> {
    CONFIG_FILE_NAMES
        .iter()
        .map(|name| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// The nearest ancestor holding a `.git`.
///
/// `.git` is a file rather than a directory inside a worktree, so both count.
fn find_repository_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(directory) = current {
        if directory.join(".git").exists() {
            return Some(directory.to_path_buf());
        }
        current = directory.parent();
    }
    None
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[expect(
    clippy::too_many_arguments,
    reason = "one recursive walk threading the four accumulators it fills; \
              bundling them into a context struct would only rename the arguments"
)]
fn apply_file(
    path: &Path,
    layer: Layer,
    extended_by: Option<&Path>,
    depth: usize,
    visiting: &mut Vec<PathBuf>,
    settings: &mut Settings,
    diagnostics: &mut Vec<Diagnostic>,
    sources: &mut Vec<Source>,
    vocabulary: Option<Vocabulary<'_>>,
) -> ConfigResult<()> {
    if depth > MAX_EXTENDS_DEPTH {
        return Err(ConfigError::ExtendsTooDeep {
            limit: MAX_EXTENDS_DEPTH,
        });
    }

    let identity = canonical(path);
    if visiting.contains(&identity) {
        let mut chain = visiting.clone();
        chain.push(identity);
        return Err(ConfigError::ExtendsCycle { chain });
    }
    visiting.push(identity);

    let document = read_document(path)?;

    // `extends` is applied first and beneath: a file always wins over what it
    // inherits, which is the only reading of "extends" anyone expects.
    for target in extends_targets(path, &document, diagnostics) {
        if !target.is_file() {
            return Err(ConfigError::ExtendsMissing {
                path: path.to_path_buf(),
                missing: target,
            });
        }
        apply_file(
            &target,
            layer,
            Some(path),
            depth + 1,
            visiting,
            settings,
            diagnostics,
            sources,
            vocabulary,
        )?;
    }

    let mut entry_count = 0;
    for entry in &document.entries {
        // Consumed by the loader, not merged: the merged value of a per-file
        // directive would be meaningless, and `sources` already reports the
        // resolved chain in the order it was applied.
        if entry.key == "extends" {
            continue;
        }
        entry_count += 1;
        diagnostics.extend(settings.apply(
            &entry.key,
            &entry.value,
            &Origin::file(layer, path, entry.line),
            vocabulary,
        ));
    }

    sources.push(Source {
        layer,
        path: path.to_path_buf(),
        extended_by: extended_by.map(Path::to_path_buf),
        entry_count,
    });
    visiting.pop();
    Ok(())
}

/// The files one document's `extends` names, resolved against its directory.
fn extends_targets(
    path: &Path,
    document: &toml::Document,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<PathBuf> {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut targets = Vec::new();

    for entry in &document.entries {
        if entry.key != "extends" {
            continue;
        }
        let items = match &entry.value {
            Value::Array(items) => items.clone(),
            // A bare string is the obvious single-parent spelling, and
            // refusing it would be pedantry rather than safety.
            single @ Value::String(_) => vec![single.clone()],
            other => {
                diagnostics.push(Diagnostic {
                    code: DiagnosticCode::WrongType,
                    path: Some(path.to_path_buf()),
                    line: Some(entry.line),
                    key: "extends".to_owned(),
                    message: format!(
                        "`extends` expects a string or an array of strings, found {}",
                        other.kind()
                    ),
                    suggestion: None,
                });
                continue;
            }
        };
        for item in items {
            if let Value::String(text) = item {
                let candidate = Path::new(&text);
                targets.push(if candidate.is_absolute() {
                    candidate.to_path_buf()
                } else {
                    base.join(candidate)
                });
            }
        }
    }
    targets
}

fn read_document(path: &Path) -> ConfigResult<toml::Document> {
    let metadata = std::fs::metadata(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(ConfigError::TooLarge {
            path: path.to_path_buf(),
            limit: MAX_CONFIG_BYTES,
        });
    }

    let bytes = std::fs::read(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let text = String::from_utf8(bytes).map_err(|_| ConfigError::NotUtf8 {
        path: path.to_path_buf(),
    })?;

    toml::parse(&text).map_err(|source| ConfigError::Syntax {
        path: path.to_path_buf(),
        source,
    })
}

/// Applies `PAREDIT_<KEY>` for every schema key that has one set.
fn apply_environment(
    options: &LoadOptions,
    settings: &mut Settings,
    diagnostics: &mut Vec<Diagnostic>,
    vocabulary: Option<Vocabulary<'_>>,
) {
    for entry in &schema::SCHEMA {
        // Loader-only, exactly as in a file. An `extends` that could be set
        // from the environment would make a CI run's configuration depend on
        // a variable nobody reading the file can see.
        if entry.key == "extends" {
            continue;
        }
        let variable = entry.env_var();
        let Some(raw) = options.variable(&variable) else {
            continue;
        };
        let origin = Origin::environment(&variable);
        match parse_environment_value(entry, raw) {
            Ok(value) => {
                diagnostics.extend(settings.apply(entry.key, &value, &origin, vocabulary));
            }
            Err(message) => diagnostics.push(Diagnostic {
                code: DiagnosticCode::WrongType,
                path: None,
                line: None,
                key: entry.key.to_owned(),
                message,
                suggestion: None,
            }),
        }
    }
}

/// Turns a variable's raw text into the value its key expects.
///
/// Lists are comma-separated. That is the only spelling an environment can
/// carry, so it is the spelling: `PAREDIT_LINT_DISABLE=a,b,c`.
fn parse_environment_value(entry: &schema::KeySchema, raw: &str) -> Result<Value, String> {
    use schema::ValueKind;

    let variable = entry.env_var();
    match entry.kind {
        ValueKind::Boolean => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(Value::Boolean(true)),
            "0" | "false" | "no" | "off" => Ok(Value::Boolean(false)),
            other => Err(format!(
                "{variable}=\"{other}\" is not a boolean (use true or false)"
            )),
        },
        ValueKind::Integer { .. } => raw
            .trim()
            .parse::<i64>()
            .map(Value::Integer)
            .map_err(|_| format!("{variable}=\"{raw}\" is not an integer")),
        ValueKind::Text | ValueKind::Choice(_) => Ok(Value::String(raw.trim().to_owned())),
        kind if kind.is_list() => Ok(Value::Array(
            raw.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| Value::String(item.to_owned()))
                .collect(),
        )),
        _ => Err(format!("{variable} cannot be set from the environment")),
    }
}

/// Settings that are individually valid and jointly contradictory.
fn check_conflicts(settings: &Settings) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let enabled = settings.list("lint.enable");
    let disabled = settings.list("lint.disable");
    if !enabled.is_empty() && !disabled.is_empty() {
        let origin = settings
            .origin("lint.disable")
            .cloned()
            .unwrap_or_else(Origin::default_layer);
        diagnostics.push(Diagnostic {
            code: DiagnosticCode::Conflict,
            path: origin.path.clone(),
            line: origin.line,
            key: "lint.disable".to_owned(),
            message: "`lint.enable` and `lint.disable` cannot both be set: \
                      `lint.enable` is already an exhaustive list"
                .to_owned(),
            suggestion: Some(format!(
                "drop one of them (`lint.enable` was set at {})",
                settings
                    .origin("lint.enable")
                    .map_or_else(|| "an unknown place".to_owned(), Origin::describe)
            )),
        });
    }

    let overlapping: Vec<&str> = settings
        .list("lint.deny")
        .into_iter()
        .filter(|name| settings.list("lint.warn").contains(name))
        .collect();
    if !overlapping.is_empty() {
        let origin = settings
            .origin("lint.warn")
            .cloned()
            .unwrap_or_else(Origin::default_layer);
        diagnostics.push(Diagnostic {
            code: DiagnosticCode::Conflict,
            path: origin.path.clone(),
            line: origin.line,
            key: "lint.warn".to_owned(),
            message: format!(
                "{} appears in both `lint.deny` and `lint.warn`",
                overlapping
                    .iter()
                    .map(|name| format!("\"{name}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            suggestion: Some("keep whichever severity you meant".to_owned()),
        });
    }

    diagnostics
}

/// A snapshot of the schema's environment variables and their current values,
/// for `config show`. Keyed by variable name.
#[must_use]
pub fn environment_overrides(options: &LoadOptions) -> BTreeMap<String, String> {
    schema::SCHEMA
        .iter()
        .filter_map(|entry| {
            let variable = entry.env_var();
            options
                .variable(&variable)
                .map(|value| (variable, value.to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A scratch directory that removes itself, so these tests can exercise
    /// real discovery rather than a mocked filesystem — discovery *is* the
    /// thing under test, and mocking it would test nothing.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("paredit-config-{name}-{}", std::process::id()));
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

        fn dir(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(&path).expect("create dir");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn options(start: PathBuf) -> LoadOptions {
        LoadOptions {
            start,
            ..LoadOptions::default()
        }
    }

    #[test]
    fn with_no_files_anywhere_the_defaults_stand() {
        let scratch = Scratch::new("empty");
        let loaded = load(&options(scratch.dir("work")), None).expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(2));
        assert!(loaded.sources.is_empty());
        assert!(!loaded.has_errors());
    }

    #[test]
    fn a_repository_file_is_found_from_a_subdirectory() {
        let scratch = Scratch::new("repo");
        scratch.dir(".git");
        scratch.write("paredit.toml", "[format]\nindent = 4\n");
        let start = scratch.dir("src/deep");

        let loaded = load(&options(start), None).expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(4));
        assert_eq!(loaded.sources[0].layer, Layer::Repository);
    }

    /// The layering H2 asks for, checked end to end: the deeper file wins, and
    /// the key it does not set keeps the shallower file's value.
    #[test]
    fn a_directory_file_overrides_the_repository_and_leaves_the_rest() {
        let scratch = Scratch::new("layers");
        scratch.dir(".git");
        scratch.write(
            "paredit.toml",
            "[format]\nindent = 4\n[lint]\npreset = \"all\"\n",
        );
        scratch.write("src/paredit.toml", "[format]\nindent = 8\n");
        let start = scratch.dir("src");

        let loaded = load(&options(start), None).expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(8));
        assert_eq!(loaded.settings.text("lint.preset"), Some("all"));
        assert_eq!(
            loaded.settings.origin("format.indent").expect("set").layer,
            Layer::Directory
        );
        assert_eq!(
            loaded.settings.origin("lint.preset").expect("set").layer,
            Layer::Repository
        );
    }

    #[test]
    fn the_repository_root_file_is_not_also_read_as_a_directory_layer() {
        let scratch = Scratch::new("dedupe");
        scratch.dir(".git");
        scratch.write("paredit.toml", "[format]\nindent = 4\n");

        let loaded = load(&options(scratch.0.clone()), None).expect("loads");
        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.sources[0].layer, Layer::Repository);
    }

    #[test]
    fn a_hidden_file_name_is_accepted_when_the_plain_one_is_absent() {
        let scratch = Scratch::new("hidden");
        scratch.dir(".git");
        scratch.write(".paredit.toml", "[format]\nindent = 6\n");
        let loaded = load(&options(scratch.0.clone()), None).expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(6));
    }

    #[test]
    fn a_user_file_sits_beneath_the_repository() {
        let scratch = Scratch::new("user");
        scratch.dir(".git");
        let user_dir = scratch.dir("home/paredit");
        fs::write(user_dir.join("paredit.toml"), "[format]\nindent = 3\n").expect("write");
        scratch.write("paredit.toml", "[lint]\npreset = \"minimal\"\n");

        let loaded = load(
            &LoadOptions {
                start: scratch.0.clone(),
                user_config_dir: Some(user_dir),
                ..LoadOptions::default()
            },
            None,
        )
        .expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(3));
        assert_eq!(loaded.settings.text("lint.preset"), Some("minimal"));
        assert_eq!(loaded.sources[0].layer, Layer::User);
        assert_eq!(loaded.sources[1].layer, Layer::Repository);
    }

    #[test]
    fn extends_is_applied_beneath_the_file_that_names_it() {
        let scratch = Scratch::new("extends");
        scratch.dir(".git");
        scratch.write(
            "shared.toml",
            "[format]\nindent = 4\n[lint]\npreset = \"all\"\n",
        );
        scratch.write(
            "paredit.toml",
            "extends = [\"shared.toml\"]\n[format]\nindent = 8\n",
        );

        let loaded = load(&options(scratch.0.clone()), None).expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(8));
        assert_eq!(loaded.settings.text("lint.preset"), Some("all"));
        assert_eq!(loaded.sources.len(), 2);
        assert!(loaded.sources[0].extended_by.is_some());
        assert!(loaded.sources[1].extended_by.is_none());
    }

    #[test]
    fn extends_accepts_a_bare_string() {
        let scratch = Scratch::new("extends-string");
        scratch.dir(".git");
        scratch.write("shared.toml", "[format]\nindent = 4\n");
        scratch.write("paredit.toml", "extends = \"shared.toml\"\n");

        let loaded = load(&options(scratch.0.clone()), None).expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(4));
    }

    #[test]
    fn extends_resolves_relative_to_the_file_that_wrote_it() {
        let scratch = Scratch::new("extends-relative");
        scratch.dir(".git");
        scratch.write("shared/base.toml", "[format]\nindent = 5\n");
        scratch.write(
            "nested/paredit.toml",
            "extends = [\"../shared/base.toml\"]\n",
        );

        let loaded = load(&options(scratch.dir("nested")), None).expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(5));
    }

    #[test]
    fn an_extends_cycle_is_an_error_rather_than_a_hang() {
        let scratch = Scratch::new("cycle");
        scratch.dir(".git");
        scratch.write("paredit.toml", "extends = [\"b.toml\"]\n");
        scratch.write("b.toml", "extends = [\"paredit.toml\"]\n");

        let error = load(&options(scratch.0.clone()), None).unwrap_err();
        assert!(
            matches!(error, ConfigError::ExtendsCycle { .. }),
            "expected a cycle, got {error}"
        );
    }

    #[test]
    fn extending_a_file_that_is_not_there_names_both_files() {
        let scratch = Scratch::new("extends-missing");
        scratch.dir(".git");
        scratch.write("paredit.toml", "extends = [\"nope.toml\"]\n");

        let error = load(&options(scratch.0.clone()), None).unwrap_err();
        assert!(matches!(error, ConfigError::ExtendsMissing { .. }));
    }

    #[test]
    fn an_environment_variable_beats_every_file() {
        let scratch = Scratch::new("env");
        scratch.dir(".git");
        scratch.write("paredit.toml", "[format]\nindent = 4\n");

        let loaded = load(
            &LoadOptions {
                start: scratch.0.clone(),
                environment: vec![("PAREDIT_FORMAT_INDENT".to_owned(), "7".to_owned())],
                ..LoadOptions::default()
            },
            None,
        )
        .expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(7));
        assert_eq!(
            loaded.settings.origin("format.indent").expect("set").layer,
            Layer::Environment
        );
    }

    #[test]
    fn an_environment_list_is_comma_separated() {
        let loaded = load(
            &LoadOptions {
                start: PathBuf::from("."),
                skip_files: true,
                environment: vec![(
                    "PAREDIT_PATHS_EXCLUDE".to_owned(),
                    "vendor, third-party ,".to_owned(),
                )],
                ..LoadOptions::default()
            },
            None,
        )
        .expect("loads");
        assert_eq!(
            loaded.settings.list("paths.exclude"),
            vec!["vendor", "third-party"]
        );
    }

    #[test]
    fn an_unparsable_environment_value_is_reported_and_ignored() {
        let loaded = load(
            &LoadOptions {
                start: PathBuf::from("."),
                skip_files: true,
                environment: vec![("PAREDIT_FORMAT_INDENT".to_owned(), "wide".to_owned())],
                ..LoadOptions::default()
            },
            None,
        )
        .expect("loads");
        assert!(loaded.has_errors());
        assert_eq!(loaded.settings.integer("format.indent"), Some(2));
    }

    #[test]
    fn skipping_the_environment_leaves_the_files_in_charge() {
        let scratch = Scratch::new("skip-env");
        scratch.dir(".git");
        scratch.write("paredit.toml", "[format]\nindent = 4\n");

        let loaded = load(
            &LoadOptions {
                start: scratch.0.clone(),
                skip_environment: true,
                environment: vec![("PAREDIT_FORMAT_INDENT".to_owned(), "7".to_owned())],
                ..LoadOptions::default()
            },
            None,
        )
        .expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(4));
    }

    #[test]
    fn an_explicit_file_replaces_discovery_entirely() {
        let scratch = Scratch::new("explicit");
        scratch.dir(".git");
        scratch.write("paredit.toml", "[format]\nindent = 4\n");
        let named = scratch.write("elsewhere.toml", "[lint]\npreset = \"all\"\n");

        let loaded = load(
            &LoadOptions {
                start: scratch.0.clone(),
                explicit: Some(named),
                ..LoadOptions::default()
            },
            None,
        )
        .expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(2));
        assert_eq!(loaded.settings.text("lint.preset"), Some("all"));
        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.sources[0].layer, Layer::Explicit);
    }

    #[test]
    fn naming_a_config_file_that_is_not_there_is_an_error() {
        let error = load(
            &LoadOptions {
                start: PathBuf::from("."),
                explicit: Some(PathBuf::from("/definitely/not/here.toml")),
                ..LoadOptions::default()
            },
            None,
        )
        .unwrap_err();
        assert!(matches!(error, ConfigError::ExplicitMissing { .. }));
    }

    #[test]
    fn skip_files_reads_nothing_even_with_a_file_right_there() {
        let scratch = Scratch::new("skip-files");
        scratch.dir(".git");
        scratch.write("paredit.toml", "[format]\nindent = 4\n");

        let loaded = load(
            &LoadOptions {
                start: scratch.0.clone(),
                skip_files: true,
                ..LoadOptions::default()
            },
            None,
        )
        .expect("loads");
        assert_eq!(loaded.settings.integer("format.indent"), Some(2));
        assert!(loaded.sources.is_empty());
    }

    #[test]
    fn a_syntax_error_names_the_file_and_the_line() {
        let scratch = Scratch::new("syntax");
        scratch.dir(".git");
        scratch.write("paredit.toml", "[format]\nindent = 4\nbroken\n");

        let error = load(&options(scratch.0.clone()), None).unwrap_err();
        let ConfigError::Syntax { source, .. } = &error else {
            panic!("expected a syntax error, got {error}");
        };
        assert_eq!(source.line, 3);
    }

    #[test]
    fn enable_and_disable_together_are_reported_as_a_conflict() {
        let scratch = Scratch::new("conflict");
        scratch.dir(".git");
        scratch.write(
            "paredit.toml",
            "[lint]\nenable = [\"a\"]\ndisable = [\"b\"]\n",
        );

        let loaded = load(&options(scratch.0.clone()), None).expect("loads");
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::Conflict)
        );
        assert!(loaded.has_errors());
    }

    #[test]
    fn a_rule_in_both_deny_and_warn_is_a_conflict() {
        let scratch = Scratch::new("severity-conflict");
        scratch.dir(".git");
        scratch.write("paredit.toml", "[lint]\ndeny = [\"a\"]\nwarn = [\"a\"]\n");

        let loaded = load(&options(scratch.0.clone()), None).expect("loads");
        let conflict = loaded
            .diagnostics
            .iter()
            .find(|d| d.code == DiagnosticCode::Conflict)
            .expect("reports the overlap");
        assert!(conflict.message.contains("\"a\""));
    }

    #[test]
    fn a_relative_exclude_path_resolves_against_the_file_that_set_it() {
        let scratch = Scratch::new("relative-exclude");
        scratch.dir(".git");
        scratch.write("nested/paredit.toml", "[paths]\nexclude = [\"vendor\"]\n");

        let loaded = load(&options(scratch.dir("nested")), None).expect("loads");
        let excluded = loaded.settings.list("paths.exclude");
        assert!(
            excluded[0].ends_with("nested/vendor"),
            "expected a path under nested/, got {excluded:?}"
        );
    }

    #[test]
    fn the_environment_snapshot_reports_only_schema_variables() {
        let overrides = environment_overrides(&LoadOptions {
            environment: vec![
                ("PAREDIT_FORMAT_INDENT".to_owned(), "4".to_owned()),
                ("PAREDIT_NOT_A_KEY".to_owned(), "x".to_owned()),
                ("PATH".to_owned(), "/usr/bin".to_owned()),
            ],
            ..LoadOptions::default()
        });
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides["PAREDIT_FORMAT_INDENT"], "4");
    }

    #[test]
    fn extends_may_not_be_set_from_the_environment() {
        let loaded = load(
            &LoadOptions {
                start: PathBuf::from("."),
                skip_files: true,
                environment: vec![("PAREDIT_EXTENDS".to_owned(), "/etc/evil.toml".to_owned())],
                ..LoadOptions::default()
            },
            None,
        )
        .expect("loads");
        assert!(loaded.sources.is_empty());
        assert!(loaded.diagnostics.is_empty());
    }
}
