//! Process-wide settings resolved once at startup.
//!
//! Most of what `paredit.toml` configures reaches a command as an injected
//! flag (see the composition root's `config_bridge`), because a flag is a
//! thing a command already understands and a thing `--help` already documents.
//! Four settings cannot travel that way: they act *below* the argument layer,
//! inside dialect detection and workspace discovery, which no flag reaches on
//! most commands.
//!
//! Those live here. It is deliberately a plain struct with no dependency on
//! the configuration package: `paredit-core-cli` is handed the resolved
//! answers, and the composition root — the only place that may name both the
//! configuration schema and every feature — does the resolving.
//!
//! Installed exactly once, before any command runs, and read-only afterwards.
//! Global state, which is what a process-wide setting is; pretending otherwise
//! by threading it through 275 call sites would not make it less global.

use std::path::PathBuf;
use std::sync::OnceLock;

use paredit_core_syntax::dialect::Dialect;

/// How much detail a report includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verbosity {
    /// Counts, gates, and nothing a consumer can derive from them.
    Quiet,
    #[default]
    Normal,
    /// Everything the report can say, including per-item context.
    Detailed,
}

impl Verbosity {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Quiet => "quiet",
            Self::Normal => "normal",
            Self::Detailed => "detailed",
        }
    }

    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "quiet" => Some(Self::Quiet),
            "normal" => Some(Self::Normal),
            "detailed" => Some(Self::Detailed),
            _ => None,
        }
    }
}

/// The language diagnostics and gate messages are written in.
///
/// Report *payloads* stay in English whatever this says. A finding's `kind`
/// and a rule's name are identifiers that automation matches on; translating
/// them would break every consumer to help no one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    Japanese,
}

impl Language {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Japanese => "ja",
        }
    }

    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "en" => Some(Self::English),
            "ja" => Some(Self::Japanese),
            _ => None,
        }
    }
}

/// The settings that act below the argument layer.
#[derive(Debug, Clone, Default)]
pub struct RuntimeSettings {
    /// Dialect to fall back on when detection cannot decide.
    pub dialect_default: Option<Dialect>,
    /// Apply `dialect_default` even when the extension is recognised.
    pub dialect_force: bool,
    /// Paths workspace discovery skips.
    pub exclude_paths: Vec<PathBuf>,
    pub include_hidden: bool,
    pub include_generated: bool,
    pub max_depth: Option<usize>,
    /// Do not read `.gitignore` / `.git/info/exclude`.
    ///
    /// Stored negated so that `Default` — which every unit test and every
    /// library consumer sees — keeps reading them, matching the workspace
    /// package's own default. A positively-named field would default to
    /// `false` and silently turn ignore handling off everywhere.
    pub no_gitignore: bool,
    /// Do not read `.pareditignore`. Negated for the same reason.
    pub no_pareditignore: bool,
    pub verbosity: Verbosity,
    /// Approximate token budget for a report. Zero means no budget.
    pub max_tokens: usize,
    pub language: Language,
    /// Emit JSON Lines progress on stderr.
    pub progress: bool,
    /// Nothing may be written to disk.
    ///
    /// Resolved before argument parsing rather than per command, because its
    /// value is "no command in this process writes" — a property of the run,
    /// not of one invocation's arguments.
    pub dry_run: bool,
    /// Permission bits (e.g. `0o600`) a brand-new file is created with.
    ///
    /// `None` keeps the built-in `0o600`. Global rather than per-command for
    /// the same reason as `dry_run`: every one of the ~90 writing commands
    /// creates a new file through the same staging function in `io.rs`, and a
    /// caller who wants group-readable output (a shared checkout, a CI
    /// artifact directory) wants it for all of them, not one at a time.
    pub new_file_mode: Option<u32>,
    /// Refuse a write whose target's parent path climbs through a symlinked
    /// directory above the immediate parent (which is refused unconditionally
    /// already, `O_NOFOLLOW` catching only the last path component).
    ///
    /// Default off: `/tmp` is a symlink to `/private/tmp` on macOS, and
    /// similar stable, root-owned redirections are common enough elsewhere
    /// that refusing them unconditionally would break routine use. This is
    /// for a caller who wants the stricter policy — CI writing into a
    /// less-trusted checkout, for instance — not a universal default.
    pub refuse_symlinked_ancestors: bool,
}

static RUNTIME: OnceLock<RuntimeSettings> = OnceLock::new();

