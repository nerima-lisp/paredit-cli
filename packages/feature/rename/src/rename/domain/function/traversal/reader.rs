use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::rename::reader::{
    bare_lambda_body_children, explicit_reader_form_kind,
    explicit_reader_function_lambda_body_children,
};
use crate::domain::sexpr::{ExpressionKind, ExpressionView, ReaderPrefix};

use super::super::RenameFunctionOccurrence;
use super::super::target::callable_name_target;
use super::core::{
    TraversalContext, TraversalFrame, TraversalPath, TraversalPathArena, TraversalState,
    allows_function_reference_rename,
};

fn collect_callable_target_rename(
    target_view: &ExpressionView,
    target_path: TraversalPath,
    state: &TraversalState,
    paths: &TraversalPathArena,
    context: &TraversalContext<'_>,
    renames: &mut Vec<RenameFunctionOccurrence>,
) -> bool {
    let target_path = paths.materialize(target_path);
    let Some(target) = callable_name_target(target_view, &target_path) else {
        return false;
    };

    if !common_lisp_symbol_reference_eq(target.text, context.from.as_str())
        || !allows_function_reference_rename(state, target.text)
    {
        return false;
    }

    renames.push(RenameFunctionOccurrence {
        path: target.path.to_string(),
        span: target.span,
        text: target.text.to_owned(),
        replacement: context.to.as_str().to_owned(),
    });
    true
}

pub(in crate::domain::rename::function) fn collect_function_designator_renames(
    view: &ExpressionView,
    state: &TraversalState,
    paths: &TraversalPathArena,
    context: &TraversalContext<'_>,
    renames: &mut Vec<RenameFunctionOccurrence>,
) -> bool {
    if state.quasiquote_depth == 0 && view.reader_prefixes.contains(&ReaderPrefix::Function) {
        return collect_callable_target_rename(view, state.path, state, paths, context, renames);
    }

    false
}

/// Handles a bare `(lambda ...)` form directly, skipping its parameter list
/// the same way the `#'(lambda ...)` case below skips it via
/// `explicit_reader_function_lambda_body_children`; see `bare_lambda_body_children`.
pub(in crate::domain::rename::function) fn collect_bare_lambda_call_renames<'a>(
    view: &'a ExpressionView,
    state: &TraversalState,
    paths: &mut TraversalPathArena,
    stack: &mut Vec<TraversalFrame<'a>>,
) -> bool {
    let Some(children) = bare_lambda_body_children(view) else {
        return false;
    };

    let children: Vec<_> = children.collect();
    for (child_index, child) in children.into_iter().rev() {
        let path = paths.child(state.path, child_index);
        stack.push(TraversalFrame {
            view: child,
            state: state.with_path(path),
        });
    }
    true
}

pub(in crate::domain::rename::function) fn collect_explicit_reader_form_call_renames<'a>(
    view: &'a ExpressionView,
    context: &TraversalContext<'_>,
    state: TraversalState,
    paths: &mut TraversalPathArena,
    renames: &mut Vec<RenameFunctionOccurrence>,
    stack: &mut Vec<TraversalFrame<'a>>,
) -> bool {
    if view.kind != ExpressionKind::List || view.children.len() < 2 {
        return false;
    }

    let Some(head) = explicit_reader_form_kind(view) else {
        return false;
    };

    match head.as_str() {
        "quote" if state.quasiquote_depth == 0 => {
            // Quoted symbols are data, not function designators. Explicit
            // function-valued forms are handled by the branches below.
            true
        }
        "quote" => true,
        "macro-function" | "compiler-macro-function" | "symbol-function" | "fdefinition"
            if state.quasiquote_depth == 0 =>
        {
            if let Some(target) = view.children.get(1) {
                let path = paths.child(state.path, 1);
                collect_callable_target_rename(target, path, &state, paths, context, renames);
            }
            true
        }
        "function" if state.quasiquote_depth == 0 => {
            if let Some(target) = view.children.get(1) {
                let path = paths.child(state.path, 1);
                collect_callable_target_rename(target, path, &state, paths, context, renames);
            }
            if let Some(children) = explicit_reader_function_lambda_body_children(view) {
                let children: Vec<_> = children.collect();
                for (child_index, child) in children.into_iter().rev() {
                    let target = paths.child(state.path, 1);
                    let path = paths.child(target, child_index);
                    stack.push(TraversalFrame {
                        view: child,
                        state: state.with_path(path),
                    });
                }
            }
            true
        }
        "function" => true,
        "quasiquote" => {
            for (index, child) in view.children.iter().enumerate().skip(1).rev() {
                let path = paths.child(state.path, index);
                stack.push(TraversalFrame {
                    view: child,
                    state: state
                        .with_quasiquote_depth(state.quasiquote_depth.saturating_add(1))
                        .with_path(path),
                });
            }
            true
        }
        "unquote" | "unquote-splicing" if state.quasiquote_depth > 0 => {
            for (index, child) in view.children.iter().enumerate().skip(1).rev() {
                let path = paths.child(state.path, index);
                stack.push(TraversalFrame {
                    view: child,
                    state: state
                        .with_quasiquote_depth(state.quasiquote_depth.saturating_sub(1))
                        .with_path(path),
                });
            }
            true
        }
        _ => false,
    }
}
