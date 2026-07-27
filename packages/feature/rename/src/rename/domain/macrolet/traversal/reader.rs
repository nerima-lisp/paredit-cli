use crate::domain::rename::reader::{
    bare_lambda_body_children, explicit_reader_form_kind,
    explicit_reader_function_lambda_body_children,
};

use super::super::RenameFunctionOccurrence;
use super::super::scope::reader_lambda_body_scope as activate_reader_lambda_body_scope;
use super::core::{RenameTraversalMode, TraversalFrame, TraversalPathArena, TraversalTask};
use super::state::TraversalContext;

pub(in crate::domain::rename::macrolet) fn collect_explicit_reader_form_renames<
    'a,
    M: RenameTraversalMode,
>(
    frame: TraversalFrame<'a>,
    context: TraversalContext<'_>,
    paths: &mut TraversalPathArena,
    tasks: &mut Vec<TraversalTask<'a>>,
    renames: &mut Vec<RenameFunctionOccurrence>,
) -> bool {
    let Some(kind_name) = explicit_reader_form_kind(frame.view) else {
        return false;
    };

    match kind_name.as_str() {
        "quote" => true,
        "function" => {
            M::collect_function_reader_target_renames(
                frame.view,
                frame.path,
                paths,
                context,
                frame.state,
                renames,
            );
            if let Some(children) = explicit_reader_function_lambda_body_children(frame.view) {
                let lambda_scope =
                    activate_reader_lambda_body_scope(frame.state.reader_lambda_body_scope);
                let lambda_state = frame
                    .state
                    .with_scopes(lambda_scope, lambda_scope)
                    .with_quasiquote_depth(frame.state.quasiquote_depth);
                let children: Vec<_> = children.collect();
                for (child_index, child) in children.into_iter().rev() {
                    let child_path = paths.descendant(frame.path, [1, child_index]);
                    tasks.push(TraversalTask::ExplicitFunctionLambdaAtom(TraversalFrame {
                        view: child,
                        path: child_path,
                        state: lambda_state,
                    }));
                }
            }
            true
        }
        "quasiquote" => {
            let frame = TraversalFrame {
                state: frame
                    .state
                    .with_quasiquote_depth(frame.state.quasiquote_depth + 1),
                ..frame
            };
            schedule_explicit_reader_children(frame, paths, tasks);
            true
        }
        "unquote" | "unquote-splicing" if frame.state.quasiquote_depth > 0 => {
            let frame = TraversalFrame {
                state: frame
                    .state
                    .with_quasiquote_depth(frame.state.quasiquote_depth - 1),
                ..frame
            };
            schedule_explicit_reader_children(frame, paths, tasks);
            true
        }
        _ => false,
    }
}

/// Handles a bare `(lambda ...)` form directly, not just the `#'(lambda ...)`
/// spelling handled by the "function" arm above; see `bare_lambda_body_children`.
pub(in crate::domain::rename::macrolet) fn collect_reader_lambda_renames<
    'a,
    M: RenameTraversalMode,
>(
    frame: TraversalFrame<'a>,
    _context: TraversalContext<'_>,
    paths: &mut TraversalPathArena,
    tasks: &mut Vec<TraversalTask<'a>>,
) -> bool {
    let Some(children) = bare_lambda_body_children(frame.view) else {
        return false;
    };

    let lambda_scope = activate_reader_lambda_body_scope(frame.state.reader_lambda_body_scope);
    let lambda_state = frame
        .state
        .with_scopes(lambda_scope, lambda_scope)
        .with_quasiquote_depth(frame.state.quasiquote_depth);
    let children: Vec<_> = children.collect();
    for (child_index, child) in children.into_iter().rev() {
        let child_path = paths.child(frame.path, child_index);
        tasks.push(TraversalTask::ReaderQuotedLambdaAtom(TraversalFrame {
            view: child,
            path: child_path,
            state: lambda_state,
        }));
    }
    true
}

fn schedule_explicit_reader_children<'a>(
    frame: TraversalFrame<'a>,
    paths: &mut TraversalPathArena,
    tasks: &mut Vec<TraversalTask<'a>>,
) {
    let Some((_, explicit_children)) = frame.view.children.split_first() else {
        return;
    };
    let explicit_frame = TraversalFrame {
        view: frame.view,
        path: frame.path,
        state: frame.state,
    };
    for (offset, child) in explicit_children.iter().enumerate().rev() {
        let child_path = paths.child(explicit_frame.path, offset + 1);
        tasks.push(TraversalTask::Visit(TraversalFrame {
            view: child,
            path: child_path,
            state: explicit_frame.state,
        }));
    }
}
