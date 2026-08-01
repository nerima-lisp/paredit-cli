//! The one table that declares what a `paredit.toml` may contain.
//!
//! Every recognised key appears here exactly once, with its type, its accepted
//! values, its default, its documentation, and — derived, not written — its
//! environment variable. Four consumers read this table:
//!
//! - validation (`paredit config check`),
//! - the effective-settings merge,
//! - `paredit config show`,
//! - `paredit config schema`.
//!
//! They are derived rather than maintained in parallel for the reason the lint
//! registry is: four hand-kept lists of the same thing will disagree, and the
//! disagreement is invisible until someone's configuration is silently ignored.

use paredit_core_syntax::dialect::Dialect;

/// What a key's value has to look like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Boolean,
    /// A whole number, inclusive of both bounds.
    Integer {
        min: i64,
        max: i64,
    },
    /// Any string. Used for paths, which this package never interprets.
    Text,
    /// A string from a closed set.
    Choice(&'static [&'static str]),
    /// An array of arbitrary strings.
    TextList,
    /// An array of lint rule names. Validated against the registry the caller
    /// supplies, because the registry is composition root and this is core.
    RuleList,
    /// An array of strings that may each name a rule *or* a rule category.
    RuleOrCategoryList,
    /// An array of `symbol=style` entries. The style half is checked against
    /// [`paredit_core_syntax::sexpr::STYLE_NAMES`]; the symbol half is never
    /// checked, because any string is a legitimate head to retarget.
    ///
    /// A table-shaped TOML value (`[format.indent-table] my-macro = "..."`)
    /// would read more naturally, but no `ValueKind` here represents a map —
    /// every other key is a scalar or a homogeneous array — and adding one
    /// disproportionate to this key's own scope, so this reuses the
    /// string-array shape every list key already has, the same way
    /// `RuleList`'s "one string per entry" already carries a mapping in
    /// disguise (a name to a rule struct). `check_list` gives each entry the
    /// same "did you get the shape right" checking a table's key/value pair
    /// would.
    IndentStyleEntries,
    /// An array of `style=width` entries. The style half is checked against
    /// [`paredit_core_syntax::sexpr::STYLE_NAMES`]; the width half must parse
    /// as an integer in the same range `format.max-inline-width` accepts.
    WidthProfileEntries,
}

impl ValueKind {
    /// The type name a diagnostic and `config schema` both use.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Integer { .. } => "integer",
            Self::Text => "string",
            Self::Choice(_) => "choice",
            Self::TextList => "string-array",
            Self::RuleList => "rule-array",
            Self::RuleOrCategoryList => "rule-or-category-array",
            Self::IndentStyleEntries => "indent-style-array",
            Self::WidthProfileEntries => "width-profile-array",
        }
    }

    /// Whether a value of this kind is written as a TOML array.
    #[must_use]
    pub const fn is_list(self) -> bool {
        matches!(
            self,
            Self::TextList
                | Self::RuleList
                | Self::RuleOrCategoryList
                | Self::IndentStyleEntries
                | Self::WidthProfileEntries
        )
    }

    /// The closed set of accepted strings, when there is one.
    ///
    /// For [`Self::IndentStyleEntries`] and [`Self::WidthProfileEntries`] this
    /// is not the set of accepted whole values — a whole value is a
    /// `symbol=style`/`style=width` entry, which is never itself one of
    /// [`paredit_core_syntax::sexpr::STYLE_NAMES`] — but the vocabulary valid
    /// on the style half of each entry. `--indent-table`'s and
    /// `--width-profile`'s own help text already tells a user to run
    /// `paredit config schema` for that vocabulary (see
    /// `packages/core/cli/src/args.rs`); before this, the command they were
    /// told to run answered with `"choices": null`, so the only way to learn
    /// the names was to get one wrong first and read the resulting
    /// `UnknownStyleName` error. `entry.kind.label()` still distinguishes
    /// these from a true [`Self::Choice`] (`"indent-style-array"` and
    /// `"width-profile-array"`, not `"choice"`), so a consumer reading
    /// `config schema`'s JSON is not told the whole value must equal one
    /// of these strings — only that each entry's style half must.
    #[must_use]
    pub const fn choices(self) -> Option<&'static [&'static str]> {
        match self {
            Self::Choice(values) => Some(values),
            Self::IndentStyleEntries | Self::WidthProfileEntries => {
                Some(paredit_core_syntax::sexpr::STYLE_NAMES)
            }
            _ => None,
        }
    }
}

