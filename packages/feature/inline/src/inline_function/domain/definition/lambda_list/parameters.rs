use anyhow::{Context, Result};

use crate::domain::sexpr::ExpressionView;

use super::super::super::syntax::atom_text;
use super::super::destructure::parse_macro_destructure_pattern;
use super::super::types::{
    InlineDefinitionKind, InlineParameter, InlineParameterBinding, InlineParameterKind,
};

pub(super) fn rest_parameter_name(child: &ExpressionView) -> Result<&str> {
    atom_text(child)
        .context("inline-function currently supports only simple symbol &rest parameters")
}

pub(super) fn whole_parameter_name(child: &ExpressionView) -> Result<&str> {
    atom_text(child)
        .context("inline-function currently supports only simple symbol &whole parameters")
}

pub(super) fn environment_parameter_name(child: &ExpressionView) -> Result<&str> {
    atom_text(child)
        .context("inline-function currently supports only simple symbol &environment parameters")
}

pub(super) fn optional_parameter(
    input: &str,
    definition_kind: InlineDefinitionKind,
    child: &ExpressionView,
) -> Result<InlineParameter> {
    Ok(InlineParameter {
        binding: optional_parameter_binding(input, definition_kind, child)?,
        kind: InlineParameterKind::Positional { optional: true },
        default_value: optional_parameter_default_value(input, child),
        supplied_p: optional_parameter_supplied_p(child)?,
    })
}

fn optional_parameter_binding(
    input: &str,
    definition_kind: InlineDefinitionKind,
    child: &ExpressionView,
) -> Result<InlineParameterBinding> {
    if let Some(name) = atom_text(child) {
        return Ok(InlineParameterBinding::Name(name.to_owned()));
    }

    let binding = child.children.first().context(
        "inline-function currently supports only simple or destructuring &optional parameter specifications",
    )?;
    if let Some(name) = atom_text(binding) {
        return Ok(InlineParameterBinding::Name(name.to_owned()));
    }

    if definition_kind != InlineDefinitionKind::Macro {
        anyhow::bail!("inline-function currently supports only simple symbol parameters");
    }

    Ok(InlineParameterBinding::Destructure(
        parse_macro_destructure_pattern(input, binding)?,
    ))
}

fn optional_parameter_supplied_p(child: &ExpressionView) -> Result<Option<String>> {
    match child.children.len() {
        0..=2 => Ok(None),
        3 => Ok(Some(
            atom_text(&child.children[2])
                .context(
                    "inline-function currently supports only atom supplied-p names in &optional parameter specifications",
                )?
                .to_owned(),
        )),
        _ => anyhow::bail!(
            "inline-function currently supports only simple or destructuring &optional parameter specifications"
        ),
    }
}

fn optional_parameter_default_value(input: &str, child: &ExpressionView) -> Option<String> {
    child
        .children
        .get(1)
        .map(|default| default.span.slice(input).to_owned())
}

pub(super) fn keyword_parameter(
    input: &str,
    definition_kind: InlineDefinitionKind,
    child: &ExpressionView,
) -> Result<InlineParameter> {
    if let Some(name) = atom_text(child) {
        if name.starts_with(':') {
            anyhow::bail!("inline-function requires a binding name for &key parameter {name}");
        }
        return Ok(InlineParameter {
            binding: InlineParameterBinding::Name(name.to_owned()),
            kind: InlineParameterKind::Keyword {
                keyword: format!(":{name}"),
            },
            default_value: None,
            supplied_p: None,
        });
    }

    let binding = child
        .children
        .first()
        .context("inline-function currently supports only simple &key parameter specifications")?;
    if let Some(name) = atom_text(binding) {
        if name.starts_with(':') {
            anyhow::bail!("inline-function requires a binding name for &key parameter {name}");
        }
        return Ok(InlineParameter {
            binding: InlineParameterBinding::Name(name.to_owned()),
            kind: InlineParameterKind::Keyword {
                keyword: format!(":{name}"),
            },
            default_value: keyword_parameter_default_value(input, child),
            supplied_p: keyword_parameter_supplied_p(child)?,
        });
    }

    let [external, internal] = binding.children.as_slice() else {
        anyhow::bail!("inline-function currently supports only (:keyword name) &key bindings");
    };
    let external = atom_text(external)
        .context("inline-function currently supports only atom &key external names")?;
    if !external.starts_with(':') {
        anyhow::bail!("inline-function &key external name must be a keyword: {external}");
    }
    let internal_binding = if let Some(internal) = atom_text(internal) {
        if internal.starts_with(':') {
            anyhow::bail!(
                "inline-function &key internal binding must not be a keyword: {internal}"
            );
        }
        InlineParameterBinding::Name(internal.to_owned())
    } else if definition_kind == InlineDefinitionKind::Macro {
        InlineParameterBinding::Destructure(parse_macro_destructure_pattern(input, internal)?)
    } else {
        anyhow::bail!("inline-function currently supports only atom &key internal names");
    };

    Ok(InlineParameter {
        binding: internal_binding,
        kind: InlineParameterKind::Keyword {
            keyword: external.to_owned(),
        },
        default_value: keyword_parameter_default_value(input, child),
        supplied_p: keyword_parameter_supplied_p(child)?,
    })
}

