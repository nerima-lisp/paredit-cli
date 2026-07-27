use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionView, Path, SymbolName};

use super::RenameFunctionOccurrence;
use super::scope::{LocalCallableRenameKind, MacroletRenameScope};
use super::traversal::{CallTraversal, collect_renames_from_view};

#[expect(
    clippy::too_many_arguments,
    reason = "macrolet call traversal carries recursive scope and quasiquote state"
)]
pub(super) fn collect_macrolet_call_head_renames_from_view(
    view: &ExpressionView,
    path: Path,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
    kind: LocalCallableRenameKind,
    scope: MacroletRenameScope,
    reader_lambda_body_scope: MacroletRenameScope,
    quasiquote_depth: usize,
    renames: &mut Vec<RenameFunctionOccurrence>,
) {
    collect_renames_from_view::<CallTraversal>(
        view,
        path,
        dialect,
        from,
        to,
        kind,
        scope,
        reader_lambda_body_scope,
        quasiquote_depth,
        renames,
    );
}
