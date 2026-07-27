use super::syntax::{expression_source, is_threadable_call, list_head};
use super::types::{ThreadExpressionParts, ThreadExpressionStep, ThreadStyle};
use crate::error::{FormTransformResult, TransformTargetError};
use paredit_core_syntax::sexpr::ExpressionView;

pub fn thread_expression_parts(
    input: &str,
    view: &ExpressionView,
    style: ThreadStyle,
) -> FormTransformResult<ThreadExpressionParts> {
    if !is_threadable_call(view) {
        return Err(TransformTargetError::ThreadTargetNotACall.into());
    }

    let head = list_head(view)
        .ok_or(TransformTargetError::ThreadTargetHeadNotAnAtom)?
        .to_owned();
    let threaded_child_index = style.threaded_child_index(view.children.len());
    let threaded_child = view
        .children
        .get(threaded_child_index)
        .ok_or(TransformTargetError::ThreadTargetMissingArgument)?;

    let mut parts = if is_threadable_call(threaded_child) {
        thread_expression_parts(input, threaded_child, style)?
    } else {
        ThreadExpressionParts {
            base: expression_source(input, threaded_child),
            steps: Vec::new(),
        }
    };

    let arguments = view
        .children
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 0 && *index != threaded_child_index)
        .map(|(_, child)| expression_source(input, child))
        .collect::<Vec<_>>();
    let step = if arguments.is_empty() {
        head.clone()
    } else {
        format!("({head} {})", arguments.join(" "))
    };

    parts.steps.push(ThreadExpressionStep {
        head,
        argument_count: view.children.len().saturating_sub(1),
        threaded_argument_index: threaded_child_index.saturating_sub(1),
        span: view.span,
        step,
    });
    Ok(parts)
}
