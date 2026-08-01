//! Minimal `.editorconfig` discovery, folded into the four `format.*` keys
//! it maps onto: `indent_size` → `format.indent`, `max_line_length` →
//! `format.max-inline-width`, `insert_final_newline` →
//! `format.insert-final-newline`, `trim_trailing_whitespace` →
//! `format.trim-trailing-whitespace`.
//!
//! **Precedence.** `paredit.toml` always wins over a discovered
//! `.editorconfig` for any key both files set. [`crate::load::load`]
//! achieves this by construction rather than by comparison: it applies
//! whatever this module discovers *before* the `paredit.toml` layer loop, so
//! [`crate::settings::Settings::apply`]'s unconditional "last write wins"
//! (see its own doc comment) makes every later `paredit.toml` layer replace
//! an `.editorconfig`-sourced value the same way a `Directory` layer already
//! replaces a `Repository` one — no special-casing needed here.
//!
//! **This is not a `paredit.toml`-layer reuse.** `load.rs`'s own directory
//! discovery (`discover`, `directories_between`, `find_repository_root`) is
//! built around reading a *closed, whole-document* TOML file into one
//! `Settings` merge pass — it does not know about `.editorconfig`'s very
//! different rules (an INI-like, section-glob-scoped format, an early-stop
//! `root = true` directive, precedence keyed off file depth rather than
//! layer identity) and folding those into it would bend a single-purpose
//! loader around a second file format it was never asked to understand. This
//! module borrows only the two *values* that pass ([`crate::load::load`]'s
//! own `repository_root` and `start`) and walks independently.
//!
//! **Reduced glob support, by necessity, not laziness.** A property's
//! section restricts it to files matching a glob (`[*.rs]`, `[{a,b}]`, brace
//! expansion, `**`), but *which* file `edit format` will run against is not
//! known yet at configuration-load time: `paredit.toml`'s own values reach a
//! command by rewriting its argument vector before `clap` even parses
//! `--file` (see `config_bridge`'s module doc), so there is no target
//! filename to match a glob against this early. Rather than guess, this
//! module only honours a section whose glob is exactly `*` or `**` (or a
//! property written before any `[section]` at all, which the spec never
//! restricts to a glob in the first place) — the "applies to everything"
//! case every real-world `.editorconfig` this tool is likely to meet already
//! uses for a project-wide indent/width policy. A narrower section (`[*.py]`)
//! is skipped outright: silently guessing which files it means, and getting
//! it wrong, would be worse than not reading it.
//!
//! No dependency was added for this. `.editorconfig`'s subset in play here —
//! `key = value` lines, `[glob]` section headers, `;`/`#` comments — is
//! narrow enough to hand-parse safely for the four properties this module
//! actually reads, and a real `.ini`/`.editorconfig` crate would bring far
//! more of the format (nested sections, escaping rules, full brace-glob
//! matching) than this reduced pass ever uses.

use std::path::{Path, PathBuf};

use crate::toml::Value;

/// The file name this format always uses; unlike `paredit.toml` it has no
/// alternate spelling.
const FILE_NAME: &str = ".editorconfig";

/// One `format.*` value a discovered `.editorconfig` contributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredValue {
    pub key: &'static str,
    pub value: Value,
    pub path: PathBuf,
    /// 1-based line the property was written on, for the same `Origin`
    /// every other file-sourced value carries.
    pub line: usize,
}

/// Every `format.*` value discovered by ascending from `start` to (and
/// including) `repository_root`, in the order [`crate::settings::Settings`]
/// should apply them: shallowest file first, so a deeper file's property —
/// applied later — replaces a shallower file's, the same direction
/// `paredit.toml`'s own directory layers already use. Stops ascending past a
/// file that declares a top-of-file `root = true`, same as the real
/// EditorConfig spec, just bounded at `repository_root` instead of the
/// filesystem root — consistent with every other discovery pass this tool
/// runs.
#[must_use]
pub fn discover(repository_root: Option<&Path>, start: &Path) -> Vec<DiscoveredValue> {
    let mut discovered = Vec::new();
    for (path, document) in files_shallowest_first(repository_root, start) {
        for property in &document.properties {
            if let Some((key, value)) = map_property(&property.key, &property.value) {
                discovered.push(DiscoveredValue {
                    key,
                    value,
                    path: path.clone(),
                    line: property.line,
                });
            }
        }
    }
    discovered
}

