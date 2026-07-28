//! Use case for converting nested calls into threading pipelines.

#[cfg(test)]
mod tests;

mod parts;
mod rewrite;
mod syntax;
mod types;

pub use types::{ThreadExpressionPlan, ThreadExpressionRequest, ThreadExpressionStep, ThreadStyle};

use paredit_core_edit::DocumentRefusal;

use crate::error::{
    CommentWouldBeDiscardedError, FormTransformResult, TransformDialectError, TransformTargetError,
};
use paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals;
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use parts::thread_expression_parts;
use rewrite::{replace_span, thread_expression_replacement};
use syntax::list_head;

pub fn plan_thread_expression(
    request: ThreadExpressionRequest<'_>,
) -> FormTransformResult<ThreadExpressionPlan> {
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
                operation: "thread-expression",
            }
            .into());
        }
    }

    reject_common_lisp_reader_conditionals(request.tree, request.dialect)?;

    let already_threaded = list_head(&request.target).is_some_and(|head| match request.dialect {
        Dialect::CommonLisp => common_lisp_symbol_reference_eq(head, request.operator.as_str()),
        Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Clojure
        | Dialect::Janet
        | Dialect::Fennel => head == request.operator.as_str(),
        Dialect::Unknown => false,
    });
    if already_threaded {
        return Err(TransformTargetError::AlreadyThreaded {
            operator: request.operator.to_string(),
        }
        .into());
    }
    // Threading rebuilds the nested calls as a flat pipeline from parsed
    // parts; a comment anywhere inside the selection lives outside the tree
    // and has no slot in the rebuilt text, so it would be silently dropped.
    if request.tree.has_comment_in(request.target.span) {
        return Err(CommentWouldBeDiscardedError::Threading.into());
    }

    let parts = thread_expression_parts(request.input, &request.target, request.style)?;
    if parts.steps.is_empty() {
        return Err(TransformTargetError::ThreadProducedNoSteps.into());
    }
    let replacement = thread_expression_replacement(&request.operator, &parts.base, &parts.steps);
    SyntaxTree::parse_with_dialect(&replacement, request.dialect).map_err(|source| {
        DocumentRefusal::ReplacementDoesNotParse {
            operation: "thread-expression",
            source,
        }
    })?;
    let rewritten = replace_span(request.input, request.target.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::RewrittenDoesNotParse {
            operation: "thread-expression",
            source,
        }
    })?;
    let changed = rewritten != request.input;

    Ok(ThreadExpressionPlan {
        dialect: request.dialect,
        path: request.path,
        style: request.style,
        operator: request.operator,
        span: request.target.span,
        base: parts.base,
        steps: parts.steps,
        replacement,
        rewritten,
        changed,
    })
}
