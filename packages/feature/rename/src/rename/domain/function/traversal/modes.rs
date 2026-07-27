use crate::domain::callable_scope::{common_lisp_local_callable_form, local_callable_names};
use crate::domain::common_lisp::{
    CommonLispLocalCallableForm, CommonLispOperator, common_lisp_symbol_reference_eq,
};
use crate::domain::sexpr::ExpressionView;

use super::super::RenameFunctionOccurrence;
use super::core::{TraversalContext, TraversalFrame, TraversalPathArena, TraversalState};

pub(in crate::domain::rename::function) fn collect_local_callable_function_call_renames<'a>(
    view: &'a ExpressionView,
    head: &str,
    context: &TraversalContext<'_>,
    state: TraversalState,
    paths: &mut TraversalPathArena,
    _renames: &mut Vec<RenameFunctionOccurrence>,
    stack: &mut Vec<TraversalFrame<'a>>,
) -> bool {
    let Some(form) = common_lisp_local_callable_form(context.dialect, head) else {
        return false;
    };

    let shadows_target = local_callable_names(view)
        .iter()
        .any(|name| common_lisp_symbol_reference_eq(name, context.from.as_str()));
    let body_callable_shadowed = state.local_callable_shadowed || shadows_target;
    let body_function_shadowed = if form.is_macro() {
        state.local_function_shadowed
    } else {
        state.local_function_shadowed || shadows_target
    };

    for (index, child) in view.children.iter().enumerate().skip(2).rev() {
        let child_path = paths.child(state.path, index);
        stack.push(TraversalFrame {
            view: child,
            state: state
                .with_local_shadowing(body_callable_shadowed, body_function_shadowed)
                .with_path(child_path),
        });
    }

    if let Some(bindings) = view.children.get(1) {
        let binding_callable_shadowed = if form == CommonLispLocalCallableForm::Labels {
            body_callable_shadowed
        } else {
            state.local_callable_shadowed
        };
        let function_binding_shadowed = if form.is_macro() {
            state.local_function_shadowed
        } else if form == CommonLispLocalCallableForm::Labels {
            body_function_shadowed
        } else {
            state.local_function_shadowed
        };
        for (binding_index, binding) in bindings.children.iter().enumerate().rev() {
            for (child_index, child) in binding.children.iter().enumerate().skip(2).rev() {
                let path = paths.descendant(state.path, [1, binding_index, child_index]);
                stack.push(TraversalFrame {
                    view: child,
                    state: TraversalState {
                        path,
                        local_callable_shadowed: binding_callable_shadowed,
                        local_function_shadowed: function_binding_shadowed,
                        quasiquote_depth: 0,
                        in_macro_expander: state.in_macro_expander || form.is_macro(),
                        shadowed_depth: state.shadowed_depth,
                    },
                });
            }
        }
    }

    true
}

pub(in crate::domain::rename::function) fn collect_symbol_macrolet_function_call_renames<'a>(
    view: &'a ExpressionView,
    head: &str,
    _context: &TraversalContext<'_>,
    state: TraversalState,
    paths: &mut TraversalPathArena,
    _renames: &mut Vec<RenameFunctionOccurrence>,
    stack: &mut Vec<TraversalFrame<'a>>,
) -> bool {
    if CommonLispOperator::from_head(head) != Some(CommonLispOperator::SymbolMacrolet) {
        return false;
    }

    push_body_forms(view, &state, paths, stack);

    if let Some(bindings) = view.children.get(1) {
        for (binding_index, binding) in bindings.children.iter().enumerate().rev() {
            for (child_index, child) in binding.children.iter().enumerate().skip(1).rev() {
                let path = paths.descendant(state.path, [1, binding_index, child_index]);
                stack.push(TraversalFrame {
                    view: child,
                    state: state.with_quasiquote_depth(0).with_path(path),
                });
            }
        }
    }

    true
}

fn push_body_forms<'a>(
    view: &'a ExpressionView,
    state: &TraversalState,
    paths: &mut TraversalPathArena,
    stack: &mut Vec<TraversalFrame<'a>>,
) {
    for (index, child) in view.children.iter().enumerate().skip(2).rev() {
        let path = paths.child(state.path, index);
        stack.push(TraversalFrame {
            view: child,
            state: state.with_path(path),
        });
    }
}