/// One parsed property line: a lowercased key, its raw value text exactly as
/// written, and the 1-based line it came from.
struct Property {
    key: String,
    value: String,
    line: usize,
}

/// One `.editorconfig` file's parse: whether it declared `root = true`
/// (which bounds the ascent), and every property this module's reduced glob
/// matcher accepted.
#[derive(Default)]
struct Document {
    root: bool,
    properties: Vec<Property>,
}

/// The `.editorconfig` files between `start` and `repository_root`
/// (inclusive), shallowest first, each already parsed. Reads at most one
/// file per directory in the walk, so a directory with no `.editorconfig` is
/// silently skipped rather than an error — the overwhelming majority of
/// directories in any ascent have none.
fn files_shallowest_first(
    repository_root: Option<&Path>,
    start: &Path,
) -> Vec<(PathBuf, Document)> {
    let mut deepest_first = Vec::new();
    let mut current = Some(start);
    while let Some(directory) = current {
        let candidate = directory.join(FILE_NAME);
        if let Ok(text) = std::fs::read_to_string(&candidate) {
            let document = parse(&text);
            let stop = document.root;
            deepest_first.push((candidate, document));
            if stop {
                break;
            }
        }
        if repository_root.is_some_and(|root| root == directory) {
            break;
        }
        current = directory.parent();
    }
    deepest_first.reverse();
    deepest_first
}

/// Parses `text` as a reduced `.editorconfig` document: `key = value` lines,
/// `[glob]` section headers, and `;`/`#` full-line comments. Anything this
/// reduced grammar does not recognise (an inline comment, a quoted value, a
/// malformed line) is skipped rather than guessed at — a property this pass
/// misses is a property `paredit.toml` or the compiled-in default still
/// supplies, never a wrong value silently applied.
fn parse(text: &str) -> Document {
    let mut document = Document::default();
    let mut seen_section = false;
    let mut in_universal_section = true;

    for (index, raw_line) in text.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(glob) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            in_universal_section = matches!(glob.trim(), "*" | "**");
            seen_section = true;
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = raw_key.trim().to_ascii_lowercase();
        let value = raw_value.trim().to_owned();

        // `root` is only meaningful before the first section header — a
        // `root = true` written inside `[*]` is not what the spec means by
        // it, and real tools ignore it there too.
        if !seen_section && key == "root" {
            document.root = value.eq_ignore_ascii_case("true");
            continue;
        }
        if in_universal_section {
            document.properties.push(Property {
                key,
                value,
                line: line_number,
            });
        }
    }
    document
}

