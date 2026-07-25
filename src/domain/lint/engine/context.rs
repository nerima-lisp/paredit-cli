//! What every rule is handed besides the node it is looking at.

use std::path::Path;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, SyntaxTree};

/// The read-only surroundings of a check: which file, which dialect, the whole
/// tree, and the exact source bytes.
///
/// The source matters because a fix copies untouched sub-forms verbatim rather
/// than re-printing them, which is how formatting survives an autofix. The
/// whole tree matters for the handful of rules that correlate separate
/// definitions.
///
/// This is also the extension point for the semantic layers: the binding,
/// value, and type tables will hang off this struct, built lazily so a run
/// whose active rules need none of them pays for none of them.
#[derive(Debug, Clone, Copy)]
pub struct RuleContext<'a> {
    path: &'a Path,
    dialect: Dialect,
    tree: &'a SyntaxTree,
    source: &'a str,
}

impl<'a> RuleContext<'a> {
    pub fn new(path: &'a Path, dialect: Dialect, tree: &'a SyntaxTree, source: &'a str) -> Self {
        Self {
            path,
            dialect,
            tree,
            source,
        }
    }

    pub const fn path(&self) -> &'a Path {
        self.path
    }

    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }

    pub const fn tree(&self) -> &'a SyntaxTree {
        self.tree
    }

    pub const fn source(&self) -> &'a str {
        self.source
    }

    /// The exact source of `span`, or `""` if the span is not a character
    /// boundary of this document. Spans handed to a rule come from the tree, so
    /// the fallback is unreachable in practice and exists only to keep fix
    /// synthesis total.
    pub fn slice(&self, span: ByteSpan) -> &'a str {
        self.source
            .get(span.start().get()..span.end().get())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sexpr::ByteOffset;

    #[test]
    fn slices_the_exact_source_of_a_span() {
        let input = "(progn (or x))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let context = RuleContext::new(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
            input,
        );
        let span = ByteSpan::new(ByteOffset::new(7), ByteOffset::new(13));
        assert_eq!(context.slice(span), "(or x)");
    }

    #[test]
    fn an_out_of_range_span_slices_to_empty() {
        let input = "(a)";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let context =
            RuleContext::new(Path::new("test.lisp"), Dialect::CommonLisp, &tree, input);
        let span = ByteSpan::new(ByteOffset::new(0), ByteOffset::new(99));
        assert_eq!(context.slice(span), "");
    }
}
