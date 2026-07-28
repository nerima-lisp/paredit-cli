//! `;;;###autoload` magic comments.
//!
//! The cookie is not syntax — it is a comment that `loaddefs.el` generation
//! scans for, and it therefore has rules the reader never enforces. It must
//! begin its own line, it must be followed by a top-level form (or carry one
//! on the same line), and a cookie that satisfies neither is silently
//! ignored by Emacs rather than reported. Silence is exactly what makes it
//! worth checking here.

use crate::sexpr::{ByteSpan, SyntaxTree};

/// The literal cookie the standard `loaddefs` generator looks for.
const STANDARD_COOKIE: &str = ";;;###autoload";

/// What a cookie comment carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmacsLispAutoloadPayload {
    /// `;;;###autoload` alone: the form that follows is autoloaded.
    NextForm,
    /// `;;;###autoload (autoload 'f "lib")`: the rest of the line is copied
    /// into the generated file verbatim, and the following form is *not*
    /// autoloaded.
    InlineForm,
}

/// One `;;;###autoload` cookie found in a file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmacsLispAutoloadCookie {
    span: ByteSpan,
    prefix: String,
    payload: EmacsLispAutoloadPayload,
}

impl EmacsLispAutoloadCookie {
    /// The span of the whole cookie comment.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The package prefix of a non-standard cookie.
    ///
    /// Empty for the standard `;;;###autoload`. A package may generate its
    /// own loaddefs under a different cookie — `;;;###org-autoload` — and
    /// those are collected too, because a typo'd cookie looks exactly like a
    /// deliberate custom one and only the caller knows which it is.
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Whether this is the standard cookie.
    #[must_use]
    pub fn is_standard(&self) -> bool {
        self.prefix.is_empty()
    }

    #[must_use]
    pub const fn payload(&self) -> EmacsLispAutoloadPayload {
        self.payload
    }
}

/// Collects every autoload cookie in a parsed file, in source order.
///
/// Reading the parser's comment record rather than the raw text is what keeps
/// a `";;;###autoload"` inside a string literal from counting — a docstring
/// that documents the cookie is common enough in packages that generate their
/// own loaddefs.
#[must_use]
pub fn emacs_lisp_autoload_cookies(tree: &SyntaxTree) -> Vec<EmacsLispAutoloadCookie> {
    tree.comments()
        .filter(|comment| comment.own_line())
        .filter_map(|comment| {
            let (prefix, rest) = split_cookie(comment.text())?;
            Some(EmacsLispAutoloadCookie {
                span: comment.span(),
                prefix: prefix.to_owned(),
                payload: if rest.trim().is_empty() {
                    EmacsLispAutoloadPayload::NextForm
                } else {
                    EmacsLispAutoloadPayload::InlineForm
                },
            })
        })
        .collect()
}

/// Splits `;;;###PREFIXautoload REST` into its package prefix and remainder.
///
/// The generator's own pattern is `^;;;###\(.*\)autoload`, so the prefix is
/// whatever sits between the hashes and the word `autoload` — usually
/// nothing, sometimes `org-`.
fn split_cookie(text: &str) -> Option<(&str, &str)> {
    let body = text.trim_end().strip_prefix(";;;###")?;
    if let Some(rest) = body.strip_prefix("autoload") {
        // A cookie is `;;;###autoload` exactly, not `;;;###autoloading`: the
        // next character has to end the word.
        return rest
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
            .then_some(("", rest));
    }

    let cut = body.find("autoload")?;
    let rest = &body[cut + "autoload".len()..];
    rest.chars()
        .next()
        .is_none_or(char::is_whitespace)
        .then_some((&body[..cut], rest))
}

/// Whether `text` is the standard cookie, ignoring any payload after it.
#[must_use]
pub fn is_standard_emacs_lisp_autoload_cookie(text: &str) -> bool {
    split_cookie(text).is_some_and(|(prefix, _)| prefix.is_empty())
        || text.trim_end() == STANDARD_COOKIE
}
