//! Use case for converting nested calls into threading pipelines.

#[cfg(test)]
mod tests;

mod parts;
mod rewrite;
mod syntax;
mod types;

pub use types::{ThreadExpressionPlan, ThreadExpressionRequest, ThreadExpressionStep, ThreadStyle};

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::sexpr::SyntaxTree;
use anyhow::{Context, Result};
use parts::thread_expression_parts;
use rewrite::{replace_span, thread_expression_replacement};
use syntax::list_head;

pub fn plan_thread_expression(
    request: ThreadExpressionRequest<'_>,
) -> Result<ThreadExpressionPlan> {
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
            anyhow::bail!("thread-expression does not support dialect unknown");
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
        anyhow::bail!(
            "thread-expression selection is already threaded with {}",
            request.operator
        );
    }
    // Threading rebuilds the nested calls as a flat pipeline from parsed
    // parts; a comment anywhere inside the selection lives outside the tree
    // and has no slot in the rebuilt text, so it would be silently dropped.
    if request.tree.has_comment_in(request.target.span) {
        anyhow::bail!(
            "thread-expression target contains a comment, which would be discarded by \
             flattening into a pipeline; remove or relocate the comment before threading"
        );
    }

    let parts = thread_expression_parts(request.input, &request.target, request.style)?;
    if parts.steps.is_empty() {
        anyhow::bail!("thread-expression target did not produce any pipeline steps");
    }
    let replacement = thread_expression_replacement(&request.operator, &parts.base, &parts.steps);
    SyntaxTree::parse_with_dialect(&replacement, request.dialect)
        .context("thread-expression replacement does not parse")?;
    let rewritten = replace_span(request.input, request.target.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("thread-expression rewritten output does not parse")?;
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