/// A key's value when no layer sets it.
///
/// A separate enum from `toml::Value` because this one has to be constructible
/// in a `const`, which rules out an owned `String`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultValue {
    /// The key has no default; absent means absent.
    Unset,
    Boolean(bool),
    Integer(i64),
    Text(&'static str),
    EmptyList,
}

impl DefaultValue {
    /// Rendered as it would be written in a file, or `None` when unset.
    #[must_use]
    pub fn display(self) -> Option<String> {
        match self {
            Self::Unset => None,
            Self::Boolean(flag) => Some(flag.to_string()),
            Self::Integer(number) => Some(number.to_string()),
            Self::Text(text) => Some(format!("\"{text}\"")),
            Self::EmptyList => Some("[]".to_owned()),
        }
    }
}

/// One recognised configuration key.
#[derive(Debug, Clone, Copy)]
pub struct KeySchema {
    /// Full dotted name, as it is written after table expansion.
    pub key: &'static str,
    pub kind: ValueKind,
    pub default: DefaultValue,
    /// One line, present tense, describing what setting it does.
    pub summary: &'static str,
}

impl KeySchema {
    /// The environment variable that overrides this key.
    ///
    /// Derived from the key rather than declared, so a key can never gain a
    /// variable that does not match its name — `lint.fail-on` is always
    /// `PAREDIT_LINT_FAIL_ON` and there is nowhere to write anything else.
    #[must_use]
    pub fn env_var(&self) -> String {
        let mut name = String::from("PAREDIT_");
        for character in self.key.chars() {
            name.push(match character {
                '.' | '-' => '_',
                other => other.to_ascii_uppercase(),
            });
        }
        name
    }
}

/// Every dialect label, derived from the syntax package so a dialect added
/// there cannot go missing here.
pub const DIALECT_CHOICES: [&str; Dialect::ALL.len()] = {
    let mut labels = [""; Dialect::ALL.len()];
    let mut index = 0;
    while index < Dialect::ALL.len() {
        labels[index] = Dialect::ALL[index].label();
        index += 1;
    }
    labels
};

const OUTPUT_FORMATS: &[&str] = &["text", "json"];
const VERBOSITIES: &[&str] = &["quiet", "normal", "detailed"];
const LANGUAGES: &[&str] = &["en", "ja"];
const COLOR_MODES: &[&str] = &["auto", "always", "never"];
const LINT_PRESETS: &[&str] = &["minimal", "recommended", "pedantic", "all"];
const FAIL_SEVERITIES: &[&str] = &["never", "warning", "error"];
const QUOTE_STYLES: &[&str] = &["shorthand", "canonical"];
const NUMERIC_LITERAL_CASES: &[&str] = &["preserve", "lower", "upper"];

/// How many keys the schema declares. Pinned for the same reason
/// `RULE_COUNT` is: gaining or losing a configuration key should be a
/// reviewed change, not a diff nobody looked at.
pub const KEY_COUNT: usize = 40;

