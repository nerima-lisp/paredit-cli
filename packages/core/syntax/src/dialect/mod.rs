//! Dialect detection and capability helpers for Lisp-family files, including
//! extension-based routing and parser-affecting feature checks.

mod capability;
mod parse;

pub use parse::UnsupportedDialect;
mod semantic;

pub use semantic::{
    BinderShape, BindingVisibility, BodyShape, DefinitionShape, ExtractFunctionOperation,
    IntroduceLetOperation, NameListArity, ParameterShape, RelativeNodePath, RenameBindingOperation,
    ScopeShape, SemanticOperation, UnsupportedSemanticOperation, VerifiedSemanticPolicy,
};

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::sexpr::reader_policy_lang_directive;

/// Reads the `#lang` line a Racket source may open with.
///
/// Racket requires the directive to be the first non-whitespace, non-comment
/// text in the file, but a leading `;`-comment banner is common enough that
/// skipping those keeps ordinary files working.
fn lang_directive(source: &str) -> Option<&str> {
    for line in source.lines() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        return reader_policy_lang_directive(line);
    }
    None
}

/// Selects Lisp-family parsing and refactoring rules for a source file.
///
/// # Examples
///
/// ```
/// use paredit_core_syntax::dialect::Dialect;
///
/// assert_eq!(Dialect::from_extension("el"), Dialect::EmacsLisp);
/// assert_eq!(
///     Dialect::detect(Some(std::path::Path::new("init.el")), None),
///     Dialect::EmacsLisp
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    CommonLisp,
    EmacsLisp,
    Lfe,
    Scheme,
    Racket,
    Clojure,
    Hy,
    Carp,
    Janet,
    Fennel,
    Unknown,
}

impl Dialect {
    /// Resolves the effective dialect from an explicit override or file extension.
    pub fn detect(path: Option<&Path>, explicit: Option<Self>) -> Self {
        if let Some(dialect) = explicit {
            return dialect;
        }
        path.and_then(|path| path.extension().and_then(|extension| extension.to_str()))
            .map_or(Self::Unknown, Self::from_extension)
    }

    /// Resolves the effective dialect, consulting the source for a `#lang`
    /// line when the extension does not settle it.
    ///
    /// An explicit override always wins, and a recognised extension is trusted
    /// over the file's contents. The directive only breaks ties: a `.scm` or
    /// extensionless file opening with `#lang racket` is Racket, and reading
    /// it as R7RS Scheme would apply the wrong reader to `#:keyword` literals
    /// and the wrong rules to `struct`.
    #[must_use]
    pub fn detect_in_source(path: Option<&Path>, explicit: Option<Self>, source: &str) -> Self {
        let detected = Self::detect(path, explicit);
        if explicit.is_some() || !matches!(detected, Self::Unknown | Self::Scheme) {
            return detected;
        }
        if lang_directive(source).is_some() {
            return Self::Racket;
        }
        detected
    }

    /// The language a `#lang` directive names, if the source opens with one.
    ///
    /// Returns the bare language path -- `racket/base`, `typed/racket` -- so
    /// callers can report it without re-parsing the line.
    #[must_use]
    pub fn lang_directive(source: &str) -> Option<&str> {
        lang_directive(source)
    }

    /// Maps a lowercase file extension onto the closest supported dialect.
    #[must_use]
    pub fn from_extension(extension: &str) -> Self {
        match extension {
            "lisp" | "lsp" | "cl" | "asd" => Self::CommonLisp,
            "el" => Self::EmacsLisp,
            "lfe" => Self::Lfe,
            "scm" | "ss" | "sld" | "sls" | "sps" => Self::Scheme,
            "rkt" | "rktl" | "rktd" => Self::Racket,
            "clj" | "cljs" | "cljc" | "cljd" | "edn" | "bb" => Self::Clojure,
            "hy" => Self::Hy,
            "carp" => Self::Carp,
            "janet" => Self::Janet,
            "fnl" => Self::Fennel,
            _ => Self::Unknown,
        }
    }

    /// Returns the stable CLI and JSON identifier for this dialect.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CommonLisp => "common-lisp",
            Self::EmacsLisp => "emacs-lisp",
            Self::Lfe => "lfe",
            Self::Scheme => "scheme",
            Self::Racket => "racket",
            Self::Clojure => "clojure",
            Self::Hy => "hy",
            Self::Carp => "carp",
            Self::Janet => "janet",
            Self::Fennel => "fennel",
            Self::Unknown => "unknown",
        }
    }

    /// Returns a human-facing family label for diagnostics and reports.
    #[must_use]
    pub const fn family(self) -> &'static str {
        match self {
            Self::CommonLisp => "Common Lisp",
            Self::EmacsLisp => "Emacs Lisp",
            Self::Lfe => "Lisp Flavored Erlang",
            Self::Scheme => "Scheme",
            Self::Racket => "Racket",
            Self::Clojure => "Clojure",
            Self::Hy => "Hy",
            Self::Carp => "Carp",
            Self::Janet => "Janet",
            Self::Fennel => "Fennel",
            Self::Unknown => "generic s-expression",
        }
    }
}
