//! Static semantic analysis: read-only side tables hung beside the syntax
//! tree.
//!
//! The tree is never rewritten. Formatting survives a refactor because edits
//! are byte-span replacements over untouched source, and that discipline only
//! holds while the tree stays authoritative — so everything the analyses learn
//! lives here, keyed by [`NodeKey`], rather than as annotations on the tree.
//!
//! The layers stack: bindings first, then values on top of bindings, then
//! types. Each is owned data linked to the one below by id, never by borrow,
//! so they can all sit in one lazily-built context without becoming a
//! self-referential struct.
//!
//! Two rules hold throughout. Facts are only recorded when they are provable —
//! anything uncertain is simply absent rather than guessed. And propagation
//! stops at forms whose semantics are not registered for the dialect, because
//! an unknown macro can do anything to what it encloses.

// The layer is deliberately wider than its current callers. Three lint rules
// read the value table today; the type context's consumers (`eq-number-
// comparison` and friends) and the project context's (the impact and
// undefined-package reports) still match names and spellings by hand and have
// not been converted. Deleting the API they will need — `NodeKey::of`,
// `Binding::name`, the project tables — would leave modules that cannot be
// wired up, and each is covered by its own tests.
#![allow(dead_code, unused_imports)]

mod node_key;

pub mod binding;
pub mod project;
pub mod typing;
pub mod value;

pub use node_key::NodeKey;