/// Every recognised key, in the order `config schema` and `config show`
/// present them. Grouped by table, tables in the order a file would write them.
pub const SCHEMA: [KeySchema; KEY_COUNT] = [
    KeySchema {
        key: "extends",
        kind: ValueKind::TextList,
        default: DefaultValue::EmptyList,
        summary: "Configuration files this one inherits from, resolved relative to it and applied \
                  left to right beneath it.",
    },
    // --- [dialect] ---
    KeySchema {
        key: "dialect.default",
        kind: ValueKind::Choice(&DIALECT_CHOICES),
        default: DefaultValue::Unset,
        summary: "Dialect to assume for files whose extension does not settle it.",
    },
    KeySchema {
        key: "dialect.force",
        kind: ValueKind::Boolean,
        default: DefaultValue::Boolean(false),
        summary: "Apply dialect.default even to files whose extension is recognised.",
    },
    // --- [paths] ---
    KeySchema {
        key: "paths.exclude",
        kind: ValueKind::TextList,
        default: DefaultValue::EmptyList,
        summary: "Paths to skip during workspace discovery, relative to the file that sets them.",
    },
    KeySchema {
        key: "paths.include-hidden",
        kind: ValueKind::Boolean,
        default: DefaultValue::Boolean(false),
        summary: "Descend into dot-directories during workspace discovery.",
    },
    KeySchema {
        key: "paths.include-generated",
        kind: ValueKind::Boolean,
        default: DefaultValue::Boolean(false),
        summary: "Include conventionally generated directories (target, vendor, dist, build).",
    },
    KeySchema {
        key: "paths.max-depth",
        kind: ValueKind::Integer { min: 1, max: 256 },
        default: DefaultValue::Unset,
        summary: "Deepest directory level workspace discovery descends to.",
    },
    KeySchema {
        key: "paths.respect-gitignore",
        kind: ValueKind::Boolean,
        default: DefaultValue::Boolean(true),
        summary: "Skip files that .gitignore or .git/info/exclude excludes.",
    },
    KeySchema {
        key: "paths.respect-pareditignore",
        kind: ValueKind::Boolean,
        default: DefaultValue::Boolean(true),
        summary: "Skip files that a .pareditignore excludes.",
    },
    // --- [cache] ---
    KeySchema {
        key: "cache.dir",
        kind: ValueKind::Text,
        default: DefaultValue::Unset,
        summary: "Directory that stores workspace discovery results between runs, relative to \
                  the file that sets it.",
    },
    // --- [format] ---
    KeySchema {
        key: "format.indent",
        kind: ValueKind::Integer { min: 0, max: 16 },
        default: DefaultValue::Integer(2),
        summary: "Spaces per nesting level for `edit format`.",
    },
    KeySchema {
        key: "format.max-inline-width",
        kind: ValueKind::Integer { min: 1, max: 512 },
        default: DefaultValue::Unset,
        summary: "Widest a form may be and still be printed on one line.",
    },
    KeySchema {
        key: "format.block-comment-reindent",
        kind: ValueKind::Boolean,
        default: DefaultValue::Boolean(false),
        summary: "Realign #|...|# block comment lines to their new nesting depth.",
    },
    KeySchema {
        key: "format.comment-column",
        kind: ValueKind::Integer { min: 0, max: 512 },
        default: DefaultValue::Unset,
        summary: "Column trailing comments align to; 0 auto-aligns to one past the widest \
                  adjacent form. Unset keeps a single space with no alignment.",
    },
    KeySchema {
        key: "format.blank-lines-max-consecutive",
        kind: ValueKind::Integer { min: 0, max: 16 },
        default: DefaultValue::Unset,
        summary: "Most consecutive blank lines to preserve between top-level forms. Unset \
                  always collapses every gap to exactly one.",
    },
    KeySchema {
        key: "format.indent-table",
        kind: ValueKind::IndentStyleEntries,
        default: DefaultValue::EmptyList,
        summary: "Per-symbol indent style overrides for `edit format`, each written as \
                  `symbol=style`.",
    },
    KeySchema {
        key: "format.width-profiles",
        kind: ValueKind::WidthProfileEntries,
        default: DefaultValue::EmptyList,
        summary: "Per-indent-style `--max-width` overrides for `edit format`, each written as \
                  `style=width`; only the `general` style ever renders on one line, so a \
                  non-general entry has no effect unless format.indent-table retargets a form \
                  onto general.",
    },
    KeySchema {
        key: "format.quote-style",
        kind: ValueKind::Choice(QUOTE_STYLES),
        default: DefaultValue::Text("shorthand"),
        summary: "How reader-macro prefixes like 'x print for `edit format`: shorthand or their \
                  canonical list form.",
    },
    KeySchema {
        key: "format.numeric-literal-case",
        kind: ValueKind::Choice(NUMERIC_LITERAL_CASES),
        default: DefaultValue::Text("preserve"),
        summary: "Letter case for a numeric literal's radix prefix (#x/#o/#b/#NNr) and float \
                  exponent marker, for `edit format`.",
    },
    KeySchema {
        key: "format.align-clause-values",
        kind: ValueKind::Boolean,
        default: DefaultValue::Boolean(false),
        summary: "Pad every let-style binding's value to one column past the widest name in its \
                  run, for `edit format`.",
    },
    KeySchema {
        key: "format.insert-final-newline",
        kind: ValueKind::Boolean,
        default: DefaultValue::Boolean(true),
        summary: "End the document `edit format` renders in exactly one trailing newline.",
    },
    KeySchema {
        key: "format.trim-trailing-whitespace",
        kind: ValueKind::Boolean,
        default: DefaultValue::Boolean(true),
        summary: "Trim a comment's own trailing whitespace in `edit format`'s output.",
    },
    // --- [lint] ---
    KeySchema {
        key: "lint.preset",
        kind: ValueKind::Choice(LINT_PRESETS),
        default: DefaultValue::Text("recommended"),
        summary: "How wide a net the rule selection casts.",
    },
    KeySchema {
        key: "lint.enable",
        kind: ValueKind::RuleList,
        default: DefaultValue::EmptyList,
        summary: "Run only these rules. Mutually exclusive with lint.disable.",
    },
    KeySchema {
        key: "lint.disable",
        kind: ValueKind::RuleList,
        default: DefaultValue::EmptyList,
        summary: "Skip these rules. Mutually exclusive with lint.enable.",
    },
    KeySchema {
        key: "lint.categories",
        kind: ValueKind::TextList,
        default: DefaultValue::EmptyList,
        summary: "Run only rules in these categories.",
    },
    KeySchema {
        key: "lint.tags",
        kind: ValueKind::TextList,
        default: DefaultValue::EmptyList,
        summary: "Run only rules carrying every one of these tags.",
    },
    KeySchema {
        key: "lint.deny",
        kind: ValueKind::RuleOrCategoryList,
        default: DefaultValue::EmptyList,
        summary: "Report these rules or categories at error severity.",
    },
    KeySchema {
        key: "lint.warn",
        kind: ValueKind::RuleOrCategoryList,
        default: DefaultValue::EmptyList,
        summary: "Report these rules or categories at warning severity.",
    },
    KeySchema {
        key: "lint.experimental",
        kind: ValueKind::Boolean,
        default: DefaultValue::Boolean(false),
        summary: "Also run the experimental rules, whichever preset is in force.",
    },
    KeySchema {
        key: "lint.fail-on",
        kind: ValueKind::Choice(FAIL_SEVERITIES),
        default: DefaultValue::Text("never"),
        summary: "Lowest finding severity that makes the command exit non-zero.",
    },
    KeySchema {
        key: "lint.baseline",
        kind: ValueKind::Text,
        default: DefaultValue::Unset,
        summary: "Baseline file of known findings to suppress, relative to the file that sets it.",
    },
    KeySchema {
        key: "lint.custom-rules",
        kind: ValueKind::Text,
        default: DefaultValue::Unset,
        summary: "Directory of project pattern rules, relative to the file that sets it.",
    },
    KeySchema {
        key: "lint.require-suppression-reason",
        kind: ValueKind::Boolean,
        default: DefaultValue::Boolean(false),
        summary: "Treat a `paredit:ignore` with no `-- reason` as an unused suppression.",
    },
    KeySchema {
        key: "lint.suppress-paths",
        kind: ValueKind::TextList,
        default: DefaultValue::EmptyList,
        summary: "Silence every lint finding under these paths, relative to the file that sets \
                  them, as generated code or vendored dependencies cannot carry an inline ignore.",
    },
    // --- [output] ---
    KeySchema {
        key: "output.format",
        kind: ValueKind::Choice(OUTPUT_FORMATS),
        default: DefaultValue::Unset,
        summary: "Default --output value for commands that accept one.",
    },
    KeySchema {
        key: "output.verbosity",
        kind: ValueKind::Choice(VERBOSITIES),
        default: DefaultValue::Text("normal"),
        summary: "How much detail a report includes.",
    },
    KeySchema {
        key: "output.max-tokens",
        kind: ValueKind::Integer {
            min: 0,
            max: 8_000_000,
        },
        default: DefaultValue::Integer(0),
        summary: "Approximate token budget a report is truncated to. 0 means no budget.",
    },
    KeySchema {
        key: "output.language",
        kind: ValueKind::Choice(LANGUAGES),
        default: DefaultValue::Text("en"),
        summary: "Language for diagnostics and gate messages. Report payloads stay in English.",
    },
    KeySchema {
        key: "output.color",
        kind: ValueKind::Choice(COLOR_MODES),
        default: DefaultValue::Text("auto"),
        summary: "Whether text output may use ANSI color: follow the terminal, force on, or force off.",
    },
];

