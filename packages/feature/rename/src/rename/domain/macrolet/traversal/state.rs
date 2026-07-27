use crate::domain::dialect::Dialect;
use crate::domain::sexpr::SymbolName;

use super::super::scope::{LocalCallableRenameKind, MacroletRenameScope};

#[derive(Clone, Copy)]
pub(in crate::domain::rename::macrolet) struct TraversalContext<'a> {
    pub(super) dialect: Dialect,
    pub(super) from: &'a SymbolName,
    pub(super) to: &'a SymbolName,
    pub(super) kind: LocalCallableRenameKind,
}

#[derive(Clone, Copy)]
pub(in crate::domain::rename::macrolet) struct TraversalState {
    pub(super) scope: MacroletRenameScope,
    pub(super) reader_lambda_body_scope: MacroletRenameScope,
    pub(super) quasiquote_depth: usize,
}

impl TraversalState {
    pub(super) const fn with_scope(&self, scope: MacroletRenameScope) -> Self {
        Self {
            scope,
            reader_lambda_body_scope: self.reader_lambda_body_scope,
            quasiquote_depth: self.quasiquote_depth,
        }
    }

    pub(super) const fn with_scopes(
        &self,
        scope: MacroletRenameScope,
        reader_lambda_body_scope: MacroletRenameScope,
    ) -> Self {
        Self {
            scope,
            reader_lambda_body_scope,
            quasiquote_depth: self.quasiquote_depth,
        }
    }

    pub(super) const fn with_quasiquote_depth(&self, quasiquote_depth: usize) -> Self {
        Self {
            scope: self.scope,
            reader_lambda_body_scope: self.reader_lambda_body_scope,
            quasiquote_depth,
        }
    }

    pub(super) const fn allows_active_rename(&self, scope: MacroletRenameScope) -> bool {
        self.quasiquote_depth == 0 && scope.is_target_active() && !scope.is_shadowed()
    }
}
