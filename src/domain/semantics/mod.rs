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

// What remains unread is the binding table's structural surface — the scope
// tree (`Scope::parent`, `is_within`) and a binding's provenance
// (`definition`, `binder_head`, `references`). The consumers wired up so far
// ask value and type questions, which need neither; a scope-aware rule or an
// unused-binding report would. Each is exercised by this module's own tests,
// and deleting them would mean re-deriving the scope tree the builder already
// has.
#![allow(dead_code, unused_imports)]

mod node_key;

pub mod binding;
pub mod project;
pub mod typing;
pub mod value;

pub use node_key::NodeKey;
