use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use super::Dialect;

/// The dialect name is not one this build knows.
///
/// Typed rather than `anyhow`, because `clap` renders it as a value-parser
/// failure and a caller that offers a suggestion needs the rejected text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unsupported dialect: {name}")]
pub struct UnsupportedDialect {
    pub name: String,
}

impl FromStr for Dialect {
    type Err = UnsupportedDialect;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" | "unknown" => Ok(Self::Unknown),
            "common-lisp" | "cl" => Ok(Self::CommonLisp),
            "emacs-lisp" | "elisp" | "el" => Ok(Self::EmacsLisp),
            "lfe" => Ok(Self::Lfe),
            "scheme" | "scm" => Ok(Self::Scheme),
            "racket" | "rkt" => Ok(Self::Racket),
            "clojure" | "clj" => Ok(Self::Clojure),
            "hy" => Ok(Self::Hy),
            "carp" => Ok(Self::Carp),
            "janet" => Ok(Self::Janet),
            "fennel" | "fnl" => Ok(Self::Fennel),
            _ => Err(UnsupportedDialect { name: s.to_owned() }),
        }
    }
}

impl fmt::Display for Dialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}
