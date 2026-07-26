use super::ReplaceFormsTarget;
use crate::domain::sexpr::ByteSpan;

pub(super) fn rewrite_replace_targets(
    input: &str,
    targets: &[ReplaceFormsTarget],
    replacement: &str,
) -> String {
    let mut rewrite_order = targets.iter().collect::<Vec<_>>();
    rewrite_order.sort_by_key(|target| std::cmp::Reverse(target.span.start().get()));

    let mut rewritten = input.to_owned();
    for target in rewrite_order {
        rewritten = replace_span(&rewritten, target.span, replacement);
    }

    rewritten
}

fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() - span.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}
