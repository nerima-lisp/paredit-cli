use crate::domain::common_lisp::{
    CommonLispLambdaListShape, CommonLispOperator, normalize_common_lisp_operator_head,
};
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

use super::DefinitionCategory;

pub(super) fn definition_lambda_list_child_index(
    view: &ExpressionView,
    head: &str,
) -> Option<usize> {
    if let Some(shape) = CommonLispOperator::from_head(head)
        .and_then(CommonLispOperator::definition_lambda_list_shape)
    {
        return common_lisp_lambda_list_child_index(view, shape);
    }

    let normalized = normalize_common_lisp_operator_head(head);

    match normalized {
        "cl-defun" | "defsubst" | "definline" | "cl-defmacro" | "define" | "lambda" | "defn"
        | "defn-" => list_child_index(view, 2),
        "cl-defmethod" => common_lisp_lambda_list_child_index(
            view,
            CommonLispLambdaListShape::FirstListAtOrAfter(2),
        ),
        "cl-defgeneric" => list_child_index(view, 2),
        "deftest" | "ert-deftest" | "define-test" | "define-ert-test" => list_child_index(view, 2),
        _ => None,
    }
}

pub(super) fn definition_body_start_child_index(
    view: &ExpressionView,
    head: &str,
    category: Option<DefinitionCategory>,
    lambda_list_index: Option<usize>,
) -> usize {
    if matches!(
        CommonLispOperator::from_head(head),
        Some(CommonLispOperator::Defsetf)
    ) {
        return match lambda_list_index {
            Some(index) => (index + 2).min(view.children.len()),
            None => view.children.len(),
        };
    }

    match (category, lambda_list_index) {
        (Some(category), Some(index)) if category.is_callable() => index + 1,
        (Some(category), None) if category.is_callable() => 3,
        (Some(_), _) => 2,
        (None, _) => 0,
    }
}

pub(super) fn definition_lambda_parameter_count(lambda_list: &ExpressionView) -> usize {
    lambda_list
        .children
        .iter()
        .filter(|child| match child.kind {
            ExpressionKind::Atom => child
                .text
                .as_deref()
                .is_some_and(|text| !text.starts_with('&')),
            ExpressionKind::List => true,
            ExpressionKind::Root => false,
        })
        .count()
}

#[derive(Clone, Copy)]
enum ArityMode {
    /// Required parameters, and the (non-consuming) &whole/&environment/&aux
    /// tail: neither corresponds to a visible call argument slot, so
    /// counting them as "required" would demand arguments a caller can
    /// never actually supply.
    Required,
    Optional,
    Key,
    NonConsuming,
}

fn arity_mode_for_marker(text: &str) -> Option<(ArityMode, bool)> {
    match text {
        "&optional" => Some((ArityMode::Optional, false)),
        "&key" => Some((ArityMode::Key, false)),
        "&rest" | "&body" => Some((ArityMode::NonConsuming, true)),
        "&aux" | "&whole" | "&environment" => Some((ArityMode::NonConsuming, false)),
        "&allow-other-keys" => None,
        _ => None,
    }
}

/// Return the (minimum, maximum) call-argument arity a lambda list accepts;
/// MAXIMUM is `None` when unbounded (`&rest`/`&body` present).
///
/// Unlike DEFINITION_LAMBDA_PARAMETER_COUNT, this distinguishes required
/// parameters (which every call must supply) from &optional/&key ones
/// (which a call may omit) and &rest/&body/&aux/&whole/&environment (which
/// don't consume a fixed number of call-argument slots at all): a flat
/// count conflates all of these, so any call that omits an optional/keyword
/// argument — the overwhelmingly common case — reads as a real arity
/// mismatch.
pub(super) fn definition_lambda_parameter_arity(
    lambda_list: &ExpressionView,
) -> (usize, Option<usize>) {
    let mut mode = ArityMode::Required;
    let mut required = 0usize;
    let mut optional = 0usize;
    let mut key_count = 0usize;
    let mut unbounded = false;

    for child in &lambda_list.children {
        if let ExpressionKind::Atom = child.kind {
            if let Some(marker) = child
                .text
                .as_deref()
                .and_then(|text| text.strip_prefix('&').map(|_| text))
                .and_then(arity_mode_for_marker)
            {
                let (next_mode, marks_unbounded) = marker;
                mode = next_mode;
                unbounded |= marks_unbounded;
                continue;
            }
        }
        match mode {
            ArityMode::Required => required += 1,
            ArityMode::Optional => optional += 1,
            ArityMode::Key => key_count += 1,
            ArityMode::NonConsuming => {}
        }
    }

    if unbounded {
        (required, None)
    } else {
        (required, Some(required + optional + key_count * 2))
    }
}

fn common_lisp_lambda_list_child_index(
    view: &ExpressionView,
    shape: CommonLispLambdaListShape,
) -> Option<usize> {
    match shape {
        CommonLispLambdaListShape::ChildAt(index) => list_child_index(view, index),
        CommonLispLambdaListShape::FirstListAtOrAfter(start_index) => {
            (start_index..view.children.len()).find(|&index| {
                matches!(
                    view.children[index].delimiter,
                    Some(Delimiter::Paren | Delimiter::Bracket)
                )
            })
        }
    }
}

fn list_child_index(view: &ExpressionView, index: usize) -> Option<usize> {
    view.children
        .get(index)
        .and_then(|child| (child.kind == ExpressionKind::List).then_some(index))
}
