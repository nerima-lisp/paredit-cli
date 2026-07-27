use anyhow::{Result, anyhow};

use paredit_core_syntax::sexpr::{AtomOccurrenceIndex, ByteSpan, ExpressionView, Path};

#[derive(Clone, Copy)]
pub struct AtomPathIndex<'a> {
    occurrences: &'a AtomOccurrenceIndex<'a>,
}

impl<'a> AtomPathIndex<'a> {
    pub const fn new(occurrences: &'a AtomOccurrenceIndex<'a>) -> Self {
        Self { occurrences }
    }

    pub fn path_for_span(&self, span: ByteSpan) -> Option<Path> {
        self.occurrences.path_for_span(span)
    }

    fn last_index_for_span(&self, span: ByteSpan) -> Option<usize> {
        self.occurrences.last_index_for_span(span)
    }
}

pub fn is_common_lisp_value_position(atom_paths: AtomPathIndex<'_>, span: ByteSpan) -> bool {
    atom_paths
        .last_index_for_span(span)
        .is_some_and(|index| index != 0)
}

pub fn ancestor_views<'a>(
    root: &'a ExpressionView,
    path: &Path,
) -> Result<Vec<&'a ExpressionView>> {
    let indexes = path.to_raw_indexes();
    let mut ancestors = Vec::with_capacity(indexes.len().saturating_sub(1));
    let mut view = root;
    for &index in indexes.iter().take(indexes.len().saturating_sub(1)) {
        view = view.children.get(index).ok_or_else(|| {
            anyhow!(
                "path index {index} is out of bounds for {} children",
                view.children.len()
            )
        })?;
        ancestors.push(view);
    }
    Ok(ancestors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::{ByteOffset, SyntaxTree};

    #[test]
    fn resolves_atom_paths_by_span_without_owning_them() {
        let tree = SyntaxTree::parse("(alpha (beta gamma))").expect("tree");
        let occurrences = tree.atom_occurrence_index();
        let index = AtomPathIndex::new(&occurrences);

        for occurrence in occurrences.occurrences() {
            assert_eq!(
                tree.select_path(&index.path_for_span(occurrence.span).expect("path"))
                    .expect("selection")
                    .span(),
                occurrence.span
            );
        }
        assert_eq!(
            index.path_for_span(ByteSpan::new(ByteOffset::new(2), ByteOffset::new(3))),
            None
        );
    }
}
