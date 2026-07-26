use crate::domain::{
    dialect::{IntroduceLetOperation, VerifiedSemanticPolicy},
    sexpr::{ByteSpan, ChildIndex, ExpressionView},
};

use super::{
    EquivalentExpressionSpans, collect_equivalent_expression_spans, expressions_equivalent,
    is_path_shadowed_by_binding, is_span_shadowed_by_binding, record_equivalent_span,
};
use crate::domain::introduce_let::syntax::binding_pair_binds_name;

pub(super) fn collect_let_star_binding_spans(
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

    let mut sequential_shadowed = shadowed_by_binding;
    for binding in &binding_form.children {
        collect_let_star_binding_spec_spans(
            semantic,
            binding,
            target,
            binding_name,
            sequential_shadowed,
            output,
        );
        if binding_pair_binds_name(binding, binding_name) {
            sequential_shadowed = true;
        }
    }
}

fn collect_let_star_binding_spec_spans(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    binding: &ExpressionView,
    target: &ExpressionView,
    binding_name: &str,
    shadowed_by_binding: bool,
    output: &mut EquivalentExpressionSpans,
) {
    if expressions_equivalent(binding, target) {
        record_equivalent_span(output, binding.span, shadowed_by_binding);
        return;
    }

    for child in &binding.children {
        collect_equivalent_expression_spans(
            semantic,
            child,
            target,
            binding_name,
            shadowed_by_binding,
            output,
        );
    }
}

pub(super) fn is_span_shadowed_by_let_star_bindings(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    binding_form: &ExpressionView,
    target_span: ByteSpan,
    binding_name: &str,
    shadowed_by_binding: bool,
) -> bool {
    if binding_form.span == target_span {
        return shadowed_by_binding;
    }

    let mut sequential_shadowed = shadowed_by_binding;
    for binding in &binding_form.children {
        if binding.span.contains_span(target_span) {
            return is_span_shadowed_by_binding(
                semantic,
                binding,
                target_span,
                binding_name,
                sequential_shadowed,
            );
        }
        if binding_pair_binds_name(binding, binding_name) {
            sequential_shadowed = true;
        }
    }

    false
}

pub(super) fn is_path_shadowed_by_let_star_bindings(
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

    let mut sequential_shadowed = shadowed_by_binding;
    for (binding_index, binding) in binding_form.children.iter().enumerate() {
        if binding_index == index {
            return if rest.is_empty() {
                sequential_shadowed
            } else {
                is_path_shadowed_by_binding(
                    semantic,
                    binding,
                    rest,
                    binding_name,
                    sequential_shadowed,
                )
            };
        }
        if binding_pair_binds_name(binding, binding_name) {
            sequential_shadowed = true;
        }
    }

    false
}
