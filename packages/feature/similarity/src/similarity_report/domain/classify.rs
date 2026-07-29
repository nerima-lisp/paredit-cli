//! Labelling a reported pair on the clone taxonomy.
//!
//! Lives beside the form model rather than in the clone slice because both the
//! pair report and the class report want it, and the pair report must not
//! depend on the slice built on top of it.

use crate::form_similarity::{CloneClassification, StructuralTree, classify_clone};
use paredit_core_syntax::sexpr::{Path, SyntaxTree};

use super::types::SimilarityFormReport;

/// Rebuilds the structural tree of an already-reported form from its own text.
///
/// The alternative was carrying every candidate's tree past the point the
/// comparison pass consumes them, which doubles the peak memory of a run for
/// the sake of the few thousand forms that end up in the report. Reparsing the
/// exact source slice with the same dialect reconstructs the identical tree —
/// the slice is a whole form by construction, so it parses to exactly one root
/// child — and only the reported forms pay for it.
///
/// Returns `None` if that invariant somehow does not hold, which callers report
/// as an unclassified edge rather than guessing at a clone type.
#[must_use]
pub fn structural_tree_of(form: &SimilarityFormReport) -> Option<StructuralTree> {
    let text: &str = form.text().as_ref();
    let tree = SyntaxTree::parse_with_dialect(text, form.dialect()).ok()?;
    if tree.root_children().len() != 1 {
        return None;
    }
    let selection = tree.select_path(&Path::root_child(0)).ok()?;
    Some(StructuralTree::from_view(&selection.view()))
}

/// Classifies one reported pair on the clone taxonomy.
///
/// `None` means a form could not be reparsed into a structural tree, which
/// callers surface as unclassified rather than guessing at a clone type.
#[must_use]
pub fn classify_form_pair(
    left: &SimilarityFormReport,
    right: &SimilarityFormReport,
) -> Option<CloneClassification> {
    Some(classify_clone(
        &structural_tree_of(left)?,
        &structural_tree_of(right)?,
    ))
}
