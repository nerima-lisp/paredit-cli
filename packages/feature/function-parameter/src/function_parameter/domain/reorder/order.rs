use std::collections::{BTreeMap, BTreeSet};

use crate::error::{FunctionParameterResult, ParameterSelectionError};

use paredit_core_syntax::sexpr::SymbolName;

use super::parameter::ReorderableParameter;

pub fn ensure_reorder_stays_within_parameter_groups(
    parameters: &[ReorderableParameter],
    new_relative_order: &[usize],
    command: &'static str,
) -> FunctionParameterResult<()> {
    for (new_index, &old_index) in new_relative_order.iter().enumerate() {
        if parameters[new_index].group != parameters[old_index].group {
            return Err(ParameterSelectionError::CannotCrossSections {
                command,
                name: parameters[old_index].name.to_string(),
            }
            .into());
        }
    }
    Ok(())
}

pub fn build_new_relative_order(
    old_order: &[SymbolName],
    new_order: &[SymbolName],
) -> FunctionParameterResult<Vec<usize>> {
    if new_order.len() != old_order.len() {
        return Err(ParameterSelectionError::ReorderCountMismatch {
            requested: new_order.len(),
            actual: old_order.len(),
        }
        .into());
    }

    let mut old_indexes = BTreeMap::new();
    for (index, name) in old_order.iter().enumerate() {
        if old_indexes.insert(name.as_str(), index).is_some() {
            return Err(
                ParameterSelectionError::ReorderDuplicateDefinitionParameter {
                    name: name.to_string(),
                }
                .into(),
            );
        }
    }

    let mut requested_names = BTreeSet::new();
    let mut relative_order = Vec::with_capacity(new_order.len());
    for name in new_order {
        if !requested_names.insert(name.as_str()) {
            return Err(ParameterSelectionError::ReorderRequestedTwice {
                name: name.to_string(),
            }
            .into());
        }
        let index = old_indexes.get(name.as_str()).copied().ok_or_else(|| {
            ParameterSelectionError::ReorderUnknownParameter {
                name: name.to_string(),
            }
        })?;
        relative_order.push(index);
    }

    for name in old_order {
        if !requested_names.contains(name.as_str()) {
            return Err(ParameterSelectionError::ReorderMissingParameter {
                name: name.to_string(),
            }
            .into());
        }
    }

    Ok(relative_order)
}

pub fn is_identity_order(order: &[usize]) -> bool {
    order.iter().copied().eq(0..order.len())
}
