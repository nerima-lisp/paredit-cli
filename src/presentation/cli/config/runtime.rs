//! Turning the merged configuration into the process-wide runtime settings.
//!
//! The composition root is the only place allowed to name both the
//! configuration schema and `paredit-core-cli`, so the translation lives here
//! rather than in either of them.

use std::path::PathBuf;

use paredit_core_cli::color::ColorMode;
use paredit_core_cli::runtime::{Language, RuntimeSettings, Verbosity};
use paredit_core_config::Settings;
use paredit_core_syntax::dialect::Dialect;

/// Reads the keys that act below the argument layer.
///
/// Keys that map onto a flag are *not* read here — the bridge injects those,
/// so reading them again would apply them twice and, worse, would apply them
/// even where the caller passed the flag themselves.
#[must_use]
pub fn resolve(settings: &Settings) -> RuntimeSettings {
    RuntimeSettings {
        dialect_default: settings
            .text("dialect.default")
            .and_then(dialect_from_label),
        dialect_force: settings.boolean("dialect.force").unwrap_or(false),
        exclude_paths: settings
            .list("paths.exclude")
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        include_hidden: settings.boolean("paths.include-hidden").unwrap_or(false),
        // The configuration key is positive and the runtime field is negated;
        // this is the one place the two spellings meet.
        no_gitignore: !settings.boolean("paths.respect-gitignore").unwrap_or(true),
        no_pareditignore: !settings
            .boolean("paths.respect-pareditignore")
            .unwrap_or(true),
        include_generated: settings.boolean("paths.include-generated").unwrap_or(false),
        max_depth: settings
            .integer("paths.max-depth")
            .and_then(|depth| usize::try_from(depth).ok()),
        verbosity: settings
            .text("output.verbosity")
            .and_then(Verbosity::from_label)
            .unwrap_or_default(),
        max_tokens: settings
            .integer("output.max-tokens")
            .and_then(|budget| usize::try_from(budget).ok())
            .unwrap_or(0),
        language: settings
            .text("output.language")
            .and_then(Language::from_label)
            .unwrap_or_default(),
        color: settings
            .text("output.color")
            .and_then(ColorMode::from_label)
            .unwrap_or_default(),
        // None of the rest is a configuration key. A `paredit.toml` that
        // quietly made every write a no-op would be a trap; progress on
        // stderr and delegating to a pager are both properties of one
        // invocation rather than of a repository; and the write-policy
        // settings below are specific enough — unix permission bits, an
        // opt-in symlink refusal, a source encoding — that a flag resolved
        // in `bootstrap` fits better than a schema key every other dialect
        // and platform would have to ignore.
        progress: false,
        paginate: false,
        dry_run: false,
        new_file_mode: None,
        refuse_symlinked_ancestors: false,
        source_encoding: None,
    }
}

/// The inverse of [`Dialect::label`].
///
/// Written as a match over `ALL` rather than a hand-listed table so a dialect
/// added to the syntax package resolves here without a second edit.
fn dialect_from_label(label: &str) -> Option<Dialect> {
    Dialect::ALL
        .into_iter()
        .find(|dialect| dialect.label() == label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_config::Origin;
    use paredit_core_config::settings::Layer;
    use paredit_core_config::toml::Value;

    fn with(pairs: &[(&'static str, Value)]) -> Settings {
        let mut settings = Settings::with_defaults();
        for (key, value) in pairs {
            settings.set(
                key,
                value.clone(),
                Origin::file(Layer::Repository, "paredit.toml", 1),
            );
        }
        settings
    }

    #[test]
    fn an_empty_configuration_resolves_to_the_built_in_defaults() {
        let runtime = resolve(&Settings::with_defaults());
        assert!(runtime.dialect_default.is_none());
        assert!(!runtime.include_hidden);
        assert!(runtime.max_depth.is_none());
        assert_eq!(runtime.verbosity, Verbosity::Normal);
        assert_eq!(runtime.language, Language::English);
    }

    #[test]
    fn every_dialect_label_resolves_back_to_its_dialect() {
        for dialect in Dialect::ALL {
            assert_eq!(dialect_from_label(dialect.label()), Some(dialect));
        }
        assert_eq!(dialect_from_label("perl"), None);
    }

    #[test]
    fn the_path_and_dialect_keys_reach_the_runtime() {
        let runtime = resolve(&with(&[
            ("dialect.default", Value::String("clojure".to_owned())),
            ("dialect.force", Value::Boolean(true)),
            (
                "paths.exclude",
                Value::Array(vec![Value::String("/repo/vendor".to_owned())]),
            ),
            ("paths.max-depth", Value::Integer(7)),
            ("paths.include-hidden", Value::Boolean(true)),
        ]));

        assert_eq!(runtime.dialect_default, Some(Dialect::Clojure));
        assert!(runtime.dialect_force);
        assert_eq!(runtime.exclude_paths, vec![PathBuf::from("/repo/vendor")]);
        assert_eq!(runtime.max_depth, Some(7));
        assert!(runtime.include_hidden);
    }

    /// A negative depth cannot reach here — the schema bounds it at 1 — but
    /// resolving must not panic if one ever did.
    #[test]
    fn a_depth_that_cannot_be_a_usize_is_dropped_rather_than_panicking() {
        let runtime = resolve(&with(&[("paths.max-depth", Value::Integer(-1))]));
        assert!(runtime.max_depth.is_none());
    }

    #[test]
    fn output_keys_reach_the_runtime() {
        let runtime = resolve(&with(&[
            ("output.verbosity", Value::String("detailed".to_owned())),
            ("output.max-tokens", Value::Integer(4096)),
            ("output.language", Value::String("ja".to_owned())),
            ("output.color", Value::String("never".to_owned())),
        ]));
        assert_eq!(runtime.verbosity, Verbosity::Detailed);
        assert_eq!(runtime.max_tokens, 4096);
        assert_eq!(runtime.language, Language::Japanese);
        assert_eq!(runtime.color, ColorMode::Never);
    }

    #[test]
    fn an_empty_configuration_defaults_color_to_auto() {
        assert_eq!(resolve(&Settings::with_defaults()).color, ColorMode::Auto);
    }
}
