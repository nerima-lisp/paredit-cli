use std::collections::BTreeMap;

use anyhow::Result;
use expansion::{
    count_references_in_expanded_expression, expand_unquote_expression, expand_unquote_splicing,
};

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionKind, ExpressionView, ReaderPrefix};

use super::InlineFunctionParameterPlan;
use super::substitution::substitute_inline_function_body;

mod expansion;
mod literal_render;

pub(super) fn expand_inline_macro_body(
    dialect: Dialect,
    input: &str,
    body: &ExpressionView,
    body_bindings: &[(String, String)],
    argument_bindings: &[(String, String)],
    allow_duplicate_evaluation: bool,
    allow_drop_arguments: bool,
) -> Result<(String, Vec<InlineFunctionParameterPlan>)> {
    let mut reference_counts = BTreeMap::new();
    for (name, _) in argument_bindings {
        reference_counts.insert(name.clone(), 0usize);
    }

    let replacement = if body.reader_prefixes.first() == Some(&ReaderPrefix::Quasiquote) {
        render_macro_template(
            dialect,
            input,
            body,
            0,
            body_bindings,
            argument_bindings,
            &mut reference_counts,
        )?
    } else {
        expand_plain_macro_body(
            dialect,
            input,
            body,
            body_bindings,
            argument_bindings,
            &mut reference_counts,
        )?
    };

    let mut parameters = Vec::with_capacity(argument_bindings.len());
    for (name, argument) in argument_bindings {
        let reference_count = reference_counts.remove(name).unwrap_or_default();
        if reference_count == 0 && !allow_drop_arguments {
            anyhow::bail!(
                "inline-function would drop argument '{argument}' for unused parameter '{name}'; pass --allow-drop-arguments to permit it"
            );
        }
        if reference_count > 1 && !allow_duplicate_evaluation {
            anyhow::bail!(
                "inline-function would duplicate argument '{argument}' for parameter '{name}'; pass --allow-duplicate-evaluation to permit it"
            );
        }
        parameters.push(InlineFunctionParameterPlan {
            name: name.clone(),
            argument: argument.clone(),
            reference_count,
        });
    }

    Ok((replacement, parameters))
}

fn expand_plain_macro_body(
    dialect: Dialect,
    input: &str,
    body: &ExpressionView,
    body_bindings: &[(String, String)],
    argument_bindings: &[(String, String)],
    reference_counts: &mut BTreeMap<String, usize>,
) -> Result<String> {
    let (intermediate, _) = substitute_inline_function_body(
        dialect,
        input,
        body,
        &body_bindings
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>(),
        &body_bindings
            .iter()
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>(),
        true,
        true,
    )?;
    count_references_in_expanded_expression(dialect, &intermediate, reference_counts)?;

    let intermediate_tree = expansion::parse_single_expression_tree(dialect, &intermediate)?;
    let intermediate_expression = intermediate_tree
        .select_path(&crate::domain::sexpr::Path::root_child(0))?
        .view();
    let (expanded, _) = substitute_inline_function_body(
        dialect,
        &intermediate,
        &intermediate_expression,
        &argument_bindings
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>(),
        &argument_bindings
            .iter()
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>(),
        true,
        true,
    )?;
    Ok(expanded)
}

