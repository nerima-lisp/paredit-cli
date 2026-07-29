//! The two ways configuration goes wrong, kept apart on purpose.
//!
//! A [`ConfigError`] means the configuration could not be *read*: a file is
//! unreadable, its syntax is broken, its `extends` chain loops. Nothing useful
//! can be produced, so loading stops.
//!
//! A [`Diagnostic`] means the configuration was read and says something this
//! build does not understand — an unknown key, a value of the wrong type, a
//! lint rule that is not registered. Loading continues, because `config show`
//! still has an honest answer to give and because reporting *all* of a file's
//! problems at once is worth more to whoever is fixing it than reporting the
//! first. `config check` turns any error-severity diagnostic into a non-zero
//! exit; every other command reports them and carries on with what it could
//! understand.

use std::fmt;
use std::path::PathBuf;

use thiserror::Error;

use crate::toml::TomlError;

/// The configuration could not be read.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read configuration {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("configuration {path} is not valid UTF-8")]
    NotUtf8 { path: PathBuf },

    #[error("refusing to read configuration {path}: exceeds {limit} bytes")]
    TooLarge { path: PathBuf, limit: u64 },

    #[error("{path}: {source}")]
    Syntax {
        path: PathBuf,
        #[source]
        source: TomlError,
    },

    #[error("configuration {path} extends {missing}, which does not exist")]
    ExtendsMissing { path: PathBuf, missing: PathBuf },

    #[error("configuration extends itself: {}", render_chain(chain))]
    ExtendsCycle { chain: Vec<PathBuf> },

    #[error("configuration extends more than {limit} files deep")]
    ExtendsTooDeep { limit: usize },

    #[error("--config {path} does not exist")]
    ExplicitMissing { path: PathBuf },
}

fn render_chain(chain: &[PathBuf]) -> String {
    chain
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(" -> ")
}

/// How much a diagnostic matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The setting has no effect and almost certainly was meant to.
    Error,
    /// The setting works, but not the way it reads.
    Warning,
}

impl Severity {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// A stable identifier for a class of configuration problem.
///
/// Machine-readable on purpose: an agent that sees `unknown-key` can act on it
/// without matching on English prose that is free to be reworded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    UnknownKey,
    WrongType,
    NotAChoice,
    OutOfRange,
    UnknownRule,
    UnknownCategory,
    Conflict,
    EmptyValue,
}

impl DiagnosticCode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::UnknownKey => "unknown-key",
            Self::WrongType => "wrong-type",
            Self::NotAChoice => "not-a-choice",
            Self::OutOfRange => "out-of-range",
            Self::UnknownRule => "unknown-rule",
            Self::UnknownCategory => "unknown-category",
            Self::Conflict => "conflict",
            Self::EmptyValue => "empty-value",
        }
    }

    #[must_use]
    pub const fn severity(self) -> Severity {
        match self {
            Self::UnknownKey
            | Self::WrongType
            | Self::NotAChoice
            | Self::OutOfRange
            | Self::UnknownRule
            | Self::UnknownCategory
            | Self::Conflict => Severity::Error,
            Self::EmptyValue => Severity::Warning,
        }
    }
}

/// One thing wrong with a configuration that was nonetheless readable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    /// The file it was found in, or `None` for an environment override.
    pub path: Option<PathBuf>,
    /// 1-based line, or `None` for an environment override.
    pub line: Option<usize>,
    /// The key as written.
    pub key: String,
    pub message: String,
    /// What to do about it, when there is one obvious thing.
    pub suggestion: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.code.severity()
    }

    /// Where this was found, rendered the way an editor jumps to it.
    #[must_use]
    pub fn origin(&self) -> String {
        match (&self.path, self.line) {
            (Some(path), Some(line)) => format!("{}:{line}", path.display()),
            (Some(path), None) => path.display().to_string(),
            (None, _) => "environment".to_owned(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {} [{}] {}",
            self.origin(),
            self.severity().label(),
            self.code.label(),
            self.message
        )?;
        if let Some(suggestion) = &self.suggestion {
            write!(formatter, " ({suggestion})")?;
        }
        Ok(())
    }
}

/// The result type this package's fallible entry points return.
pub type ConfigResult<T> = std::result::Result<T, ConfigError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cycle_renders_the_whole_chain() {
        let error = ConfigError::ExtendsCycle {
            chain: vec![PathBuf::from("a.toml"), PathBuf::from("b.toml")],
        };
        assert_eq!(
            error.to_string(),
            "configuration extends itself: a.toml -> b.toml"
        );
    }

    #[test]
    fn a_diagnostic_renders_its_origin_first() {
        let diagnostic = Diagnostic {
            code: DiagnosticCode::UnknownKey,
            path: Some(PathBuf::from("paredit.toml")),
            line: Some(7),
            key: "lint.fail_on".to_owned(),
            message: "`lint.fail_on` is not a recognised key".to_owned(),
            suggestion: Some("did you mean `lint.fail-on`?".to_owned()),
        };
        assert_eq!(
            diagnostic.to_string(),
            "paredit.toml:7: error [unknown-key] `lint.fail_on` is not a recognised key \
             (did you mean `lint.fail-on`?)"
        );
    }

    #[test]
    fn an_environment_diagnostic_has_no_file_to_point_at() {
        let diagnostic = Diagnostic {
            code: DiagnosticCode::NotAChoice,
            path: None,
            line: None,
            key: "lint.fail-on".to_owned(),
            message: "boom".to_owned(),
            suggestion: None,
        };
        assert_eq!(diagnostic.origin(), "environment");
    }

    #[test]
    fn only_an_empty_value_is_a_warning() {
        for code in [
            DiagnosticCode::UnknownKey,
            DiagnosticCode::WrongType,
            DiagnosticCode::NotAChoice,
            DiagnosticCode::OutOfRange,
            DiagnosticCode::UnknownRule,
            DiagnosticCode::UnknownCategory,
            DiagnosticCode::Conflict,
        ] {
            assert_eq!(code.severity(), Severity::Error, "{}", code.label());
        }
        assert_eq!(DiagnosticCode::EmptyValue.severity(), Severity::Warning);
    }
}
