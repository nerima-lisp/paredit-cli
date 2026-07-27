use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::rename::macrolet::RenameFunctionOccurrence;
use crate::domain::sexpr::ExpressionView;

use super::super::super::reader::{
    atom_symbol_span, atom_symbol_text, callable_target, collect_local_function_designator_renames,
    push_callable_target_rename_if_match,
};
use super::super::super::scope::{LocalCallableRenameKind, allows_function_reference_rename};
use super::super::core::{RenameTraversalMode, TraversalPath, TraversalPathArena};
use super::super::state::{TraversalContext, TraversalState};
use super::common::{callable_list_head_target, collect_active_atom_rename};

pub(in crate::domain::rename::macrolet) struct CallTraversal;

impl RenameTraversalMode for CallTraversal {
    fn collect_pre_reader_renames(
        view: &ExpressionView,
        path: TraversalPath,
        paths: &mut TraversalPathArena,
        context: TraversalContext<'_>,
        state: TraversalState,
        renames: &mut Vec<RenameFunctionOccurrence>,
    ) -> bool {
        collect_local_function_designator_renames(
            view,
            path,
            paths,
            context.from,
            context.to,
            context.kind,
            state.scope,
            state.quasiquote_depth,
            renames,
        )
    }

    fn collect_function_reader_target_renames(
        view: &ExpressionView,
        path: TraversalPath,
        paths: &mut TraversalPathArena,
        context: TraversalContext<'_>,
        state: TraversalState,
        renames: &mut Vec<RenameFunctionOccurrence>,
    ) {
        if context.kind != LocalCallableRenameKind::Function {
            return;
        }

        if let Some(target) = view.children.get(1).and_then(callable_target) {
            if !allows_function_reference_rename(state.scope, target.text) {
                return;
            }
            push_callable_target_rename_if_match(
                target,
                path,
                &[1],
                paths,
                context.from,
                context.to,
                renames,
            );
        }
    }

    fn collect_list_head_renames(
        view: &ExpressionView,
        path: TraversalPath,
        paths: &mut TraversalPathArena,
        context: TraversalContext<'_>,
        state: TraversalState,
        renames: &mut Vec<RenameFunctionOccurrence>,
    ) {
        if context.kind == LocalCallableRenameKind::Function {
            if let Some(target) = callable_list_head_target(view) {
                if allows_function_reference_rename(state.scope, target.text) {
                    push_callable_target_rename_if_match(
                        target,
                        path,
                        &[0],
                        paths,
                        context.from,
                        context.to,
                        renames,
                    );
                }
            }
        }

        let Some(head) = crate::domain::rename::selection::list_head(view) else {
            return;
        };
        if !common_lisp_symbol_reference_eq(head, context.from.as_str())
            || !allows_function_reference_rename(state.scope, head)
        {
            return;
        }

        if let Some(head_view) = view.children.first() {
            let head_path = paths.child(path, 0);
            renames.push(RenameFunctionOccurrence {
                path: paths.materialize(head_path).to_string(),
                span: atom_symbol_span(head_view).unwrap_or(head_view.span),
                text: atom_symbol_text(head_view).unwrap_or(head).to_owned(),
                replacement: context.to.as_str().to_owned(),
            });
        }
    }

    fn collect_explicit_function_lambda_atom_renames(
        child: &ExpressionView,
        child_path: TraversalPath,
        paths: &mut TraversalPathArena,
        context: TraversalContext<'_>,
        state: TraversalState,
        renames: &mut Vec<RenameFunctionOccurrence>,
    ) -> bool {
        collect_active_atom_rename(
            child,
            child_path,
            paths,
            context,
            state,
            state.scope,
            renames,
        )
    }

    fn collect_reader_quoted_lambda_atom_renames(
        child: &ExpressionView,
        child_path: TraversalPath,
        paths: &mut TraversalPathArena,
        context: TraversalContext<'_>,
        state: TraversalState,
        renames: &mut Vec<RenameFunctionOccurrence>,
    ) -> bool {
        collect_active_atom_rename(
            child,
            child_path,
            paths,
            context,
            state,
            state.scope,
            renames,
        )
    }
}