/// Installs the resolved settings. The first call wins.
///
/// Later calls are ignored rather than rejected: a second install would mean
/// two answers to a process-wide question, and the first one has already been
/// read by whatever ran in between.
pub fn install(settings: RuntimeSettings) {
    let _ = RUNTIME.set(settings);
}

/// The settings in force. Built-in defaults when nothing was installed, which
/// is the state every unit test and every library consumer sees.
#[must_use]
pub fn current() -> &'static RuntimeSettings {
    RUNTIME.get_or_init(RuntimeSettings::default)
}

impl RuntimeSettings {
    /// The ignore policy, narrowing `environment` by what the configuration
    /// said.
    ///
    /// Both must permit an ignore file for it to be read: the configuration is
    /// the repository's baseline and the `PAREDIT_NO_*_IGNORE` variables are
    /// the per-run escape hatch, so the environment can only ever narrow.
    #[must_use]
    pub const fn ignore_options(
        &self,
        environment: paredit_core_workspace::workspace::IgnoreOptions,
    ) -> paredit_core_workspace::workspace::IgnoreOptions {
        paredit_core_workspace::workspace::IgnoreOptions {
            respect_gitignore: environment.respect_gitignore && !self.no_gitignore,
            respect_pareditignore: environment.respect_pareditignore && !self.no_pareditignore,
        }
    }

    /// The dialect to use when an explicit `--dialect` was not given.
    ///
    /// `force` short-circuits detection outright. Without it the configured
    /// dialect is a *fallback*: a `.el` file is still Emacs Lisp, and only a
    /// file detection could not place picks up the configured default. That
    /// asymmetry is the point — "what my extensionless scripts are" and "read
    /// everything as this" are different requests.
    #[must_use]
    pub fn resolve_dialect(&self, detected: Dialect) -> Dialect {
        match self.dialect_default {
            Some(configured) if self.dialect_force => configured,
            Some(configured) if detected == Dialect::Unknown => configured,
            _ => detected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_installed_means_built_in_defaults() {
        let settings = RuntimeSettings::default();
        assert_eq!(settings.verbosity, Verbosity::Normal);
        assert_eq!(settings.language, Language::English);
        assert_eq!(settings.max_tokens, 0);
        assert!(settings.dialect_default.is_none());
    }

    #[test]
    fn without_a_configured_dialect_detection_stands() {
        let settings = RuntimeSettings::default();
        assert_eq!(
            settings.resolve_dialect(Dialect::EmacsLisp),
            Dialect::EmacsLisp
        );
        assert_eq!(settings.resolve_dialect(Dialect::Unknown), Dialect::Unknown);
    }

    #[test]
    fn a_configured_default_only_fills_in_for_an_undetected_file() {
        let settings = RuntimeSettings {
            dialect_default: Some(Dialect::CommonLisp),
            ..RuntimeSettings::default()
        };
        assert_eq!(
            settings.resolve_dialect(Dialect::EmacsLisp),
            Dialect::EmacsLisp
        );
        assert_eq!(
            settings.resolve_dialect(Dialect::Unknown),
            Dialect::CommonLisp
        );
    }

    #[test]
    fn forcing_overrides_even_a_recognised_extension() {
        let settings = RuntimeSettings {
            dialect_default: Some(Dialect::CommonLisp),
            dialect_force: true,
            ..RuntimeSettings::default()
        };
        assert_eq!(
            settings.resolve_dialect(Dialect::EmacsLisp),
            Dialect::CommonLisp
        );
    }

    /// Forcing with nothing to force to is a configuration that says nothing,
    /// and must not silently become "everything is Unknown".
    #[test]
    fn forcing_without_a_default_leaves_detection_alone() {
        let settings = RuntimeSettings {
            dialect_force: true,
            ..RuntimeSettings::default()
        };
        assert_eq!(settings.resolve_dialect(Dialect::Racket), Dialect::Racket);
    }

    #[test]
    fn labels_round_trip() {
        for verbosity in [Verbosity::Quiet, Verbosity::Normal, Verbosity::Detailed] {
            assert_eq!(Verbosity::from_label(verbosity.label()), Some(verbosity));
        }
        for language in [Language::English, Language::Japanese] {
            assert_eq!(Language::from_label(language.label()), Some(language));
        }
        assert_eq!(Verbosity::from_label("loud"), None);
        assert_eq!(Language::from_label("fr"), None);
    }
}
