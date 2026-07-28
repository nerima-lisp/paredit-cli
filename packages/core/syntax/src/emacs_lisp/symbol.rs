//! Naming conventions that stand in for Emacs Lisp's missing package system.
//!
//! Every symbol a package defines lands in one global obarray, so packages
//! avoid collisions by prefixing: `magit-status`, `org-agenda`. That
//! convention is what `package-lint` enforces and what a rename or an
//! unused-definition report has to understand, because `foo--helper` being
//! private is a fact about its *name* and nothing else.

/// Prefixes Emacs itself owns.
///
/// A package defining `emacs-something` or `custom-something` is colliding
/// with the core, which is the check `package-lint` calls
/// `definition-should-have-prefix` in its reserved-name form.
const RESERVED_PREFIXES: [&str; 6] = ["emacs-", "custom-", "cl-", "seq-", "subr-", "package-"];

/// Whether `name` starts with `prefix` at a symbol-component boundary.
///
/// `magit-status` has the prefix `magit`, and so does `magit--status`; but
/// `magistrate` does not, which a plain `starts_with` would get wrong. The
/// boundary is a `-`, the only separator Emacs Lisp names use.
#[must_use]
pub fn emacs_lisp_symbol_has_prefix(name: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return false;
    }
    let prefix = prefix.strip_suffix('-').unwrap_or(prefix);
    name.strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('-'))
}

/// Whether the name marks itself private by the `--` convention.
///
/// `foo--helper` is internal to the `foo` package. The marker is a *double*
/// hyphen after the prefix, so `foo-helper` is public and `--foo` — which has
/// no prefix at all — is not a package-private name.
#[must_use]
pub fn is_emacs_lisp_internal_symbol_name(name: &str) -> bool {
    name.find("--").is_some_and(|index| index > 0)
}

/// Whether the name follows the predicate convention.
///
/// Emacs Lisp uses both endings and they are not interchangeable: `-p` after
/// a multi-word name (`buffer-live-p`), a bare `p` after a single word
/// (`stringp`). Reporting the wrong one is the `-p`/`p` rule that Common Lisp
/// style guides state and Emacs Lisp inherited.
#[must_use]
pub fn is_emacs_lisp_predicate_name(name: &str) -> bool {
    name.ends_with("-p") || (!name.contains('-') && name.ends_with('p') && name.len() > 1)
}

/// Whether `name` claims a prefix reserved by Emacs or a bundled library.
#[must_use]
pub fn is_emacs_lisp_reserved_prefix(name: &str) -> bool {
    RESERVED_PREFIXES
        .iter()
        .any(|reserved| name.starts_with(reserved))
}