fn render_macro_template(
    dialect: Dialect,
    input: &str,
    view: &ExpressionView,
    quasiquote_depth: usize,
    body_bindings: &[(String, String)],
    argument_bindings: &[(String, String)],
    reference_counts: &mut BTreeMap<String, usize>,
) -> Result<String> {
    render_prefixed_expression(
        dialect,
        input,
        view,
        0,
        quasiquote_depth,
        body_bindings,
        argument_bindings,
        reference_counts,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "macro template rendering threads the full expansion context plus dialect"
)]
fn render_prefixed_expression(
    dialect: Dialect,
    input: &str,
    view: &ExpressionView,
    prefix_index: usize,
    quasiquote_depth: usize,
    body_bindings: &[(String, String)],
    argument_bindings: &[(String, String)],
    reference_counts: &mut BTreeMap<String, usize>,
) -> Result<String> {
    let Some(prefix) = view.reader_prefixes.get(prefix_index).copied() else {
        return render_core_expression(
            dialect,
            input,
            view,
            quasiquote_depth,
            body_bindings,
            argument_bindings,
            reference_counts,
        );
    };

    match prefix {
        ReaderPrefix::Quasiquote => {
            let rendered = render_prefixed_expression(
                dialect,
                input,
                view,
                prefix_index + 1,
                quasiquote_depth + 1,
                body_bindings,
                argument_bindings,
                reference_counts,
            )?;
            if quasiquote_depth == 0 {
                Ok(rendered)
            } else {
                Ok(format!("`{rendered}"))
            }
        }
        ReaderPrefix::Unquote => {
            if quasiquote_depth == 1 {
                expand_unquote_expression(
                    dialect,
                    view,
                    body_bindings,
                    argument_bindings,
                    reference_counts,
                )
            } else {
                let rendered = render_prefixed_expression(
                    dialect,
                    input,
                    view,
                    prefix_index + 1,
                    quasiquote_depth.saturating_sub(1),
                    body_bindings,
                    argument_bindings,
                    reference_counts,
                )?;
                Ok(format!(",{rendered}"))
            }
        }
        ReaderPrefix::UnquoteSplicing => {
            if quasiquote_depth == 1 {
                anyhow::bail!("inline-function found unsupported top-level ,@expr in defmacro body")
            } else {
                let rendered = render_prefixed_expression(
                    dialect,
                    input,
                    view,
                    prefix_index + 1,
                    quasiquote_depth.saturating_sub(1),
                    body_bindings,
                    argument_bindings,
                    reference_counts,
                )?;
                Ok(format!(",@{rendered}"))
            }
        }
        ReaderPrefix::Quote => Ok(format!(
            "'{}",
            render_prefixed_expression(
                dialect,
                input,
                view,
                prefix_index + 1,
                quasiquote_depth,
                body_bindings,
                argument_bindings,
                reference_counts,
            )?
        )),
        ReaderPrefix::Function => Ok(format!(
            "#'{}",
            render_prefixed_expression(
                dialect,
                input,
                view,
                prefix_index + 1,
                quasiquote_depth,
                body_bindings,
                argument_bindings,
                reference_counts,
            )?
        )),
        ReaderPrefix::ReadEval => Ok(view.span.slice(input).to_owned()),
        ReaderPrefix::HashLiteral
        | ReaderPrefix::Metadata
        | ReaderPrefix::ReaderConditional
        | ReaderPrefix::ReaderConditionalSplicing => Ok(format!(
            "{}{}",
            prefix.as_source(),
            render_prefixed_expression(
                dialect,
                input,
                view,
                prefix_index + 1,
                quasiquote_depth,
                body_bindings,
                argument_bindings,
                reference_counts,
            )?
        )),
    }
}

fn render_core_expression(
    dialect: Dialect,
    input: &str,
    view: &ExpressionView,
    quasiquote_depth: usize,
    body_bindings: &[(String, String)],
    argument_bindings: &[(String, String)],
    reference_counts: &mut BTreeMap<String, usize>,
) -> Result<String> {
    match view.kind {
        ExpressionKind::Atom => Ok(view
            .text
            .clone()
            .ok_or_else(|| anyhow::anyhow!("inline-function expected atom text in macro body"))?),
        ExpressionKind::List => {
            let delimiter = view.delimiter.ok_or_else(|| {
                anyhow::anyhow!("inline-function expected delimited list in macro body")
            })?;
            let mut rendered_children = Vec::with_capacity(view.children.len());
            for child in &view.children {
                if quasiquote_depth == 1
                    && child.reader_prefixes.first() == Some(&ReaderPrefix::UnquoteSplicing)
                {
                    rendered_children.extend(expand_unquote_splicing(
                        dialect,
                        child,
                        body_bindings,
                        argument_bindings,
                        reference_counts,
                    )?);
                    continue;
                }
                rendered_children.push(render_macro_template(
                    dialect,
                    input,
                    child,
                    quasiquote_depth,
                    body_bindings,
                    argument_bindings,
                    reference_counts,
                )?);
            }
            Ok(literal_render::render_list(delimiter, rendered_children))
        }
        ExpressionKind::Root => {
            anyhow::bail!("inline-function macro body must be a single expression")
        }
    }
}
