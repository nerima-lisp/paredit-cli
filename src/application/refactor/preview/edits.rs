use crate::domain::sexpr::ByteSpan;

use super::types::RefactorPreviewEdit;

#[must_use]
pub fn refactor_preview_edits(edits: &[(ByteSpan, String)]) -> Vec<RefactorPreviewEdit> {
    let mut preview_edits = edits
        .iter()
        .map(|(span, replacement)| RefactorPreviewEdit::new(*span, replacement.clone()))
        .collect::<Vec<_>>();
    preview_edits.sort_by_key(|edit| (edit.start(), edit.end()));
    preview_edits
}
