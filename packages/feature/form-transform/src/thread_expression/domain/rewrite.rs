use super::types::ThreadExpressionStep;
use crate::domain::sexpr::{ByteSpan, SymbolName};

pub(super) fn thread_expression_replacement(
    operator: &SymbolName,
    base: &str,
    steps: &[ThreadExpressionStep],
) -> String {
    let mut forms = Vec::with_capacity(steps.len() + 2);
    forms.push(operator.as_str().to_owned());
    forms.push(base.to_owned());
    forms.extend(steps.iter().map(|step| step.step.clone()));
    format!("({})", forms.join(" "))
}

pub(super) fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() - span.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}
