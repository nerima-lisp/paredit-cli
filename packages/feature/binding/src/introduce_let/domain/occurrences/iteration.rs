use crate::domain::{
    dialect::{IntroduceLetOperation, VerifiedSemanticPolicy},
    sexpr::{ByteSpan, ChildIndex, ExpressionView},
};

use super::{
    EquivalentExpressionSpans, collect_equivalent_expression_spans, expressions_equivalent,
    is_path_shadowed_by_binding, is_span_shadowed_by_binding, record_equivalent_span,
};
use crate::domain::introduce_let::syntax::iteration_binding_child_shadowed;

pub(super) fn collect_iteration_binding_spans(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    binding_form: &ExpressionView,
    target: &ExpressionView,
    binding_name: &str,
    shadowed_by_binding: bool,
    output: &mut EquivalentExpressionSpans,
) {
    if expressions_equivalent(binding_form, target) {
        record_equivalent_span(output, binding_form.span, shadowed_by_binding);
        return;
    }

    for (index, child) in binding_form.children.iter().enumerate() {
        let child_shadowed = shadowed_by_binding
            || iteration_binding_child_shadowed(binding_form, binding_name, index);
        collect_equivalent_expression_spans(
            semantic,
            child,
            target,
            binding_name,
            child_shadowed,
            output,
        );
    }
}

pub(super) fn is_span_shadowed_by_iteration_bindings(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    binding_form: &ExpressionView,
    target_span: ByteSpan,
    binding_name: &str,
    shadowed_by_binding: bool,
) -> bool {
    if binding_form.span == target_span {
        return shadowed_by_binding;
    }

    binding_form
        .children
        .iter()
        .enumerate()
        .any(|(index, child)| {
            let child_shadowed = shadowed_by_binding
                || iteration_binding_child_shadowed(binding_form, binding_name, index);
            is_span_shadowed_by_binding(semantic, child, target_span, binding_name, child_shadowed)
        })
}

pub(super) fn is_path_shadowed_by_iteration_bindings(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    binding_form: &ExpressionView,
    target_path: &[ChildIndex],
    binding_name: &str,
    shadowed_by_binding: bool,
) -> bool {
    let Some((index, rest)) = target_path.split_first() else {
        return shadowed_by_binding;
    };
    let index = index.get();
    let Some(child) = binding_form.children.get(index) else {
        return false;
    };

    let child_shadowed =
        shadowed_by_binding || iteration_binding_child_shadowed(binding_form, binding_name, index);
    if rest.is_empty() {
        child_shadowed
    } else {
        is_path_shadowed_by_binding(semantic, child, rest, binding_name, child_shadowed)
    }
}
