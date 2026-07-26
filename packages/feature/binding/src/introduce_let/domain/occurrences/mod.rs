mod iteration;
mod let_star;
mod local_callable;
mod variable;

use crate::domain::{
    dialect::{IntroduceLetOperation, VerifiedSemanticPolicy},
    sexpr::{ByteOffset, ByteSpan, ChildIndex, ExpressionView},
};

use super::syntax::{
    child_shadowed_by_binding, iteration_bindings_child_index, let_star_bindings_child_index,
    local_callable_bindings_child_index, variable_binding_form_has_step_forms,
    variable_binding_form_is_sequential, variable_bindings_child_index,
};
use iteration::{
    collect_iteration_binding_spans, is_path_shadowed_by_iteration_bindings,
    is_span_shadowed_by_iteration_bindings,
};
use let_star::{
    collect_let_star_binding_spans, is_path_shadowed_by_let_star_bindings,
    is_span_shadowed_by_let_star_bindings,
};
use local_callable::{
    collect_local_callable_binding_spans, is_path_shadowed_by_local_callable_binding,
    is_span_shadowed_by_local_callable_binding,
};
use variable::{
    VariableBindingContext, collect_variable_binding_spans, is_path_shadowed_by_variable_bindings,
    is_span_shadowed_by_variable_bindings,
};

#[derive(Debug, Default)]
pub(super) struct EquivalentExpressionSpans {
    pub(super) replacement_spans: Vec<ByteSpan>,
    pub(super) skipped_shadowed_spans: Vec<ByteSpan>,
}

pub(super) fn collect_equivalent_expression_spans(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    view: &ExpressionView,
    target: &ExpressionView,
    binding_name: &str,
    shadowed_by_binding: bool,
    output: &mut EquivalentExpressionSpans,
) {
    let dialect = semantic.dialect();
    if expressions_equivalent(view, target) {
        record_equivalent_span(output, view.span, shadowed_by_binding);
        return;
    }

    for (index, child) in view.children.iter().enumerate() {
        if let_star_bindings_child_index(dialect, view) == Some(index) {
            collect_let_star_binding_spans(
                semantic,
                child,
                target,
                binding_name,
                shadowed_by_binding,
                output,
            );
            continue;
        }

        if iteration_bindings_child_index(dialect, view) == Some(index) {
            collect_iteration_binding_spans(
                semantic,
                child,
                target,
                binding_name,
                shadowed_by_binding,
                output,
            );
            continue;
        }

        if variable_bindings_child_index(dialect, view) == Some(index) {
            let mut ctx = VariableBindingContext {
                semantic,
                target,
                binding_name,
                has_step_forms: variable_binding_form_has_step_forms(dialect, view),
                output,
            };
            collect_variable_binding_spans(
                &mut ctx,
                child,
                shadowed_by_binding,
                variable_binding_form_is_sequential(dialect, view),
            );
            continue;
        }

        if local_callable_bindings_child_index(dialect, view) == Some(index) {
            collect_local_callable_binding_spans(
                semantic,
                child,
                target,
                binding_name,
                shadowed_by_binding,
                output,
            );
            continue;
        }

        let child_shadowed =
            shadowed_by_binding || child_shadowed_by_binding(semantic, view, binding_name, index);
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

pub(super) fn is_span_shadowed_by_binding(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    view: &ExpressionView,
    target_span: ByteSpan,
    binding_name: &str,
    shadowed_by_binding: bool,
) -> bool {
    let dialect = semantic.dialect();
    if view.span == target_span {
        return shadowed_by_binding;
    }

    view.children.iter().enumerate().any(|(index, child)| {
        if let_star_bindings_child_index(dialect, view) == Some(index) {
            return is_span_shadowed_by_let_star_bindings(
                semantic,
                child,
                target_span,
                binding_name,
                shadowed_by_binding,
            );
        }

        if iteration_bindings_child_index(dialect, view) == Some(index) {
            return is_span_shadowed_by_iteration_bindings(
                semantic,
                child,
                target_span,
                binding_name,
                shadowed_by_binding,
            );
        }

        if variable_bindings_child_index(dialect, view) == Some(index) {
            return is_span_shadowed_by_variable_bindings(
                semantic,
                child,
                target_span,
                binding_name,
                shadowed_by_binding,
                variable_binding_form_is_sequential(dialect, view),
                variable_binding_form_has_step_forms(dialect, view),
            );
        }

        if local_callable_bindings_child_index(dialect, view) == Some(index) {
            return is_span_shadowed_by_local_callable_binding(
                semantic,
                child,
                target_span,
                binding_name,
                shadowed_by_binding,
            );
        }

        let child_shadowed =
            shadowed_by_binding || child_shadowed_by_binding(semantic, view, binding_name, index);
        is_span_shadowed_by_binding(semantic, child, target_span, binding_name, child_shadowed)
    })
}

pub(super) fn is_path_shadowed_by_binding(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    view: &ExpressionView,
    target_path: &[ChildIndex],
    binding_name: &str,
    shadowed_by_binding: bool,
) -> bool {
    let dialect = semantic.dialect();
    let Some((index, rest)) = target_path.split_first() else {
        return shadowed_by_binding;
    };
    let index = index.get();
    let Some(child) = view.children.get(index) else {
        return false;
    };

    if let_star_bindings_child_index(dialect, view) == Some(index) {
        return is_path_shadowed_by_let_star_bindings(
            semantic,
            child,
            rest,
            binding_name,
            shadowed_by_binding,
        );
    }

    if iteration_bindings_child_index(dialect, view) == Some(index) {
        return is_path_shadowed_by_iteration_bindings(
            semantic,
            child,
            rest,
            binding_name,
            shadowed_by_binding,
        );
    }

    if variable_bindings_child_index(dialect, view) == Some(index) {
        return is_path_shadowed_by_variable_bindings(
            semantic,
            child,
            rest,
            binding_name,
            shadowed_by_binding,
            variable_binding_form_is_sequential(dialect, view),
            variable_binding_form_has_step_forms(dialect, view),
        );
    }

    if local_callable_bindings_child_index(dialect, view) == Some(index) {
        return is_path_shadowed_by_local_callable_binding(
            semantic,
            child,
            rest,
            binding_name,
            shadowed_by_binding,
        );
    }

    let child_shadowed =
        shadowed_by_binding || child_shadowed_by_binding(semantic, view, binding_name, index);
    if rest.is_empty() {
        child_shadowed
    } else {
        is_path_shadowed_by_binding(semantic, child, rest, binding_name, child_shadowed)
    }
}

pub(super) fn rebase_spans(mut spans: Vec<ByteSpan>, base: ByteOffset) -> Vec<ByteSpan> {
    spans.sort_by_key(|span| span.start());
    spans.dedup();
    spans
        .into_iter()
        .map(|span| {
            ByteSpan::new(
                ByteOffset::new(span.start().get() + base.get()),
                ByteOffset::new(span.end().get() + base.get()),
            )
        })
        .collect()
}

fn record_equivalent_span(
    output: &mut EquivalentExpressionSpans,
    span: ByteSpan,
    shadowed_by_binding: bool,
) {
    if shadowed_by_binding {
        output.skipped_shadowed_spans.push(span);
    } else {
        output.replacement_spans.push(span);
    }
}

fn expressions_equivalent(left: &ExpressionView, right: &ExpressionView) -> bool {
    left.kind == right.kind
        && left.delimiter == right.delimiter
        && left.text == right.text
        && left.children.len() == right.children.len()
        && left
            .children
            .iter()
            .zip(&right.children)
            .all(|(left, right)| expressions_equivalent(left, right))
}
