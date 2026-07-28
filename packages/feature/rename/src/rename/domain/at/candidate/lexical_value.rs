use crate::error::RenameResult;

use super::super::RenameAtError;
use super::super::RenameAtNamespace;
use super::super::safety::ensure_binding_target_is_available;
use super::super::selection::{AtomPathIndex, ancestor_views, is_value_position};
use super::Candidate;
use crate::rename::domain::{binding_rename_parts, selection::apply_byte_span_edits};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionView, Path, SymbolName, SyntaxTree};

/// Everything the lexical-binding search needs about one `rename-at` request.
pub struct BindingCandidateContext<'a> {
    pub tree: &'a SyntaxTree,
    pub dialect: Dialect,
    pub root_view: &'a ExpressionView,
    pub atom_paths: AtomPathIndex<'a>,
    pub input: &'a str,
    pub path: &'a Path,
    pub from: &'a SymbolName,
    pub to: &'a SymbolName,
}

pub fn binding_candidates(context: BindingCandidateContext<'_>) -> RenameResult<Vec<Candidate>> {
    let BindingCandidateContext {
        tree,
        dialect,
        root_view,
        atom_paths,
        input,
        path,
        from,
        to,
    } = context;
    // Every dialect `plan_rename_at` accepts has verified rename-binding
    // semantics; `supports_rename_at_dialect` is what guarantees it.
    let semantic = dialect
        .verify_rename_binding()
        .map_err(|_| RenameAtError::UnsupportedDialect)?;
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
            .filter(|span| is_value_position(dialect, atom_paths, *span))
            .collect();
        if parts.binding_span != selected_span && !reference_spans.contains(&selected_span) {
            continue;
        }
        ensure_binding_target_is_available(dialect, view, from, to, parts.binding_span, input)?;
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
