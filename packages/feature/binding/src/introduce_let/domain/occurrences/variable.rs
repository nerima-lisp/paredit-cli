use paredit_core_syntax::dialect::{IntroduceLetOperation, VerifiedSemanticPolicy};
use paredit_core_syntax::sexpr::{ByteSpan, ChildIndex, ExpressionView};

use super::{
    EquivalentExpressionSpans, collect_equivalent_expression_spans, expressions_equivalent,
    is_path_shadowed_by_binding, is_span_shadowed_by_binding, record_equivalent_span,
};
use crate::introduce_let::domain::syntax::variable_spec_binds_name;

pub struct VariableBindingContext<'a> {
    pub semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    pub target: &'a ExpressionView,
    pub binding_name: &'a str,
    pub has_step_forms: bool,
    pub output: &'a mut EquivalentExpressionSpans,
}

pub fn collect_variable_binding_spans(
    ctx: &mut VariableBindingContext<'_>,
    binding_form: &ExpressionView,
    shadowed_by_binding: bool,
    sequential: bool,
) {
    if expressions_equivalent(binding_form, ctx.target) {
        record_equivalent_span(ctx.output, binding_form.span, shadowed_by_binding);
        return;
    }

    let all_bindings_shadowed = shadowed_by_binding
        || binding_form
            .children
            .iter()
            .any(|binding| variable_spec_binds_name(binding, ctx.binding_name));
    let mut sequential_shadowed = shadowed_by_binding;

    for binding in &binding_form.children {
        collect_variable_binding_spec_spans(
            ctx,
            binding,
            sequential_shadowed,
            all_bindings_shadowed,
        );
        if sequential && variable_spec_binds_name(binding, ctx.binding_name) {
            sequential_shadowed = true;
        }
    }
}

fn collect_variable_binding_spec_spans(
    ctx: &mut VariableBindingContext<'_>,
    binding: &ExpressionView,
    init_shadowed: bool,
    step_shadowed: bool,
) {
    if expressions_equivalent(binding, ctx.target) {
        record_equivalent_span(ctx.output, binding.span, init_shadowed);
        return;
    }

    for (index, child) in binding.children.iter().enumerate() {
        let child_shadowed = variable_binding_spec_child_shadowed(
            index,
            init_shadowed,
            step_shadowed,
            ctx.has_step_forms,
        );
        let semantic = ctx.semantic;
        let target = ctx.target;
        let binding_name = ctx.binding_name;
        let output = &mut *ctx.output;
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

pub fn is_span_shadowed_by_variable_bindings(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    binding_form: &ExpressionView,
    target_span: ByteSpan,
    binding_name: &str,
    shadowed_by_binding: bool,
    sequential: bool,
    has_step_forms: bool,
) -> bool {
    if binding_form.span == target_span {
        return shadowed_by_binding;
    }

    let all_bindings_shadowed = shadowed_by_binding
        || binding_form
            .children
            .iter()
            .any(|binding| variable_spec_binds_name(binding, binding_name));
    let mut sequential_shadowed = shadowed_by_binding;

    for binding in &binding_form.children {
        if binding.span.contains_span(target_span) {
            return is_span_shadowed_by_variable_binding_spec(
                semantic,
                binding,
                target_span,
                binding_name,
                sequential_shadowed,
                all_bindings_shadowed,
                has_step_forms,
            );
        }
        if sequential && variable_spec_binds_name(binding, binding_name) {
            sequential_shadowed = true;
        }
    }

    false
}

fn is_span_shadowed_by_variable_binding_spec(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    binding: &ExpressionView,
    target_span: ByteSpan,
    binding_name: &str,
    init_shadowed: bool,
    step_shadowed: bool,
    has_step_forms: bool,
) -> bool {
    if binding.span == target_span {
        return init_shadowed;
    }

    binding.children.iter().enumerate().any(|(index, child)| {
        let child_shadowed = variable_binding_spec_child_shadowed(
            index,
            init_shadowed,
            step_shadowed,
            has_step_forms,
        );
        is_span_shadowed_by_binding(semantic, child, target_span, binding_name, child_shadowed)
    })
}

pub fn is_path_shadowed_by_variable_bindings(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    binding_form: &ExpressionView,
    target_path: &[ChildIndex],
    binding_name: &str,
    shadowed_by_binding: bool,
    sequential: bool,
    has_step_forms: bool,
) -> bool {
    let Some((index, rest)) = target_path.split_first() else {
        return shadowed_by_binding;
    };
    let index = index.get();

    let all_bindings_shadowed = shadowed_by_binding
        || binding_form
            .children
            .iter()
            .any(|binding| variable_spec_binds_name(binding, binding_name));
    let mut sequential_shadowed = shadowed_by_binding;

    for (binding_index, binding) in binding_form.children.iter().enumerate() {
        if binding_index == index {
            return if rest.is_empty() {
                sequential_shadowed
            } else {
                is_path_shadowed_by_variable_binding_spec(
                    semantic,
                    binding,
                    rest,
                    binding_name,
                    sequential_shadowed,
                    all_bindings_shadowed,
                    has_step_forms,
                )
            };
        }
        if sequential && variable_spec_binds_name(binding, binding_name) {
            sequential_shadowed = true;
        }
    }

    false
}

fn is_path_shadowed_by_variable_binding_spec(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    binding: &ExpressionView,
    target_path: &[ChildIndex],
    binding_name: &str,
    init_shadowed: bool,
    step_shadowed: bool,
    has_step_forms: bool,
) -> bool {
    let Some((index, rest)) = target_path.split_first() else {
        return init_shadowed;
    };
    let index = index.get();
    let Some(child) = binding.children.get(index) else {
        return false;
    };

    let child_shadowed =
        variable_binding_spec_child_shadowed(index, init_shadowed, step_shadowed, has_step_forms);
    if rest.is_empty() {
        child_shadowed
    } else {
        is_path_shadowed_by_binding(semantic, child, rest, binding_name, child_shadowed)
    }
}

const fn variable_binding_spec_child_shadowed(
    child_index: usize,
    init_shadowed: bool,
    step_shadowed: bool,
    has_step_forms: bool,
) -> bool {
    match child_index {
        0 | 1 => init_shadowed,
        _ if has_step_forms => step_shadowed,
        _ => init_shadowed,
    }
}