/// The keys whose values name a path.
///
/// A relative path in a `paredit.toml` means "relative to this file", not "to
/// wherever the command was run", so these are resolved against the file that
/// set them at the moment they are applied. Listed here rather than as a sixth
/// column on 25 rows; the test below keeps the list honest.
pub const PATH_KEYS: &[&str] = &[
    "extends",
    "paths.exclude",
    "cache.dir",
    "lint.baseline",
    "lint.custom-rules",
    "lint.suppress-paths",
];

/// Looks a key up by its full dotted name.
#[must_use]
pub fn lookup(key: &str) -> Option<&'static KeySchema> {
    SCHEMA.iter().find(|entry| entry.key == key)
}

/// The declared keys whose name starts with `prefix` followed by a dot.
#[must_use]
pub fn keys_under(prefix: &str) -> Vec<&'static KeySchema> {
    let prefix = format!("{prefix}.");
    SCHEMA
        .iter()
        .filter(|entry| entry.key.starts_with(&prefix))
        .collect()
}

/// The declared key whose name is closest to `key`, for a "did you mean" hint.
///
/// Bounded by an edit distance of three so an unrelated key does not attract a
/// confident-sounding wrong suggestion.
#[must_use]
pub fn nearest_key(key: &str) -> Option<&'static str> {
    SCHEMA
        .iter()
        .map(|entry| (edit_distance(entry.key, key), entry.key))
        .filter(|(distance, _)| *distance <= 3)
        .min_by_key(|(distance, name)| (*distance, *name))
        .map(|(_, name)| name)
}