pub(in super::super) fn keyword_parameter_default_value(
    input: &str,
    child: &ExpressionView,
) -> Option<String> {
    child
        .children
        .get(1)
        .map(|default| default.span.slice(input).to_owned())
}

pub(in super::super) fn keyword_parameter_supplied_p(
    child: &ExpressionView,
) -> Result<Option<String>> {
    match child.children.len() {
        0..=2 => Ok(None),
        3 => Ok(Some(
            atom_text(&child.children[2])
                .context(
                    "inline-function currently supports only atom supplied-p names in &key parameter specifications",
                )?
                .to_owned(),
        )),
        _ => anyhow::bail!(
            "inline-function currently supports only simple &key parameter specifications"
        ),
    }
}

pub(in super::super) fn aux_parameter(
    input: &str,
    child: &ExpressionView,
) -> Result<InlineParameter> {
    if let Some(name) = atom_text(child) {
        return Ok(InlineParameter {
            binding: InlineParameterBinding::Name(name.to_owned()),
            kind: InlineParameterKind::Aux,
            default_value: None,
            supplied_p: None,
        });
    }

    let binding = child
        .children
        .first()
        .context("inline-function currently supports only simple &aux parameter specifications")?;
    let name = atom_text(binding)
        .context("inline-function currently supports only simple &aux parameter specifications")?;
    if child.children.len() > 2 {
        anyhow::bail!(
            "inline-function currently supports only simple &aux parameter specifications"
        );
    }

    Ok(InlineParameter {
        binding: InlineParameterBinding::Name(name.to_owned()),
        kind: InlineParameterKind::Aux,
        default_value: child
            .children
            .get(1)
            .map(|value| value.span.slice(input).to_owned()),
        supplied_p: None,
    })
}

pub(in super::super) fn is_dotted_list_separator(child: &ExpressionView) -> bool {
    atom_text(child) == Some(".")
}

pub(in super::super) fn dotted_tail_parameter_name(child: &ExpressionView) -> Result<&str> {
    let name = atom_text(child)
        .context("inline-function dotted lambda lists must end in a binding name")?;
    if name == "." || name.starts_with('&') {
        anyhow::bail!("inline-function dotted lambda lists must end in a binding name");
    }
    Ok(name)
}

pub(super) fn parse_required_parameter(
    input: &str,
    definition_kind: InlineDefinitionKind,
    child: &ExpressionView,
) -> Result<InlineParameter> {
    if let Some(name) = atom_text(child) {
        return Ok(InlineParameter {
            binding: InlineParameterBinding::Name(name.to_owned()),
            kind: InlineParameterKind::Positional { optional: false },
            default_value: None,
            supplied_p: None,
        });
    }

    if definition_kind != InlineDefinitionKind::Macro {
        anyhow::bail!("inline-function currently supports only simple symbol parameters");
    }

    Ok(InlineParameter {
        binding: InlineParameterBinding::Destructure(parse_macro_destructure_pattern(
            input, child,
        )?),
        kind: InlineParameterKind::Positional { optional: false },
        default_value: None,
        supplied_p: None,
    })
}
