use std::collections::BTreeMap;

use anyhow::{Context, Result};

use crate::domain::dialect::Dialect;
use crate::domain::lexical_scope::collect_unshadowed_symbol_references;
use crate::domain::sexpr::{ExpressionKind, ExpressionView, SymbolName};

use super::literal_render::{render_literal_expression, render_unquoted_source};
use super::substitute_inline_function_body;

pub(super) fn expand_unquote_expression(
    dialect: Dialect,
    view: &ExpressionView,
    body_bindings: &[(String, String)],
    argument_bindings: &[(String, String)],
    reference_counts: &mut BTreeMap<String, usize>,
) -> Result<String> {
    let literal_source = render_unquoted_source(view)?;
    let literal_tree =
        crate::domain::sexpr::SyntaxTree::parse_with_dialect(&literal_source, dialect)
            .context("invalid unquote form")?;
    let expression = literal_tree
        .select_path(&crate::domain::sexpr::Path::root_child(0))?
        .view();
    let (intermediate, _) = substitute_inline_function_body(
        dialect,
        &literal_source,
        &expression,
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

    let intermediate_tree = parse_single_expression_tree(dialect, &intermediate)?;
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

pub(super) fn count_references_in_expanded_expression(
    dialect: Dialect,
    source: &str,
    reference_counts: &mut BTreeMap<String, usize>,
) -> Result<()> {
    let expression_tree = parse_single_expression_tree(dialect, source)?;
    let expression = expression_tree
        .select_path(&crate::domain::sexpr::Path::root_child(0))?
        .view();

    for (name, count) in reference_counts.iter_mut() {
        let symbol = SymbolName::new(name.clone())?;
        let mut spans = Vec::new();
        collect_unshadowed_symbol_references(dialect, &expression, &symbol, source, &mut spans);
        *count += spans.len();
    }

    Ok(())
}

pub(super) fn parse_single_expression_tree(
    dialect: Dialect,
    source: &str,
) -> Result<crate::domain::sexpr::SyntaxTree> {
    Ok(crate::domain::sexpr::SyntaxTree::parse_with_dialect(
        source, dialect,
    )?)
}

pub(super) fn expand_unquote_splicing(
    dialect: Dialect,
    view: &ExpressionView,
    body_bindings: &[(String, String)],
    argument_bindings: &[(String, String)],
    reference_counts: &mut BTreeMap<String, usize>,
) -> Result<Vec<String>> {
    let expanded = expand_unquote_expression(
        dialect,
        view,
        body_bindings,
        argument_bindings,
        reference_counts,
    )?;
    let expanded_tree = crate::domain::sexpr::SyntaxTree::parse_with_dialect(&expanded, dialect)
        .context("invalid ,@ expansion")?;
    let expression = expanded_tree
        .select_path(&crate::domain::sexpr::Path::root_child(0))?
        .view();
    if expression.kind != ExpressionKind::List {
        anyhow::bail!("inline-function requires ,@ expansions to produce a list form");
    }
    Ok(expression
        .children
        .iter()
        .map(render_literal_expression)
        .collect())
}
