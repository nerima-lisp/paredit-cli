use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SymbolName;

use super::super::scope::{LocalCallableRenameKind, MacroletRenameScope};

#[derive(Clone, Copy)]
pub struct TraversalContext<'a> {
    pub dialect: Dialect,
    pub from: &'a SymbolName,
    pub to: &'a SymbolName,
    pub kind: LocalCallableRenameKind,
}

#[derive(Clone, Copy)]
pub struct TraversalState {
    pub scope: MacroletRenameScope,
    pub reader_lambda_body_scope: MacroletRenameScope,
    pub quasiquote_depth: usize,
}

impl TraversalState {
    pub const fn with_scope(&self, scope: MacroletRenameScope) -> Self {
        Self {
            scope,
            reader_lambda_body_scope: self.reader_lambda_body_scope,
            quasiquote_depth: self.quasiquote_depth,
        }
    }

    pub const fn with_scopes(
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

    pub const fn with_quasiquote_depth(&self, quasiquote_depth: usize) -> Self {
        Self {
            scope: self.scope,
            reader_lambda_body_scope: self.reader_lambda_body_scope,
            quasiquote_depth,
        }
    }

    pub const fn allows_active_rename(&self, scope: MacroletRenameScope) -> bool {
        self.quasiquote_depth == 0 && scope.is_target_active() && !scope.is_shadowed()
    }
}
