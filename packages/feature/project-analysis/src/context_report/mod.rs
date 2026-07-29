//! One slice, one directory; the layers are names, not directories.
//!
//! The analysis itself is [`paredit_core_syntax::sexpr::SyntaxTree::context_at`],
//! so this slice is exposure and nothing else: there is no domain rule here
//! that the parser does not already own.

pub mod cli;
