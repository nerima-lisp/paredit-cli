use super::syntax::{atom_child, atom_text, expression_source};
use super::types::PipelineStep;
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};
use anyhow::{Context, Result};

pub(super) fn pipeline_step(input: &str, view: &ExpressionView) -> Result<PipelineStep> {
    match view.kind {
        ExpressionKind::Atom => {
            let head =
                atom_text(view).context("unthread-expression atom step must have symbol text")?;
            Ok(PipelineStep {
                head: head.to_owned(),
                arguments: Vec::new(),
                span: view.span,
                form: head.to_owned(),
            })
        }
        ExpressionKind::List if view.delimiter == Some(Delimiter::Paren) => {
            let head = atom_child(view, 0)
                .context("unthread-expression list step must start with an atom head")?
                .to_owned();
            let arguments = view
                .children
                .iter()
                .skip(1)
                .map(|child| expression_source(input, child))
                .collect::<Vec<_>>();
            Ok(PipelineStep {
                head,
                arguments,
                span: view.span,
                form: expression_source(input, view),
            })
        }
        _ => anyhow::bail!(
            "unthread-expression step must be an atom or parenthesized call at {}..{}",
            view.span.start().get(),
            view.span.end().get()
        ),
    }
}
