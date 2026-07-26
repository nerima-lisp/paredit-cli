use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};

use crate::domain::sexpr::SymbolName;

use super::parameter::ReorderableParameter;

pub(in crate::domain::function_parameter) fn ensure_reorder_stays_within_parameter_groups(
    parameters: &[ReorderableParameter],
    new_relative_order: &[usize],
    command: &str,
) -> Result<()> {
    for (new_index, &old_index) in new_relative_order.iter().enumerate() {
        if parameters[new_index].group != parameters[old_index].group {
            anyhow::bail!(
                "{command} cannot move '{}' across Common Lisp lambda-list sections",
                parameters[old_index].name
            );
        }
    }
    Ok(())
}

pub(in crate::domain::function_parameter) fn build_new_relative_order(
    old_order: &[SymbolName],
    new_order: &[SymbolName],
) -> Result<Vec<usize>> {
    if new_order.len() != old_order.len() {
        anyhow::bail!(
            "reorder-function-parameters requested {} parameters but definition has {}",
            new_order.len(),
            old_order.len()
        );
    }

    let mut old_indexes = BTreeMap::new();
    for (index, name) in old_order.iter().enumerate() {
        if old_indexes.insert(name.as_str(), index).is_some() {
            anyhow::bail!(
                "reorder-function-parameters cannot reorder duplicate definition parameter '{name}'"
            );
        }
    }

    let mut requested_names = BTreeSet::new();
    let mut relative_order = Vec::with_capacity(new_order.len());
    for name in new_order {
        if !requested_names.insert(name.as_str()) {
            anyhow::bail!(
                "reorder-function-parameters requested parameter '{name}' more than once"
            );
        }
        let index = old_indexes.get(name.as_str()).copied().with_context(|| {
            format!("reorder-function-parameters requested unknown parameter '{name}'")
        })?;
        relative_order.push(index);
    }

    for name in old_order {
        if !requested_names.contains(name.as_str()) {
            anyhow::bail!(
                "reorder-function-parameters missing parameter '{name}' from requested order"
            );
        }
    }

    Ok(relative_order)
}

pub(in crate::domain::function_parameter) fn is_identity_order(order: &[usize]) -> bool {
    order.iter().copied().eq(0..order.len())
}
