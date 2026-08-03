//! What makes a Hy `except` clause a catch-everything.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionView};

use crate::shared::is_plain_list;

/// Hy only. `except` compiles to a Python `ExceptHandler`, and an empty
/// binding list is Python's bare `except:`.
pub const DIALECTS: [Dialect; 1] = [Dialect::Hy];

pub const HEAD_NAMES: [&str; 1] = ["except"];

/// Whether an `(except …)` clause names no exception type at all.
///
/// The clause's first child is its binding list. Hy accepts four shapes, and
/// only the first is a catch-everything:
///
/// ```text
/// (except []                   …)   bare: Python `except:`
/// (except [ValueError]         …)   one type, unbound
/// (except [e ValueError]       …)   bound name and type
/// (except [e [ValueError KeyError]] …)   bound name and a tuple of types
/// ```
///
/// Verified against Hy 1.3.1: `(except [] …)` catches a `KeyboardInterrupt`
/// and a `SystemExit`, while `(except [e Exception] …)` catches neither.
#[must_use]
pub fn is_catch_everything(clause: &ExpressionView) -> bool {
    clause.children.get(1).is_some_and(|bindings| {
        is_plain_list(bindings, Delimiter::Bracket) && bindings.children.is_empty()
    })
}
