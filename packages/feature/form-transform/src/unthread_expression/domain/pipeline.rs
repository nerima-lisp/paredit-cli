use super::syntax::{atom_child, atom_text, expression_source};
use super::types::PipelineStep;
use crate::error::{FormTransformResult, TransformTargetError};
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView};

pub fn pipeline_step(input: &str, view: &ExpressionView) -> FormTransformResult<PipelineStep> {
    match view.kind {
        ExpressionKind::Atom => {
            let head = atom_text(view).ok_or(TransformTargetError::UnthreadAtomStepHasNoText)?;
            Ok(PipelineStep {
                head: head.to_owned(),
                arguments: Vec::new(),
                span: view.span,
                form: head.to_owned(),
            })
        }
        ExpressionKind::List if view.delimiter == Some(Delimiter::Paren) => {
            let head = atom_child(view, 0)
                .ok_or(TransformTargetError::UnthreadListStepHeadNotAnAtom)?
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
        _ => Err(TransformTargetError::UnthreadStepNotAtomOrCall {
            start: view.span.start().get(),
            end: view.span.end().get(),
        }
        .into()),
    }
}
