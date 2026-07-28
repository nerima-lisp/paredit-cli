//! Reading a Scheme formals list into the names it binds.
//!
//! R7RS 4.1.4 gives `lambda` three formals shapes, and all three appear in
//! ordinary code:
//!
//! ```scheme
//! (lambda (a b) ...)        ; fixed arity
//! (lambda (a b . rest) ...) ; at least two, rest collected
//! (lambda args ...)         ; every argument collected
//! ```
//!
//! Racket adds optionals with defaults and keyword arguments on top. Treating
//! any of these as a flat list of symbols would either invent a binding for
//! the `.` token or miss the rest parameter entirely.

use crate::sexpr::reader::atom_symbol_span;
use crate::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView};

use super::symbol::scheme_identifier_text;

/// How a formal parameter receives its argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemeFormalKind {
    /// An ordinary positional parameter.
    Required,
    /// A rest parameter: `(a . rest)` or the bare-symbol formals shape.
    Rest,
    /// Racket's `[name default]` optional parameter.
    Optional,
    /// Racket's `#:name` keyword parameter.
    Keyword,
}

/// One name bound by a formals list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemeFormal {
    /// The parameter name as written.
    pub name: String,
    /// The span of the name atom itself, excluding any reader prefix.
    pub span: ByteSpan,
    /// How the parameter receives its argument.
    pub kind: SchemeFormalKind,
}

/// Reads every name a formals node binds, in source order.
///
/// Returns an empty vector for a node that binds nothing, which is also what a
/// malformed formals list yields: this layer reports what it can prove, and a
/// shape it cannot read proves nothing.
#[must_use]
pub fn scheme_formals(formals: &ExpressionView) -> Vec<SchemeFormal> {
    // `(lambda args body)`: the whole argument list arrives as one binding.
    if formals.kind == ExpressionKind::Atom {
        return formal(formals, SchemeFormalKind::Rest)
            .into_iter()
            .collect();
    }

    if formals.kind != ExpressionKind::List {
        return Vec::new();
    }

    scheme_formals_in(&formals.children)
}

/// Reads formals from a slice of parameter nodes.
///
/// Needed because a definition's formals list carries its own name in child 0:
/// `(define (f a . rest) ...)` has parameters `children[1..]`, not `children`.
/// Passing the slice keeps that offset the caller's business.
#[must_use]
pub fn scheme_formals_in(parameters: &[ExpressionView]) -> Vec<SchemeFormal> {
    let mut names = Vec::new();
    let mut after_dot = false;
    let mut pending_keyword = false;

    for child in parameters {
        // The `.` of an improper formals list is punctuation, not a parameter.
        // Everything after it is the rest parameter.
        if is_dot(child) {
            after_dot = true;
            continue;
        }

        // Racket writes keyword parameters as `#:key name`, so the keyword
        // token names the *call site* and the binding is the token after it.
        if is_racket_keyword(child) {
            pending_keyword = true;
            continue;
        }

        let kind = if after_dot {
            SchemeFormalKind::Rest
        } else if pending_keyword {
            SchemeFormalKind::Keyword
        } else {
            SchemeFormalKind::Required
        };
        pending_keyword = false;

        match child.kind {
            ExpressionKind::Atom => names.extend(formal(child, kind)),
            // `[name default]` and `(name default)`: only the head binds. The
            // default is an expression evaluated in the enclosing scope, so it
            // is emphatically not a parameter name.
            ExpressionKind::List => {
                let optional_kind = if kind == SchemeFormalKind::Keyword {
                    SchemeFormalKind::Keyword
                } else {
                    SchemeFormalKind::Optional
                };
                names.extend(
                    child
                        .children
                        .first()
                        .and_then(|name| formal(name, optional_kind)),
                );
            }
            _ => {}
        }
    }

    names
}

/// The default-value expressions a formals list evaluates.
///
/// These are ordinary expressions in the *enclosing* scope, so a reference
/// query has to walk them even though they sit inside the binder.
#[must_use]
pub fn scheme_formal_defaults(formals: &ExpressionView) -> Vec<&ExpressionView> {
    if formals.kind != ExpressionKind::List {
        return Vec::new();
    }

    scheme_formal_defaults_in(&formals.children)
}

/// As [`scheme_formal_defaults`], for a slice of parameter nodes.
#[must_use]
pub fn scheme_formal_defaults_in(parameters: &[ExpressionView]) -> Vec<&ExpressionView> {
    parameters
        .iter()
        .filter(|child| child.kind == ExpressionKind::List)
        .filter_map(|child| child.children.get(1))
        .collect()
}

/// Whether a formals node is one this layer can read completely.
///
/// A caller that needs to prove "these are *all* the names" -- an unused
/// parameter report, say -- must check this first; `scheme_formals` alone
/// cannot distinguish "binds nothing" from "unreadable".
#[must_use]
pub fn scheme_formals_are_readable(formals: &ExpressionView) -> bool {
    match formals.kind {
        ExpressionKind::Atom => formals.reader_prefixes.is_empty(),
        ExpressionKind::List => {
            matches!(formals.delimiter, Some(Delimiter::Paren | Delimiter::Bracket))
                && formals.reader_prefixes.is_empty()
                && formals.children.iter().all(|child| match child.kind {
                    ExpressionKind::Atom => true,
                    ExpressionKind::List => child
                        .children
                        .first()
                        .is_some_and(|name| name.kind == ExpressionKind::Atom),
                    _ => false,
                })
        }
        _ => false,
    }
}

fn formal(view: &ExpressionView, kind: SchemeFormalKind) -> Option<SchemeFormal> {
    // `is_scheme_identifier` already rejects `#:mode`, string and numeric
    // literals, and the bare `.`; a parameter position holding any of those is
    // a shape this layer declines to read rather than a binding.
    let name = scheme_identifier_text(view)?;
    Some(SchemeFormal {
        name: name.to_owned(),
        span: atom_symbol_span(view)?,
        kind,
    })
}

fn is_dot(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && view.reader_prefixes.is_empty()
        && view.text.as_deref().is_some_and(is_dot_text)
}

const fn is_dot_text(text: &str) -> bool {
    matches!(text.as_bytes(), b".")
}

fn is_racket_keyword(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && view
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with("#:") && text.len() > 2)
}
