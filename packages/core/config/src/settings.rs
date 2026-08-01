//! The merged configuration, and the record of which layer won each key.
//!
//! Every key carries an [`Origin`]. That is the whole point of the type: a
//! setting whose source nobody can name is a setting nobody trusts, and with
//! five layers and `extends` in play, "why is `lint.preset` `all` here?" is a
//! question that comes up immediately and has no other answer.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value as Json, json};

use crate::error::{Diagnostic, DiagnosticCode};
use crate::schema::{self, DefaultValue, KeySchema, ValueKind};
use crate::toml::Value;

/// Where a value came from, coarsest first. Ordering is precedence: a later
/// layer replaces an earlier one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// Built into [`schema::SCHEMA`].
    Default,
    /// The user's own configuration directory.
    User,
    /// Beside the nearest ancestor `.git`.
    Repository,
    /// A `paredit.toml` between the repository root and the start directory.
    Directory,
    /// Named outright with `--config`.
    Explicit,
    /// A `PAREDIT_*` environment variable.
    Environment,
    /// A command-line flag. Recorded so `config show` can say that a file was
    /// read and then overridden, rather than appearing to have been ignored.
    Flag,
}

impl Layer {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::User => "user",
            Self::Repository => "repository",
            Self::Directory => "directory",
            Self::Explicit => "explicit",
            Self::Environment => "environment",
            Self::Flag => "flag",
        }
    }
}

/// The exact place a value was set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub layer: Layer,
    /// The file, for a file layer.
    pub path: Option<PathBuf>,
    /// 1-based line within `path`.
    pub line: Option<usize>,
    /// The environment variable or flag name, for those layers.
    pub detail: Option<String>,
}

impl Origin {
    #[must_use]
    pub const fn default_layer() -> Self {
        Self {
            layer: Layer::Default,
            path: None,
            line: None,
            detail: None,
        }
    }

    #[must_use]
    pub fn file(layer: Layer, path: impl Into<PathBuf>, line: usize) -> Self {
        Self {
            layer,
            path: Some(path.into()),
            line: Some(line),
            detail: None,
        }
    }

    #[must_use]
    pub fn environment(variable: impl Into<String>) -> Self {
        Self {
            layer: Layer::Environment,
            path: None,
            line: None,
            detail: Some(variable.into()),
        }
    }

    #[must_use]
    pub fn flag(name: impl Into<String>) -> Self {
        Self {
            layer: Layer::Flag,
            path: None,
            line: None,
            detail: Some(name.into()),
        }
    }

    /// One line naming the source, the way `config show` prints it.
    #[must_use]
    pub fn describe(&self) -> String {
        match (&self.path, self.line, &self.detail) {
            (Some(path), Some(line), _) => format!("{}:{line}", path.display()),
            (Some(path), None, _) => path.display().to_string(),
            (None, _, Some(detail)) => detail.clone(),
            (None, _, None) => "built-in".to_owned(),
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Json {
        json!({
            "layer": self.layer.label(),
            "path": self.path.as_ref().map(|path| path.display().to_string()),
            "line": self.line,
            "detail": self.detail,
            "describe": self.describe(),
        })
    }
}

/// One key's winning value and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub value: Value,
    pub origin: Origin,
}

/// The merged configuration.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    values: BTreeMap<&'static str, Resolved>,
}

/// The rule and category names a build actually registers.
///
/// Supplied by the caller rather than read here: the lint registry is
/// composition root (it names every feature package), and a core package that
/// reached for it would invert the dependency the workspace is built on.
#[derive(Debug, Clone, Copy)]
pub struct Vocabulary<'a> {
    pub rules: &'a [&'a str],
    pub categories: &'a [&'a str],
}

