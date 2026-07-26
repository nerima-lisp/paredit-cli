use super::types::{PipelineStep, UnthreadExpressionStep, UnthreadStyle};
use crate::domain::sexpr::ByteSpan;

pub(super) fn unthread_replacement(
    style: UnthreadStyle,
    base: &str,
    pipeline_steps: Vec<PipelineStep>,
) -> (String, Vec<UnthreadExpressionStep>) {
    let mut current = base.to_owned();
    let mut steps = Vec::with_capacity(pipeline_steps.len());

    for pipeline_step in pipeline_steps {
        let mut arguments = pipeline_step.arguments;
        let insertion_index = match style {
            UnthreadStyle::First => 0,
            UnthreadStyle::Last => arguments.len(),
        };
        arguments.insert(insertion_index, current);
        current = format!("({} {})", pipeline_step.head, arguments.join(" "));

        steps.push(UnthreadExpressionStep {
            head: pipeline_step.head,
            argument_count: arguments.len(),
            insertion_index,
            span: pipeline_step.span,
            form: pipeline_step.form,
        });
    }

    (current, steps)
}

pub(super) fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() - span.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}