/// Levenshtein distance, two rows at a time.
fn edit_distance(left: &str, right: &str) -> usize {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn no_key_is_declared_twice() {
        let unique: BTreeSet<&str> = SCHEMA.iter().map(|entry| entry.key).collect();
        assert_eq!(unique.len(), KEY_COUNT);
    }

    /// The environment override is derived, so this is really checking that no
    /// two distinct keys collapse onto the same variable — `a.b-c` and `a-b.c`
    /// would, and only one of them could ever be set.
    #[test]
    fn no_two_keys_share_an_environment_variable() {
        let unique: BTreeSet<String> = SCHEMA.iter().map(KeySchema::env_var).collect();
        assert_eq!(unique.len(), KEY_COUNT);
    }

    #[test]
    fn an_environment_variable_is_the_key_shouted() {
        let entry = lookup("lint.fail-on").expect("declared");
        assert_eq!(entry.env_var(), "PAREDIT_LINT_FAIL_ON");
    }

    /// FR-012: `cache.dir` resolves to `PAREDIT_CACHE_DIR`, is a path key
    /// resolved relative to the file that sets it, and carries no default —
    /// unset means "no cache", exactly as an absent `--cache-dir` always has.
    #[test]
    fn cache_dir_is_a_path_key_with_no_default_and_the_expected_variable() {
        let entry = lookup("cache.dir").expect("declared");
        assert_eq!(entry.env_var(), "PAREDIT_CACHE_DIR");
        assert_eq!(entry.kind, ValueKind::Text);
        assert_eq!(entry.default, DefaultValue::Unset);
        assert!(PATH_KEYS.contains(&"cache.dir"));
    }

    /// A default has to be a value the key's own type would accept, or the
    /// merge would start from a state validation would reject.
    #[test]
    fn every_default_satisfies_its_own_kind() {
        for entry in &SCHEMA {
            let ok = match (entry.kind, entry.default) {
                (_, DefaultValue::Unset) => true,
                (ValueKind::Boolean, DefaultValue::Boolean(_))
                | (ValueKind::Integer { .. }, DefaultValue::Integer(_))
                | (ValueKind::Text, DefaultValue::Text(_)) => true,
                (ValueKind::Choice(values), DefaultValue::Text(text)) => values.contains(&text),
                (kind, DefaultValue::EmptyList) => kind.is_list(),
                _ => false,
            };
            assert!(ok, "{} has a default its kind rejects", entry.key);
        }
    }

    #[test]
    fn an_integer_default_is_inside_its_own_bounds() {
        for entry in &SCHEMA {
            if let (ValueKind::Integer { min, max }, DefaultValue::Integer(value)) =
                (entry.kind, entry.default)
            {
                assert!(
                    (min..=max).contains(&value),
                    "{} defaults to {value}, outside {min}..={max}",
                    entry.key
                );
            }
        }
    }

    #[test]
    fn every_summary_is_a_sentence() {
        for entry in &SCHEMA {
            assert!(
                entry.summary.ends_with('.') && entry.summary.starts_with(char::is_uppercase),
                "{}'s summary should be a sentence",
                entry.key
            );
        }
    }

    #[test]
    fn dialect_choices_track_the_syntax_package() {
        assert!(DIALECT_CHOICES.contains(&"common-lisp"));
        assert_eq!(DIALECT_CHOICES.len(), Dialect::ALL.len());
    }

    #[test]
    fn a_typo_finds_its_key_and_an_unrelated_word_does_not() {
        assert_eq!(nearest_key("lint.fail_on"), Some("lint.fail-on"));
        assert_eq!(nearest_key("format.indnt"), Some("format.indent"));
        assert_eq!(nearest_key("completely-unrelated-nonsense"), None);
    }

    /// A path key that is not declared, or is declared with a type that
    /// cannot hold a path, would silently skip relative-path resolution.
    #[test]
    fn every_path_key_is_declared_and_holds_strings() {
        for key in PATH_KEYS {
            let entry = lookup(key).unwrap_or_else(|| panic!("{key} is not in the schema"));
            assert!(
                matches!(entry.kind, ValueKind::Text) || entry.kind.is_list(),
                "{key} is a path key but holds {}",
                entry.kind.label()
            );
        }
    }

    #[test]
    fn keys_under_a_table_are_exactly_that_table() {
        let keys: Vec<&str> = keys_under("dialect")
            .into_iter()
            .map(|entry| entry.key)
            .collect();
        assert_eq!(keys, vec!["dialect.default", "dialect.force"]);
    }
}