impl Settings {
    /// Every key at its built-in default. The base of every merge.
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut values = BTreeMap::new();
        for entry in &schema::SCHEMA {
            if let Some(value) = default_value(entry) {
                values.insert(
                    entry.key,
                    Resolved {
                        value,
                        origin: Origin::default_layer(),
                    },
                );
            }
        }
        Self { values }
    }

    /// Validates one key against the schema and, if it holds up, records it.
    ///
    /// Returns the diagnostics the value produced. A value that produced any
    /// error-severity diagnostic is *not* recorded: applying a value this
    /// build does not understand would be worse than leaving the previous
    /// layer's, and worse still than the default.
    pub fn apply(
        &mut self,
        key: &str,
        value: &Value,
        origin: &Origin,
        vocabulary: Option<Vocabulary<'_>>,
    ) -> Vec<Diagnostic> {
        let Some(entry) = schema::lookup(key) else {
            return vec![diagnostic(
                DiagnosticCode::UnknownKey,
                origin,
                key,
                format!("`{key}` is not a recognised configuration key"),
                schema::nearest_key(key).map(|near| format!("did you mean `{near}`?")),
            )];
        };

        let mut diagnostics = Vec::new();
        for (code, message, suggestion) in check(entry, value, vocabulary) {
            diagnostics.push(diagnostic(code, origin, key, message, suggestion));
        }
        if diagnostics
            .iter()
            .any(|d| d.severity() == crate::error::Severity::Error)
        {
            return diagnostics;
        }

        let value = resolve_relative_paths(entry, value, origin);
        self.values.insert(
            entry.key,
            Resolved {
                value,
                origin: origin.clone(),
            },
        );
        diagnostics
    }

    /// Records a value that did not come from a file — a flag, typically —
    /// without re-validating it. The caller's type system already checked it.
    pub fn set(&mut self, key: &'static str, value: Value, origin: Origin) {
        debug_assert!(
            schema::lookup(key).is_some(),
            "`{key}` is not in the configuration schema"
        );
        self.values.insert(key, Resolved { value, origin });
    }

    #[must_use]
    pub fn resolved(&self, key: &str) -> Option<&Resolved> {
        self.values.get(key)
    }

    #[must_use]
    pub fn origin(&self, key: &str) -> Option<&Origin> {
        self.resolved(key).map(|resolved| &resolved.origin)
    }

    #[must_use]
    pub fn boolean(&self, key: &str) -> Option<bool> {
        match self.resolved(key)?.value {
            Value::Boolean(flag) => Some(flag),
            _ => None,
        }
    }

    #[must_use]
    pub fn integer(&self, key: &str) -> Option<i64> {
        match self.resolved(key)?.value {
            Value::Integer(number) => Some(number),
            _ => None,
        }
    }

    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        match &self.resolved(key)?.value {
            Value::String(text) => Some(text),
            _ => None,
        }
    }

    /// A list key's items, or an empty slice when unset. Empty and unset are
    /// deliberately the same here: every list key means "no restriction" empty.
    #[must_use]
    pub fn list(&self, key: &str) -> Vec<&str> {
        let Some(Resolved {
            value: Value::Array(items),
            ..
        }) = self.resolved(key)
        else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| match item {
                Value::String(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Every declared key with its value and origin, in schema order.
    ///
    /// Schema order rather than alphabetical so `config show` groups a table's
    /// keys together and reads like the file it describes.
    #[must_use]
    pub fn entries(&self) -> Vec<(&'static KeySchema, Option<&Resolved>)> {
        schema::SCHEMA
            .iter()
            .map(|entry| (entry, self.values.get(entry.key)))
            .collect()
    }

    /// Whether any layer above the default set this key.
    #[must_use]
    pub fn is_customised(&self, key: &str) -> bool {
        self.origin(key)
            .is_some_and(|origin| origin.layer != Layer::Default)
    }
}

fn default_value(entry: &KeySchema) -> Option<Value> {
    match entry.default {
        DefaultValue::Unset => None,
        DefaultValue::Boolean(flag) => Some(Value::Boolean(flag)),
        DefaultValue::Integer(number) => Some(Value::Integer(number)),
        DefaultValue::Text(text) => Some(Value::String(text.to_owned())),
        DefaultValue::EmptyList => Some(Value::Array(Vec::new())),
    }
}

fn diagnostic(
    code: DiagnosticCode,
    origin: &Origin,
    key: &str,
    message: String,
    suggestion: Option<String>,
) -> Diagnostic {
    Diagnostic {
        code,
        path: origin.path.clone(),
        line: origin.line,
        key: key.to_owned(),
        message,
        suggestion,
    }
}

type Finding = (DiagnosticCode, String, Option<String>);

/// Checks one value against its declared kind.
fn check(entry: &KeySchema, value: &Value, vocabulary: Option<Vocabulary<'_>>) -> Vec<Finding> {
    let key = entry.key;
    match (entry.kind, value) {
        (ValueKind::Boolean, Value::Boolean(_)) | (ValueKind::Text, Value::String(_)) => Vec::new(),

        (ValueKind::Integer { min, max }, Value::Integer(number)) => {
            if (min..=max).contains(number) {
                Vec::new()
            } else {
                vec![(
                    DiagnosticCode::OutOfRange,
                    format!("`{key}` is {number}, outside the accepted range {min}..={max}"),
                    None,
                )]
            }
        }

        (ValueKind::Choice(choices), Value::String(text)) => {
            if choices.contains(&text.as_str()) {
                Vec::new()
            } else {
                vec![(
                    DiagnosticCode::NotAChoice,
                    format!("`{key}` does not accept \"{text}\""),
                    Some(format!("accepted values: {}", choices.join(", "))),
                )]
            }
        }

        (kind, Value::Array(items)) if kind.is_list() => check_list(entry, items, vocabulary),

        (kind, other) => vec![(
            DiagnosticCode::WrongType,
            format!(
                "`{key}` expects {}, found {}",
                describe_kind(kind),
                other.kind()
            ),
            None,
        )],
    }
}

fn check_list(
    entry: &KeySchema,
    items: &[Value],
    vocabulary: Option<Vocabulary<'_>>,
) -> Vec<Finding> {
    let key = entry.key;
    let mut findings = Vec::new();

    for item in items {
        let Value::String(name) = item else {
            findings.push((
                DiagnosticCode::WrongType,
                format!(
                    "`{key}` expects an array of strings, found a {}",
                    item.kind()
                ),
                None,
            ));
            continue;
        };
        if name.is_empty() {
            findings.push((
                DiagnosticCode::EmptyValue,
                format!("`{key}` contains an empty string, which matches nothing"),
                Some("remove the empty entry".to_owned()),
            ));
            continue;
        }

        match entry.kind {
            ValueKind::IndentStyleEntries => {
                if let Some(finding) = check_indent_style_entry(key, name) {
                    findings.push(finding);
                }
                continue;
            }
            ValueKind::WidthProfileEntries => {
                if let Some(finding) = check_width_profile_entry(key, name) {
                    findings.push(finding);
                }
                continue;
            }
            _ => {}
        }

        let Some(vocabulary) = vocabulary else {
            continue;
        };
        match entry.kind {
            ValueKind::RuleList if !vocabulary.rules.contains(&name.as_str()) => {
                findings.push(unknown_name(key, name, vocabulary.rules, false));
            }
            ValueKind::RuleOrCategoryList
                if !vocabulary.rules.contains(&name.as_str())
                    && !vocabulary.categories.contains(&name.as_str()) =>
            {
                findings.push(unknown_name(key, name, vocabulary.rules, true));
            }
            _ => {}
        }
    }
    findings
}

/// Checks one `format.indent-table` entry: `symbol=style`, `style` in
/// [`paredit_core_syntax::sexpr::STYLE_NAMES`].
///
/// The style vocabulary lives in the syntax package, not behind a
/// caller-supplied [`Vocabulary`] the way lint rule names are: `config`
/// already depends on `syntax` directly (see `DIALECT_CHOICES` above), and
/// unlike the lint registry — which is assembled from whichever feature
/// packages a build actually links — the style list is intrinsic to the
/// formatter and identical in every build.
fn check_indent_style_entry(key: &str, entry: &str) -> Option<Finding> {
    let Some((symbol, style)) = entry.split_once('=') else {
        return Some((
            DiagnosticCode::WrongType,
            format!("`{key}` entry \"{entry}\" is not written as `symbol=style`"),
            Some("write it as e.g. \"my-macro=definition\"".to_owned()),
        ));
    };
    if symbol.is_empty() {
        return Some((
            DiagnosticCode::EmptyValue,
            format!("`{key}` entry \"{entry}\" names no symbol before `=`"),
            None,
        ));
    }
    unknown_style(key, entry, style)
}

/// Checks one `format.width-profiles` entry: `style=width`, `style` in
/// [`paredit_core_syntax::sexpr::STYLE_NAMES`] and `width` an integer in the
/// same range `format.max-inline-width` accepts.
fn check_width_profile_entry(key: &str, entry: &str) -> Option<Finding> {
    let Some((style, width)) = entry.split_once('=') else {
        return Some((
            DiagnosticCode::WrongType,
            format!("`{key}` entry \"{entry}\" is not written as `style=width`"),
            Some("write it as e.g. \"definition=60\"".to_owned()),
        ));
    };
    if let Some(finding) = unknown_style(key, entry, style) {
        return Some(finding);
    }
    match width.parse::<i64>() {
        Ok(value) if (1..=512).contains(&value) => None,
        Ok(value) => Some((
            DiagnosticCode::OutOfRange,
            format!("`{key}` entry \"{entry}\" is {value}, outside the accepted range 1..=512"),
            None,
        )),
        Err(_) => Some((
            DiagnosticCode::WrongType,
            format!("`{key}` entry \"{entry}\" does not have an integer width"),
            None,
        )),
    }
}

fn unknown_style(key: &str, entry: &str, style: &str) -> Option<Finding> {
    if paredit_core_syntax::sexpr::STYLE_NAMES.contains(&style) {
        return None;
    }
    Some((
        DiagnosticCode::NotAChoice,
        format!(
            "`{key}` entry \"{entry}\" names \"{style}\", which is not a recognised indent style"
        ),
        Some(format!(
            "accepted styles: {}",
            paredit_core_syntax::sexpr::STYLE_NAMES.join(", ")
        )),
    ))
}

fn unknown_name(key: &str, name: &str, rules: &[&str], categories_too: bool) -> Finding {
    let subject = if categories_too {
        "lint rule or category"
    } else {
        "lint rule"
    };
    let nearest = rules
        .iter()
        .map(|rule| (levenshtein(rule, name), *rule))
        .filter(|(distance, _)| *distance <= 3)
        .min_by_key(|(distance, rule)| (*distance, *rule))
        .map(|(_, rule)| rule);
    (
        DiagnosticCode::UnknownRule,
        format!("`{key}` names \"{name}\", which is not a registered {subject}"),
        nearest.map(|rule| format!("did you mean \"{rule}\"?")),
    )
}

fn levenshtein(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (row, left_char) in left.iter().enumerate() {
        current[0] = row + 1;
        for (column, right_char) in right.iter().enumerate() {
            let substitution = usize::from(left_char != right_char);
            current[column + 1] = (previous[column] + substitution)
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

fn describe_kind(kind: ValueKind) -> String {
    match kind {
        ValueKind::Choice(choices) => format!("one of {}", choices.join(", ")),
        ValueKind::Integer { min, max } => format!("an integer in {min}..={max}"),
        ValueKind::Boolean => "a boolean".to_owned(),
        ValueKind::Text => "a string".to_owned(),
        other => format!("an array of strings ({})", other.label()),
    }
}

/// Rewrites the path-valued keys to be relative to the file that set them.
///
/// A `paredit.toml` two directories down that says `baseline = "lint.json"`
/// means the one beside it, not one beside whatever directory the command
/// happened to be run from. Resolving here — once, at apply time — means every
/// consumer downstream gets a path it can just open.
fn resolve_relative_paths(entry: &KeySchema, value: &Value, origin: &Origin) -> Value {
    if !schema::PATH_KEYS.contains(&entry.key) {
        return value.clone();
    }
    let Some(base) = origin.path.as_deref().and_then(Path::parent) else {
        return value.clone();
    };

    match value {
        Value::String(text) => Value::String(join_relative(base, text)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| match item {
                    Value::String(text) => Value::String(join_relative(base, text)),
                    other => other.clone(),
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

fn join_relative(base: &Path, text: &str) -> String {
    let candidate = Path::new(text);
    if candidate.is_absolute() {
        return text.to_owned();
    }
    base.join(candidate).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Severity;

    fn origin() -> Origin {
        Origin::file(Layer::Repository, "/repo/paredit.toml", 4)
    }

    fn vocabulary() -> Vocabulary<'static> {
        Vocabulary {
            rules: &["self-assignment", "nil-comparison"],
            categories: &["correctness"],
        }
    }

    #[test]
    fn defaults_come_from_the_schema_and_are_attributed_to_it() {
        let settings = Settings::with_defaults();
        assert_eq!(settings.integer("format.indent"), Some(2));
        assert_eq!(settings.text("lint.preset"), Some("recommended"));
        assert_eq!(
            settings.origin("format.indent").expect("set").layer,
            Layer::Default
        );
        assert!(!settings.is_customised("format.indent"));
    }

    #[test]
    fn an_unset_key_has_no_value_rather_than_a_zero() {
        let settings = Settings::with_defaults();
        assert_eq!(settings.text("dialect.default"), None);
        assert_eq!(settings.integer("paths.max-depth"), None);
    }

    #[test]
    fn applying_a_value_records_the_file_and_line_that_set_it() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "format.indent",
            &Value::Integer(4),
            &origin(),
            Some(vocabulary()),
        );
        assert!(diagnostics.is_empty());
        assert_eq!(settings.integer("format.indent"), Some(4));
        assert_eq!(
            settings.origin("format.indent").expect("set").describe(),
            "/repo/paredit.toml:4"
        );
        assert!(settings.is_customised("format.indent"));
    }

    #[test]
    fn an_unknown_key_is_reported_with_the_key_it_probably_meant() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "lint.fail_on",
            &Value::String("error".to_owned()),
            &origin(),
            None,
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::UnknownKey);
        assert_eq!(
            diagnostics[0].suggestion.as_deref(),
            Some("did you mean `lint.fail-on`?")
        );
    }

    /// The property that makes a rejected value safe: the previous layer's
    /// value stands. Half-applying a configuration is worse than not applying.
    #[test]
    fn a_rejected_value_leaves_the_previous_one_in_place() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "format.indent",
            &Value::String("four".to_owned()),
            &origin(),
            None,
        );
        assert_eq!(diagnostics[0].code, DiagnosticCode::WrongType);
        assert_eq!(settings.integer("format.indent"), Some(2));
        assert_eq!(
            settings.origin("format.indent").expect("set").layer,
            Layer::Default
        );
    }

    #[test]
    fn a_value_outside_its_range_is_refused_with_the_range() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply("format.indent", &Value::Integer(99), &origin(), None);
        assert_eq!(diagnostics[0].code, DiagnosticCode::OutOfRange);
        assert!(diagnostics[0].message.contains("0..=16"));
    }

    #[test]
    fn a_value_outside_a_closed_set_lists_the_set() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "lint.preset",
            &Value::String("everything".to_owned()),
            &origin(),
            None,
        );
        assert_eq!(diagnostics[0].code, DiagnosticCode::NotAChoice);
        assert!(
            diagnostics[0]
                .suggestion
                .as_deref()
                .expect("lists the choices")
                .contains("recommended")
        );
    }

    #[test]
    fn an_unregistered_rule_name_is_refused_against_the_supplied_vocabulary() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "lint.disable",
            &Value::Array(vec![Value::String("self-assignmnt".to_owned())]),
            &origin(),
            Some(vocabulary()),
        );
        assert_eq!(diagnostics[0].code, DiagnosticCode::UnknownRule);
        assert_eq!(
            diagnostics[0].suggestion.as_deref(),
            Some("did you mean \"self-assignment\"?")
        );
    }

    /// Without a vocabulary there is nothing to check against, and inventing
    /// one would reject every rule in a build that did not supply it.
    #[test]
    fn rule_names_are_not_checked_when_no_vocabulary_is_supplied() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "lint.disable",
            &Value::Array(vec![Value::String("whatever".to_owned())]),
            &origin(),
            None,
        );
        assert!(diagnostics.is_empty());
        assert_eq!(settings.list("lint.disable"), vec!["whatever"]);
    }

    #[test]
    fn a_category_name_satisfies_a_rule_or_category_key() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "lint.deny",
            &Value::Array(vec![Value::String("correctness".to_owned())]),
            &origin(),
            Some(vocabulary()),
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn an_empty_list_entry_warns_without_dropping_the_setting() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "paths.exclude",
            &Value::Array(vec![Value::String(String::new())]),
            &origin(),
            None,
        );
        assert_eq!(diagnostics[0].severity(), Severity::Warning);
        assert!(settings.is_customised("paths.exclude"));
    }

    /// The rule that makes a nested `paredit.toml` usable: its relative paths
    /// mean what someone editing that file would expect.
    #[test]
    fn a_relative_path_resolves_against_the_file_that_set_it() {
        let mut settings = Settings::with_defaults();
        settings.apply(
            "lint.baseline",
            &Value::String("lint.json".to_owned()),
            &Origin::file(Layer::Directory, "/repo/sub/paredit.toml", 2),
            None,
        );
        assert_eq!(settings.text("lint.baseline"), Some("/repo/sub/lint.json"));
    }

    #[test]
    fn an_absolute_path_is_left_alone() {
        let mut settings = Settings::with_defaults();
        settings.apply(
            "lint.baseline",
            &Value::String("/elsewhere/lint.json".to_owned()),
            &Origin::file(Layer::Directory, "/repo/sub/paredit.toml", 2),
            None,
        );
        assert_eq!(settings.text("lint.baseline"), Some("/elsewhere/lint.json"));
    }

    #[test]
    fn a_flag_origin_names_the_flag_rather_than_a_file() {
        let mut settings = Settings::with_defaults();
        settings.set("format.indent", Value::Integer(8), Origin::flag("--indent"));
        assert_eq!(
            settings.origin("format.indent").expect("set").describe(),
            "--indent"
        );
    }

    // --- FR-008/FR-009: `format.indent-table` / `format.width-profiles` ---

    #[test]
    fn a_well_formed_indent_table_entry_is_accepted() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "format.indent-table",
            &Value::Array(vec![Value::String("my-macro=definition".to_owned())]),
            &origin(),
            None,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            settings.list("format.indent-table"),
            vec!["my-macro=definition"]
        );
    }

    /// This is the FR-008 requirement that a style name outside the
    /// vocabulary is rejected cleanly *at config-resolution time* rather than
    /// deep inside the formatter: `Settings::apply` runs during
    /// configuration loading, well before any `Formatter` exists.
    #[test]
    fn an_unrecognised_style_name_in_an_indent_table_entry_is_rejected_at_config_resolution_time() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "format.indent-table",
            &Value::Array(vec![Value::String("my-macro=not-a-real-style".to_owned())]),
            &origin(),
            None,
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::NotAChoice);
        assert_eq!(diagnostics[0].severity(), Severity::Error);
        assert!(diagnostics[0].message.contains("not-a-real-style"));
        // Rejected, not merely warned about: the previous (unset) value
        // stands, matching `a_rejected_value_leaves_the_previous_one_in_place`.
        assert!(settings.list("format.indent-table").is_empty());
    }

    #[test]
    fn an_indent_table_entry_missing_its_equals_sign_is_rejected() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "format.indent-table",
            &Value::Array(vec![Value::String("my-macro".to_owned())]),
            &origin(),
            None,
        );
        assert_eq!(diagnostics[0].code, DiagnosticCode::WrongType);
        assert!(diagnostics[0].message.contains("symbol=style"));
    }

    #[test]
    fn a_well_formed_width_profile_entry_is_accepted() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "format.width-profiles",
            &Value::Array(vec![Value::String("definition=60".to_owned())]),
            &origin(),
            None,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            settings.list("format.width-profiles"),
            vec!["definition=60"]
        );
    }

    #[test]
    fn an_unrecognised_style_name_in_a_width_profile_entry_is_rejected_at_config_resolution_time() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "format.width-profiles",
            &Value::Array(vec![Value::String("not-a-real-style=60".to_owned())]),
            &origin(),
            None,
        );
        assert_eq!(diagnostics[0].code, DiagnosticCode::NotAChoice);
        assert!(settings.list("format.width-profiles").is_empty());
    }

    #[test]
    fn a_width_profile_entry_with_a_non_integer_width_is_rejected() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "format.width-profiles",
            &Value::Array(vec![Value::String("definition=wide".to_owned())]),
            &origin(),
            None,
        );
        assert_eq!(diagnostics[0].code, DiagnosticCode::WrongType);
    }

    #[test]
    fn a_width_profile_entry_outside_the_accepted_range_is_rejected() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "format.width-profiles",
            &Value::Array(vec![Value::String("definition=99999".to_owned())]),
            &origin(),
            None,
        );
        assert_eq!(diagnostics[0].code, DiagnosticCode::OutOfRange);
    }

    #[test]
    fn a_width_profile_entry_missing_its_equals_sign_is_rejected() {
        let mut settings = Settings::with_defaults();
        let diagnostics = settings.apply(
            "format.width-profiles",
            &Value::Array(vec![Value::String("definition".to_owned())]),
            &origin(),
            None,
        );
        assert_eq!(diagnostics[0].code, DiagnosticCode::WrongType);
        assert!(diagnostics[0].message.contains("style=width"));
    }

    #[test]
    fn a_quote_style_choice_is_accepted_and_an_unrecognised_one_is_rejected() {
        let mut settings = Settings::with_defaults();
        assert_eq!(settings.text("format.quote-style"), Some("shorthand"));

        let diagnostics = settings.apply(
            "format.quote-style",
            &Value::String("canonical".to_owned()),
            &origin(),
            None,
        );
        assert!(diagnostics.is_empty());
        assert_eq!(settings.text("format.quote-style"), Some("canonical"));

        let diagnostics = settings.apply(
            "format.quote-style",
            &Value::String("expanded".to_owned()),
            &origin(),
            None,
        );
        assert_eq!(diagnostics[0].code, DiagnosticCode::NotAChoice);
    }

    #[test]
    fn entries_are_listed_in_schema_order_including_unset_keys() {
        let settings = Settings::with_defaults();
        let entries = settings.entries();
        assert_eq!(entries.len(), schema::KEY_COUNT);
        assert_eq!(entries[0].0.key, "extends");
        assert!(
            entries
                .iter()
                .any(|(entry, resolved)| entry.key == "dialect.default" && resolved.is_none())
        );
    }
}
