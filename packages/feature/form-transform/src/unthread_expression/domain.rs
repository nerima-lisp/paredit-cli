//! Use case for converting threading pipelines back into nested calls.

#[cfg(test)]
mod tests;

mod pipeline;
mod rewrite;
mod syntax;
mod types;

pub use types::{
    UnthreadExpressionPlan, UnthreadExpressionRequest, UnthreadExpressionStep, UnthreadStyle,
};

use paredit_core_edit::DocumentRefusal;

use crate::error::{
    CommentWouldBeDiscardedError, FormTransformResult, TransformDialectError, TransformTargetError,
};
use paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, SymbolName, SyntaxTree};
use pipeline::pipeline_step;
use rewrite::{replace_span, unthread_replacement};
use syntax::{atom_child, expression_source};

pub fn plan_unthread_expression(
    request: UnthreadExpressionRequest<'_>,
) -> FormTransformResult<UnthreadExpressionPlan> {
    match request.dialect {
        Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Clojure
        | Dialect::Janet
        | Dialect::Fennel => {}
        Dialect::Unknown => {
            return Err(TransformDialectError::Unknown {
                operation: "unthread-expression",
            }
            .into());
        }
    }

    reject_common_lisp_reader_conditionals(request.tree, request.dialect)?;

    if request.target.kind != ExpressionKind::List
        || request.target.delimiter != Some(Delimiter::Paren)
    {
        return Err(TransformTargetError::UnthreadTargetNotAPipeline.into());
    }

    let head =
        atom_child(&request.target, 0).ok_or(TransformTargetError::UnthreadTargetHeadNotAnAtom)?;
    if let Some(expected) = &request.operator {
        if head != expected.as_str() {
            return Err(TransformTargetError::UnthreadOperatorMismatch {
                head: head.to_owned(),
                expected: expected.to_string(),
            }
            .into());
        }
    }
    let explicit_operator = request.operator.is_some();
    let operator = match request.operator {
        Some(operator) => operator,
        None => SymbolName::new(head)?,
    };
    let recognized = UnthreadStyle::from_operator(operator.as_str());
    if !explicit_operator && recognized.is_none() {
        // Without an explicit --operator confirming the caller's intent, an
        // unrecognized head is not known to be a threading pipeline at all —
        // trusting a bare --style here would rewrite an ordinary call (e.g.
        // `(+ a b)`) into garbage nested-call output.
        return Err(TransformTargetError::UnthreadOperatorUnrecognized {
            operator: operator.to_string(),
        }
        .into());
    }
    let style = match (request.style, recognized) {
        (Some(style), _) => style,
        (None, Some(style)) => style,
        (None, None) => {
            return Err(TransformTargetError::UnthreadCustomOperatorNeedsStyle {
                operator: operator.to_string(),
            }
            .into());
        }
    };

    if request.target.children.len() < 3 {
        return Err(TransformTargetError::UnthreadPipelineTooShort.into());
    }
    // Unthreading rebuilds the pipeline as nested calls from parsed steps; a
    // comment anywhere inside the selection lives outside the tree and has
    // no slot in the rebuilt text, so it would be silently dropped.
    if request.tree.has_comment_in(request.target.span) {
        return Err(CommentWouldBeDiscardedError::Unthreading.into());
    }

    let base_view = &request.target.children[1];
    let base = expression_source(request.input, base_view);
    let pipeline_steps = request
        .target
        .children
        .iter()
        .skip(2)
        .map(|view| pipeline_step(request.input, view))
        .collect::<FormTransformResult<Vec<_>>>()?;
    let (replacement, steps) = unthread_replacement(style, &base, pipeline_steps);
    SyntaxTree::parse_with_dialect(&replacement, request.dialect).map_err(|source| {
        DocumentRefusal::ReplacementDoesNotParse {
            operation: "unthread-expression",
            source,
        }
    })?;
    let rewritten = replace_span(request.input, request.target.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::RewrittenDoesNotParse {
            operation: "unthread-expression",
            source,
        }
    })?;
    let changed = rewritten != request.input;

    Ok(UnthreadExpressionPlan {
        dialect: request.dialect,
        path: request.path,
        style,
        operator,
        span: request.target.span,
        base,
        steps,
        replacement,
        rewritten,
        changed,
    })
}