/// The `format.*` schema key and value one `.editorconfig` property maps
/// onto, or `None` when this module does not recognise the property, its
/// value does not parse as the shape the target key expects (`indent_size =
/// tab`, `max_line_length = off`), or it is one of the properties this pass
/// does not map at all. A value this function returns still passes through
/// [`crate::settings::Settings::apply`]'s own range/shape validation like
/// any other source, so an out-of-range `indent_size` is reported the same
/// way an out-of-range `paredit.toml` value would be — this function only
/// decides which key a property *means*, not whether its value is valid for
/// that key.
fn map_property(key: &str, value: &str) -> Option<(&'static str, Value)> {
    match key {
        "indent_size" => value
            .parse::<i64>()
            .ok()
            .map(|size| ("format.indent", Value::Integer(size))),
        "max_line_length" => value
            .parse::<i64>()
            .ok()
            .map(|width| ("format.max-inline-width", Value::Integer(width))),
        "insert_final_newline" => {
            parse_bool(value).map(|flag| ("format.insert-final-newline", Value::Boolean(flag)))
        }
        "trim_trailing_whitespace" => {
            parse_bool(value).map(|flag| ("format.trim-trailing-whitespace", Value::Boolean(flag)))
        }
        _ => None,
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "paredit-editorconfig-{name}-{}",
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

    #[test]
    fn no_editorconfig_anywhere_discovers_nothing() {
        let scratch = Scratch::new("none");
        let start = scratch.dir("work");
        assert!(discover(None, &start).is_empty());
    }

    #[test]
    fn a_universal_section_maps_all_four_properties() {
        let scratch = Scratch::new("universal");
        scratch.write(
            ".editorconfig",
            "root = true\n[*]\nindent_size = 4\nmax_line_length = 100\ninsert_final_newline = false\ntrim_trailing_whitespace = false\n",
        );
        let discovered = discover(Some(&scratch.0), &scratch.0);
        let find = |key: &str| discovered.iter().find(|value| value.key == key);
        assert_eq!(find("format.indent").unwrap().value, Value::Integer(4));
        assert_eq!(
            find("format.max-inline-width").unwrap().value,
            Value::Integer(100)
        );
        assert_eq!(
            find("format.insert-final-newline").unwrap().value,
            Value::Boolean(false)
        );
        assert_eq!(
            find("format.trim-trailing-whitespace").unwrap().value,
            Value::Boolean(false)
        );
    }

    #[test]
    fn a_top_of_file_property_with_no_section_is_also_universal() {
        let scratch = Scratch::new("top-level");
        scratch.write("root/.editorconfig", "root = true\nindent_size = 3\n");
        let root = scratch.dir("root");
        let discovered = discover(Some(&root), &root);
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].key, "format.indent");
        assert_eq!(discovered[0].value, Value::Integer(3));
    }

    #[test]
    fn a_narrower_glob_section_is_skipped() {
        let scratch = Scratch::new("narrow-glob");
        scratch.write(".editorconfig", "root = true\n[*.py]\nindent_size = 4\n");
        assert!(discover(Some(&scratch.0), &scratch.0).is_empty());
    }

    #[test]
    fn indent_size_tab_and_max_line_length_off_do_not_parse_as_integers() {
        let scratch = Scratch::new("non-numeric");
        scratch.write(
            ".editorconfig",
            "root = true\n[*]\nindent_size = tab\nmax_line_length = off\n",
        );
        assert!(discover(Some(&scratch.0), &scratch.0).is_empty());
    }

    #[test]
    fn root_true_stops_the_ascent() {
        let scratch = Scratch::new("root-stop");
        scratch.write(".editorconfig", "root = true\n[*]\nindent_size = 8\n");
        scratch.write(
            "nested/.editorconfig",
            "root = true\n[*]\nindent_size = 2\n",
        );
        let start = scratch.dir("nested");
        let discovered = discover(Some(&scratch.0), &start);
        // Only the nested file's value: its own `root = true` stopped the
        // ascent before the outer file was ever read.
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].value, Value::Integer(2));
    }

    #[test]
    fn a_deeper_file_overrides_a_shallower_one_for_the_same_key() {
        let scratch = Scratch::new("layering");
        scratch.write(".editorconfig", "root = true\n[*]\nindent_size = 8\n");
        scratch.write("nested/.editorconfig", "[*]\nindent_size = 2\n");
        let start = scratch.dir("nested");
        let discovered = discover(Some(&scratch.0), &start);
        // Application order is shallowest first, so the last entry for
        // `format.indent` — the one `Settings::apply` actually keeps — is
        // the deeper file's.
        let indent_values: Vec<&Value> = discovered
            .iter()
            .filter(|value| value.key == "format.indent")
            .map(|value| &value.value)
            .collect();
        assert_eq!(indent_values, vec![&Value::Integer(8), &Value::Integer(2)]);
    }

    #[test]
    fn discovery_is_bounded_at_the_repository_root() {
        let scratch = Scratch::new("bounded");
        // Outside the repository root entirely: must never be read, even
        // though it sits directly above it in the directory tree.
        scratch.write(".editorconfig", "root = true\n[*]\nindent_size = 16\n");
        let repository_root = scratch.dir("repo");
        let start = scratch.dir("repo/src");
        assert!(discover(Some(&repository_root), &start).is_empty());
    }

    #[test]
    fn a_line_comment_and_a_narrower_glob_do_not_confuse_the_parser() {
        let scratch = Scratch::new("comments");
        scratch.write(
            ".editorconfig",
            "; a comment\nroot = true\n# another comment\n[*]\nindent_size = 4\n[*.md]\nindent_size = 2\n",
        );
        let discovered = discover(Some(&scratch.0), &scratch.0);
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].value, Value::Integer(4));
    }
}
