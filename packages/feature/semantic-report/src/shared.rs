//! The three semantic tables, built once per file.
//!
//! Every report in this package needs some prefix of the stack — `constants`
//! stops at values, `types` and `narrowing` go all the way up — and building
//! them is the expensive part. One context type means a report body is its own
//! projection of the tables and nothing else.

use std::path::{Path, PathBuf};

use paredit_core_semantics::semantics::binding::{BindingTable, build_binding_table};
use paredit_core_semantics::semantics::typing::TypeTable;
use paredit_core_semantics::semantics::typing::service::build_type_table;
use paredit_core_semantics::semantics::value::{ValueTable, build_value_table};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};

/// One file, parsed, with every semantic table the package can build.
#[derive(Debug)]
pub struct SemanticFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub tree: SyntaxTree,
    pub bindings: BindingTable,
    pub values: ValueTable,
    pub types: TypeTable,
}

impl SemanticFile {
    /// Builds the whole stack. The layers are ordered because each reads the
    /// one below: values resolve references through the binding table, and
    /// types read a literal's value straight off the value table.
    #[must_use]
    pub fn analyze(path: &Path, dialect: Dialect, tree: SyntaxTree) -> Self {
        let bindings = build_binding_table(dialect, &tree, tree.source());
        let values = build_value_table(dialect, &tree, &bindings);
        let types = build_type_table(dialect, &tree, &bindings, &values);
        Self {
            path: path.to_path_buf(),
            dialect,
            tree,
            bindings,
            values,
            types,
        }
    }

    /// Whether the semantic layer models this file's dialect at all.
    ///
    /// A report must print this rather than an empty finding list: the two
    /// look identical to a consumer, and only one of them means "nothing to
    /// report here".
    #[must_use]
    pub fn dialect_is_modelled(&self) -> bool {
        self.dialect == Dialect::CommonLisp
    }
}

/// The source text a span covers, with interior whitespace collapsed and a
/// long span elided.
///
/// Reports quote the form they found so the finding can be read without
/// opening the file. An unbounded quote defeats that — a `defun` body is not
/// a label — so this caps at `limit` characters.
#[must_use]
pub fn snippet(source: &str, span: ByteSpan, limit: usize) -> String {
    let text = source
        .get(span.start().get()..span.end().get())
        .unwrap_or_default();
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= limit {
        return collapsed;
    }
    let truncated = collapsed
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::ByteOffset;

    fn span(start: usize, end: usize) -> ByteSpan {
        ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
    }

    #[test]
    fn a_snippet_collapses_interior_whitespace() {
        assert_eq!(snippet("(a\n   b)", span(0, 8), 40), "(a b)");
    }

    #[test]
    fn a_long_snippet_is_elided_rather_than_cut_mid_output() {
        let source = "(aaaaaaaaaa)";
        assert_eq!(snippet(source, span(0, source.len()), 5), "(aaa…");
    }

    #[test]
    fn a_snippet_of_a_span_outside_the_source_is_empty() {
        assert_eq!(snippet("(a)", span(90, 100), 40), "");
    }
}
