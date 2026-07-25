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

use crate::domain::dialect::Dialect;
use crate::domain::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::sexpr::{Delimiter, ExpressionKind, SymbolName, SyntaxTree};
use anyhow::{Context, Result};
use pipeline::pipeline_step;
use rewrite::{replace_span, unthread_replacement};
use syntax::{atom_child, expression_source};

pub fn plan_unthread_expression(
    request: UnthreadExpressionRequest<'_>,
) -> Result<UnthreadExpressionPlan> {
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
            anyhow::bail!("unthread-expression does not support dialect unknown");
        }
    }

    reject_common_lisp_reader_conditionals(request.tree, request.dialect)?;

    if request.target.kind != ExpressionKind::List
        || request.target.delimiter != Some(Delimiter::Paren)
    {
        anyhow::bail!("unthread-expression target must be a parenthesized threading pipeline");
    }

    let head = atom_child(&request.target, 0)
        .context("unthread-expression target must start with an atom operator")?;
    if let Some(expected) = &request.operator {
        if head != expected.as_str() {
            anyhow::bail!(
                "unthread-expression operator mismatch: selected {head}, expected {expected}"
            );
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
        anyhow::bail!(
            "unthread-expression operator {} is not a recognized threading operator (->, ->>); pass --operator to confirm a custom threading macro",
            operator
        );
    }
    let style = match (request.style, recognized) {
        (Some(style), _) => style,
        (None, Some(style)) => style,
        (None, None) => anyhow::bail!(
            "unthread-expression custom operator {} requires --style",
            operator
        ),
    };

    if request.target.children.len() < 3 {
        anyhow::bail!("unthread-expression pipeline must contain a base and at least one step");
    }
    // Unthreading rebuilds the pipeline as nested calls from parsed steps; a
    // comment anywhere inside the selection lives outside the tree and has
    // no slot in the rebuilt text, so it would be silently dropped.
    if request.tree.has_comment_in(request.target.span) {
        anyhow::bail!(
            "unthread-expression target contains a comment, which would be discarded by \
             re-nesting into calls; remove or relocate the comment before unthreading"
        );
    }

    let base_view = &request.target.children[1];
    let base = expression_source(request.input, base_view);
    let pipeline_steps = request
        .target
        .children
        .iter()
        .skip(2)
        .map(|view| pipeline_step(request.input, view))
        .collect::<Result<Vec<_>>>()?;
    let (replacement, steps) = unthread_replacement(style, &base, pipeline_steps);
    SyntaxTree::parse_with_dialect(&replacement, request.dialect)
        .context("unthread-expression replacement does not parse")?;
    let rewritten = replace_span(request.input, request.target.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("unthread-expression rewritten output does not parse")?;
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
