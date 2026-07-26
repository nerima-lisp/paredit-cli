//! The project context: symbol identity that survives crossing a file.
//!
//! Everything below this layer is per-file. That is enough for a lint, but not
//! for a question like "who calls this function?", which today is answered by
//! comparing bare names and therefore conflates `app:run` with `test:run`.
//!
//! Resolving `defpackage`/`in-package`/`use-package` gives each name a home
//! package, which makes those two different values. On top of that sits a
//! table of top-level definitions — and, for `defconstant` alone, their
//! values, since a constant is the only definition whose value provably cannot
//! differ between the point of definition and the point of use.

pub mod model;
pub mod service;

pub use model::{GlobalTable, PackageId, QualifiedSymbol};
