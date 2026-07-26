use anyhow::{Context, Result};

use crate::domain::dialect::Dialect;

use super::super::super::definition::{
    InlineDestructurePattern, InlineParameter, InlineParameterBinding,
};
use super::super::super::substitution::substitute_expression;
use super::super::destructure::destructure_argument_entries;
use super::super::types::ParameterBinding;

pub(super) fn bind_positional_parameter(
    dialect: Dialect,
    param: &InlineParameter,
    argument: Option<String>,
    default_scope: &[(String, String)],
    allow_drop_arguments: bool,
) -> Result<ParameterBinding> {
    if let Some(argument) = argument {
        return supplied_parameter_binding(dialect, param, argument, allow_drop_arguments);
    }

    missing_parameter_binding(dialect, param, default_scope, allow_drop_arguments)
}

pub(super) fn bind_keyword_parameter(
    dialect: Dialect,
    param: &InlineParameter,
    argument: Option<String>,
    default_scope: &[(String, String)],
    allow_drop_arguments: bool,
) -> Result<ParameterBinding> {
    if let Some(argument) = argument {
        return supplied_parameter_binding(dialect, param, argument, allow_drop_arguments);
    }

    missing_parameter_binding(dialect, param, default_scope, allow_drop_arguments)
}

pub(super) fn bind_aux_parameter(
    dialect: Dialect,
    param: &InlineParameter,
    default_scope: &[(String, String)],
) -> Result<ParameterBinding> {
    let name = simple_binding_name(param)?;
    let argument = resolve_default_value(dialect, param, default_scope)?;
    Ok(ParameterBinding {
        body_entries: vec![(name.clone(), argument.clone())],
        argument_entries: Vec::new(),
        default_scope_entries: vec![(name, argument)],
    })
}

fn supplied_parameter_binding(
    dialect: Dialect,
    param: &InlineParameter,
    argument: String,
    allow_drop_arguments: bool,
) -> Result<ParameterBinding> {
    let supplied_p = param
        .supplied_p
        .as_ref()
        .map(|name| (name.clone(), "t".to_owned()));
    let argument_entries =
        bound_parameter_argument_entries(dialect, param, argument, allow_drop_arguments)?;

    Ok(ParameterBinding {
        body_entries: supplied_p.clone().into_iter().collect(),
        argument_entries: argument_entries.clone(),
        default_scope_entries: supplied_parameter_default_scope_entries(
            param,
            &argument_entries,
            supplied_p,
        )?,
    })
}

fn missing_parameter_binding(
    dialect: Dialect,
    param: &InlineParameter,
    default_scope: &[(String, String)],
    allow_drop_arguments: bool,
) -> Result<ParameterBinding> {
    let argument = resolve_default_value(dialect, param, default_scope)?;
    let mut body_entries =
        bound_parameter_argument_entries(dialect, param, argument.clone(), allow_drop_arguments)?;
    let mut default_scope_entries = body_entries.clone();
    if let Some(supplied_p) = &param.supplied_p {
        let supplied_entry = (supplied_p.clone(), "nil".to_owned());
        body_entries.push(supplied_entry.clone());
        default_scope_entries.push(supplied_entry);
    }

    Ok(ParameterBinding {
        body_entries,
        argument_entries: Vec::new(),
        default_scope_entries,
    })
}

fn resolve_default_value(
    dialect: Dialect,
    param: &InlineParameter,
    default_scope: &[(String, String)],
) -> Result<String> {
    let Some(value) = param.default_value.as_ref() else {
        return Ok("nil".to_owned());
    };
    let (names, arguments): (Vec<_>, Vec<_>) = default_scope.iter().cloned().unzip();
    substitute_expression(dialect, value, &names, &arguments)
}

fn supplied_parameter_default_scope_entries(
    param: &InlineParameter,
    argument_entries: &[(String, String)],
    supplied_p: Option<(String, String)>,
) -> Result<Vec<(String, String)>> {
    let mut entries = match &param.binding {
        InlineParameterBinding::Name(name) => vec![(name.clone(), name.clone())],
        InlineParameterBinding::Destructure(_) => argument_entries.to_vec(),
    };
    if let Some(supplied_entry) = supplied_p {
        entries.push(supplied_entry);
    }
    Ok(entries)
}

pub(super) fn bound_parameter_argument_entries(
    dialect: Dialect,
    param: &InlineParameter,
    argument: String,
    allow_drop_arguments: bool,
) -> Result<Vec<(String, String)>> {
    match &param.binding {
        InlineParameterBinding::Name(name) => Ok(vec![(name.clone(), argument)]),
        InlineParameterBinding::Destructure(pattern) => {
            destructure_argument_entries(dialect, pattern, &argument, allow_drop_arguments)
        }
    }
}

fn simple_binding_name(param: &InlineParameter) -> Result<String> {
    param
        .primary_name()
        .map(ToOwned::to_owned)
        .context("inline-function internal error: expected simple parameter binding")
}

pub(super) fn destructured_binding_entries(
    dialect: Dialect,
    pattern: &InlineDestructurePattern,
    argument: String,
) -> Result<Vec<(String, String)>> {
    match pattern {
        InlineDestructurePattern::Name(name) => Ok(vec![(name.clone(), argument)]),
        InlineDestructurePattern::List(_) => {
            destructure_argument_entries(dialect, pattern, &argument, false)
        }
    }
}
