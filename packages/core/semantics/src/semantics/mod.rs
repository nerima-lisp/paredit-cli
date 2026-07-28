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

// What remains unread falls into two groups, and neither is an oversight.
//
// The binding table's structural surface: the scope tree (`Scope::parent`,
// `is_within`, `scope_count`) and a binding's provenance (`definition`,
// `references`). The consumers wired up so far ask value and type questions,
// which need neither; a scope-aware rule or an unused-binding report would.
// `binder_head` left this list when the coverage harness began attributing
// uninitialized bindings to the form that bound them.
//
// The system-order resolver (`resolve_system_order`, `system_dependency_edges`,
// `SystemOrderCycle`). Cross-file constant resolution turned out not to need
// it: the project table carries a value only for a `defconstant` defined
// exactly once project-wide, and "exactly once" is the same however the files
// are visited. An analysis whose answer depends on which file was seen first
// would need it, and none does yet.
//
// Every item here is exercised by this module's own tests, and deleting them
// would mean re-deriving what the builders already have.
#![allow(dead_code, unused_imports)]

mod node_key;

pub mod binding;
pub mod project;
pub mod typing;
pub mod value;

pub use node_key::NodeKey;
