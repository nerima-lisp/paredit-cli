use paredit_core_edit::DocumentRefusal;

use crate::error::{BindingSelectionError, RenameResult};

use super::reader::executable_reader_context_at_path;
use paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{SymbolName, SyntaxTree};

mod candidate;
mod error;
mod safety;
mod selection;
mod types;

use candidate::{SpecializedCandidateContext, add_specialized_candidates, binding_candidates};
pub use error::RenameAtError;
use selection::AtomPathIndex;
pub use types::{RenameAtNamespace, RenameAtPlan, RenameAtRequest};

#[must_use]
pub const fn supports_rename_at_dialect(dialect: Dialect) -> bool {
    matches!(dialect, Dialect::CommonLisp)
}

pub fn plan_rename_at(request: RenameAtRequest<'_>) -> RenameResult<RenameAtPlan> {
    if !supports_rename_at_dialect(request.dialect) {
        return Err(RenameAtError::UnsupportedDialect.into());
    }
    if request.at.get() >= request.input.len() || !request.input.is_char_boundary(request.at.get())
    {
        return Err(RenameAtError::InvalidSelection.into());
    }

    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .map_err(|source| DocumentRefusal::InputParseFailed { source })?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect).map_err(RenameAtError::from)?;
    let atom_occurrences = tree.atom_occurrence_index();
    let atom_paths = AtomPathIndex::new(&atom_occurrences);
    let selected = atom_occurrences
        .occurrences()
        .iter()
        .find(|occurrence| occurrence.span.contains(request.at))
        .ok_or(RenameAtError::InvalidSelection)?;
    let path = atom_paths
        .path_for_span(selected.span)
        .ok_or(RenameAtError::InvalidSelection)?;
    if !executable_reader_context_at_path(&tree, request.dialect, &path)? {
        return Err(RenameAtError::InertReaderContext.into());
    }
    if selected.text.contains(':') || request.to.as_str().contains(':') {
        return Err(RenameAtError::UnsupportedPackageSyntax.into());
    }
    let from =
        SymbolName::new(selected.text.to_owned()).map_err(|_| BindingSelectionError::NotASymbol)?;
    let root_view = tree.root_view();
    let mut candidates = binding_candidates(
        &tree,
        &root_view,
        atom_paths,
        request.input,
        &path,
        &from,
        &request.to,
    )?;
    add_specialized_candidates(
        &mut candidates,
        SpecializedCandidateContext {
            input: request.input,
            dialect: request.dialect,
            tree: &tree,
            root_view: &root_view,
            atom_paths,
            path: &path,
            selected_span: selected.span,
            from: &from,
            to: &request.to,
        },
    )?;

    let candidate = match candidates.len() {
        0 => return Err(RenameAtError::Unresolved.into()),
        1 => candidates.pop().ok_or(RenameAtError::Unresolved)?,
        _ => return Err(RenameAtError::Ambiguous.into()),
    };
    SyntaxTree::parse_with_dialect(&candidate.rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "renamed",
            source,
        }
    })?;
    Ok(RenameAtPlan {
        dialect: request.dialect,
        namespace: candidate.namespace,
        selection_span: selected.span,
        from,
        to: request.to,
        occurrences: candidate.occurrences,
        changed: candidate.rewritten != request.input,
        rewritten: candidate.rewritten,
    })
}

#[cfg(test)]
mod tests;
