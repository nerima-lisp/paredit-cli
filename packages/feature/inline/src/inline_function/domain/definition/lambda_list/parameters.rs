use crate::error::{InlineError, InlineResult, UnsupportedLambdaList};

use paredit_core_syntax::sexpr::ExpressionView;

use super::super::super::syntax::atom_text;
use super::super::destructure::parse_macro_destructure_pattern;
use super::super::types::{
    InlineDefinitionKind, InlineParameter, InlineParameterBinding, InlineParameterKind,
};

pub fn rest_parameter_name(child: &ExpressionView) -> InlineResult<&str> {
    atom_text(child).ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::SupportsOnly {
            supported: "simple symbol &rest parameters".to_owned(),
        })
    })
}

pub fn whole_parameter_name(child: &ExpressionView) -> InlineResult<&str> {
    atom_text(child).ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::SupportsOnly {
            supported: "simple symbol &whole parameters".to_owned(),
        })
    })
}

pub fn environment_parameter_name(child: &ExpressionView) -> InlineResult<&str> {
    atom_text(child).ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::SupportsOnly {
            supported: "simple symbol &environment parameters".to_owned(),
        })
    })
}

pub fn optional_parameter(
    input: &str,
    definition_kind: InlineDefinitionKind,
    child: &ExpressionView,
) -> InlineResult<InlineParameter> {
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
) -> InlineResult<InlineParameterBinding> {
    if let Some(name) = atom_text(child) {
        return Ok(InlineParameterBinding::Name(name.to_owned()));
    }

    let binding = child.children.first().ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::SupportsOnly {
            supported: "simple or destructuring &optional parameter specifications".to_owned(),
        })
    })?;
    if let Some(name) = atom_text(binding) {
        return Ok(InlineParameterBinding::Name(name.to_owned()));
    }

    if definition_kind != InlineDefinitionKind::Macro {
        return Err(UnsupportedLambdaList::SupportsOnly {
            supported: "simple symbol parameters".to_owned(),
        }
        .into());
    }

    Ok(InlineParameterBinding::Destructure(
        parse_macro_destructure_pattern(input, binding)?,
    ))
}

fn optional_parameter_supplied_p(child: &ExpressionView) -> InlineResult<Option<String>> {
    match child.children.len() {
        0..=2 => Ok(None),
        3 => Ok(Some(
            atom_text(&child.children[2])
                .ok_or_else(|| {
                    InlineError::from(UnsupportedLambdaList::SupportsOnly {
                        supported: "atom supplied-p names in &optional parameter specifications"
                            .to_owned(),
                    })
                })?
                .to_owned(),
        )),
        _ => Err(UnsupportedLambdaList::SupportsOnly {
            supported: "simple or destructuring &optional parameter specifications".to_owned(),
        }
        .into()),
    }
}

fn optional_parameter_default_value(input: &str, child: &ExpressionView) -> Option<String> {
    child
        .children
        .get(1)
        .map(|default| default.span.slice(input).to_owned())
}

pub fn keyword_parameter(
    input: &str,
    definition_kind: InlineDefinitionKind,
    child: &ExpressionView,
) -> InlineResult<InlineParameter> {
    if let Some(name) = atom_text(child) {
        if name.starts_with(':') {
            return Err(UnsupportedLambdaList::RequiresBindingName {
                parameter: format!("&key parameter {name}"),
            }
            .into());
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

    let binding = child.children.first().ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::SupportsOnly {
            supported: "simple &key parameter specifications".to_owned(),
        })
    })?;
    if let Some(name) = atom_text(binding) {
        if name.starts_with(':') {
            return Err(UnsupportedLambdaList::RequiresBindingName {
                parameter: format!("&key parameter {name}"),
            }
            .into());
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
        return Err(UnsupportedLambdaList::SupportsOnly {
            supported: "(:keyword name) &key bindings".to_owned(),
        }
        .into());
    };
    let external = atom_text(external).ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::SupportsOnly {
            supported: "atom &key external names".to_owned(),
        })
    })?;
    if !external.starts_with(':') {
        return Err(UnsupportedLambdaList::Requirement {
            subject: "&key external name".to_owned(),
            requirement: format!("be a keyword: {external}"),
        }
        .into());
    }
    let internal_binding = if let Some(internal) = atom_text(internal) {
        if internal.starts_with(':') {
            return Err(UnsupportedLambdaList::Requirement {
                subject: "&key internal binding".to_owned(),
                requirement: format!("not be a keyword: {internal}"),
            }
            .into());
        }
        InlineParameterBinding::Name(internal.to_owned())
    } else if definition_kind == InlineDefinitionKind::Macro {
        InlineParameterBinding::Destructure(parse_macro_destructure_pattern(input, internal)?)
    } else {
        return Err(UnsupportedLambdaList::SupportsOnly {
            supported: "atom &key internal names".to_owned(),
        }
        .into());
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
) -> InlineResult<Option<String>> {
    match child.children.len() {
        0..=2 => Ok(None),
        3 => Ok(Some(
            atom_text(&child.children[2])
                .ok_or_else(|| {
                    InlineError::from(UnsupportedLambdaList::SupportsOnly {
                        supported: "atom supplied-p names in &key parameter specifications"
                            .to_owned(),
                    })
                })?
                .to_owned(),
        )),
        _ => Err(UnsupportedLambdaList::SupportsOnly {
            supported: "simple &key parameter specifications".to_owned(),
        }
        .into()),
    }
}

pub(in super::super) fn aux_parameter(
    input: &str,
    child: &ExpressionView,
) -> InlineResult<InlineParameter> {
    if let Some(name) = atom_text(child) {
        return Ok(InlineParameter {
            binding: InlineParameterBinding::Name(name.to_owned()),
            kind: InlineParameterKind::Aux,
            default_value: None,
            supplied_p: None,
        });
    }

    let binding = child.children.first().ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::SupportsOnly {
            supported: "simple &aux parameter specifications".to_owned(),
        })
    })?;
    let name = atom_text(binding).ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::SupportsOnly {
            supported: "simple &aux parameter specifications".to_owned(),
        })
    })?;
    if child.children.len() > 2 {
        return Err(UnsupportedLambdaList::SupportsOnly {
            supported: "simple &aux parameter specifications".to_owned(),
        }
        .into());
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

pub(in super::super) fn dotted_tail_parameter_name(child: &ExpressionView) -> InlineResult<&str> {
    let name = atom_text(child).ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::Requirement {
            subject: "dotted lambda lists".to_owned(),
            requirement: "end in a binding name".to_owned(),
        })
    })?;
    if name == "." || name.starts_with('&') {
        return Err(UnsupportedLambdaList::Requirement {
            subject: "dotted lambda lists".to_owned(),
            requirement: "end in a binding name".to_owned(),
        }
        .into());
    }
    Ok(name)
}

pub fn parse_required_parameter(
    input: &str,
    definition_kind: InlineDefinitionKind,
    child: &ExpressionView,
) -> InlineResult<InlineParameter> {
    if let Some(name) = atom_text(child) {
        return Ok(InlineParameter {
            binding: InlineParameterBinding::Name(name.to_owned()),
            kind: InlineParameterKind::Positional { optional: false },
            default_value: None,
            supplied_p: None,
        });
    }

    if definition_kind != InlineDefinitionKind::Macro {
        return Err(UnsupportedLambdaList::SupportsOnly {
            supported: "simple symbol parameters".to_owned(),
        }
        .into());
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
