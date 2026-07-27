use crate::domain::callable_scope::{common_lisp_local_callable_form, local_callable_names};
use crate::domain::common_lisp::CommonLispLocalCallableForm;
use crate::domain::definition::{DefinitionBodyRange, definition_shape};
use crate::domain::sexpr::{Delimiter, ExpressionKind};

use super::super::RenameFunctionOccurrence;
use super::super::scope::{local_callable_scopes, target_binding_presence};
use super::core::{RenameTraversalMode, TraversalFrame, TraversalPathArena, TraversalTask};
use super::state::TraversalContext;

pub(in crate::domain::rename::macrolet) enum LocalCallableTraversal {
    Handled,
    DefinitionBody(Option<DefinitionBodyRange>),
}

pub(in crate::domain::rename::macrolet) fn collect_local_callable_or_definition<
    'a,
    M: RenameTraversalMode,
>(
    frame: TraversalFrame<'a>,
    context: TraversalContext<'_>,
    paths: &mut TraversalPathArena,
    tasks: &mut Vec<TraversalTask<'a>>,
    renames: &mut Vec<RenameFunctionOccurrence>,
) -> LocalCallableTraversal {
    if frame.view.kind != ExpressionKind::List || frame.view.delimiter != Some(Delimiter::Paren) {
        return LocalCallableTraversal::DefinitionBody(None);
    }

    let Some(head) = crate::domain::rename::selection::list_head(frame.view) else {
        M::collect_list_head_renames(frame.view, frame.path, paths, context, frame.state, renames);
        return LocalCallableTraversal::DefinitionBody(None);
    };

    if let Some(form) = common_lisp_local_callable_form(context.dialect, head) {
        schedule_local_callable_form_renames::<M>(frame, context, paths, tasks, form);
        return LocalCallableTraversal::Handled;
    }

    let shape = definition_shape(context.dialect, frame.view, head);
    if shape.is_none() {
        M::collect_list_head_renames(frame.view, frame.path, paths, context, frame.state, renames);
    }
    LocalCallableTraversal::DefinitionBody(shape.map(|shape| shape.body_range()))
}

fn schedule_local_callable_form_renames<'a, M: RenameTraversalMode>(
    frame: TraversalFrame<'a>,
    context: TraversalContext<'_>,
    paths: &mut TraversalPathArena,
    tasks: &mut Vec<TraversalTask<'a>>,
    form: CommonLispLocalCallableForm,
) {
    let local_names = local_callable_names(frame.view);
    let target_binding = target_binding_presence(&local_names, context.from);
    let scopes = local_callable_scopes(frame.state.scope, context.kind, form, target_binding);
    let form_body_state = frame
        .state
        .with_scopes(scopes.body, scopes.body)
        .with_quasiquote_depth(0);

    for (index, child) in frame.view.children.iter().enumerate().skip(2).rev() {
        let child_path = paths.child(frame.path, index);
        tasks.push(TraversalTask::Visit(TraversalFrame {
            view: child,
            path: child_path,
            state: form_body_state,
        }));
    }

    let binding_body_state = frame
        .state
        .with_scopes(scopes.binding_body, scopes.body)
        .with_quasiquote_depth(0);
    if let Some(bindings) = frame.view.children.get(1) {
        for (binding_index, binding) in bindings.children.iter().enumerate().rev() {
            for (child_index, child) in binding.children.iter().enumerate().skip(2).rev() {
                let child_path = paths.descendant(frame.path, [1, binding_index, child_index]);
                tasks.push(TraversalTask::Visit(TraversalFrame {
                    view: child,
                    path: child_path,
                    state: binding_body_state,
                }));
            }
            tasks.push(TraversalTask::BindingName {
                binding,
                binding_index,
                path: frame.path,
                form,
                state: frame.state,
            });
        }
    }
}
