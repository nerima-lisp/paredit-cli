use anyhow::Result;

use super::super::RenameAtNamespace;
use super::super::safety::ensure_binding_target_is_available;
use super::super::selection::{AtomPathIndex, ancestor_views, is_common_lisp_value_position};
use super::Candidate;
use crate::domain::dialect::Dialect;
use crate::domain::rename::{binding_rename_parts, selection::apply_byte_span_edits};
use crate::domain::sexpr::{ExpressionView, Path, SymbolName, SyntaxTree};

pub(in crate::domain::rename::at) fn binding_candidates(
    tree: &SyntaxTree,
    root_view: &ExpressionView,
    atom_paths: AtomPathIndex<'_>,
    input: &str,
    path: &Path,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<Candidate>> {
    let semantic = Dialect::CommonLisp
        .verify_rename_binding()
        .expect("Common Lisp rename-binding semantics are verified");
    let selected_span = tree.select_path(path)?.span();
    let mut candidates = Vec::new();
    for view in ancestor_views(root_view, path)?.into_iter().rev() {
        let Ok(parts) = binding_rename_parts(semantic, view, from, input) else {
            continue;
        };
        let reference_spans: Vec<_> = parts
            .reference_spans
            .iter()
            .copied()
            .filter(|span| is_common_lisp_value_position(atom_paths, *span))
            .collect();
        if parts.binding_span != selected_span && !reference_spans.contains(&selected_span) {
            continue;
        }
        ensure_binding_target_is_available(view, from, to, parts.binding_span, input)?;
        let mut occurrences = vec![parts.binding_span];
        occurrences.extend(reference_spans.iter().copied());
        let mut edits = vec![(
            parts.binding_edit.span,
            parts.binding_edit.replacement(input, to),
        )];
        edits.extend(
            reference_spans
                .iter()
                .map(|span| (*span, to.as_str().to_owned())),
        );
        candidates.push(Candidate {
            namespace: RenameAtNamespace::Value,
            occurrences,
            rewritten: apply_byte_span_edits(input, edits)?,
        });
        break;
    }
    Ok(candidates)
}
