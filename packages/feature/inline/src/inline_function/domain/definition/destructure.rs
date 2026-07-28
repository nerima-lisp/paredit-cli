use crate::error::{InlineError, InlineResult, UnsupportedLambdaList};

use paredit_core_syntax::sexpr::{Delimiter, ExpressionView};

use super::super::syntax::atom_text;
use super::lambda_list::parameters::{
    aux_parameter, dotted_tail_parameter_name, is_dotted_list_separator,
    keyword_parameter_default_value, keyword_parameter_supplied_p,
};
use super::types::{
    InlineDestructureKeyPattern, InlineDestructureListPattern, InlineDestructureOptionalPattern,
    InlineDestructurePattern,
};

pub fn parse_macro_destructure_pattern(
    input: &str,
    child: &ExpressionView,
) -> InlineResult<InlineDestructurePattern> {
    if let Some(name) = atom_text(child) {
        if name.starts_with('&') {
            return Err(UnsupportedLambdaList::SupportsOnly {
                supported: "required destructuring patterns in defmacro parameter lists".to_owned(),
            }
            .into());
        }
        return Ok(InlineDestructurePattern::Name(name.to_owned()));
    }

    match child.delimiter {
        Some(Delimiter::Paren | Delimiter::Bracket) => {}
        _ => {
            return Err(UnsupportedLambdaList::SupportsOnly {
                supported: "list destructuring patterns in defmacro parameter lists".to_owned(),
            }
            .into());
        }
    }

    let mut whole = None;
    let mut required = Vec::with_capacity(child.children.len());
    let mut optional = Vec::new();
    let mut rest = None;
    let mut keys = Vec::new();
    let mut aux = Vec::new();
    let mut in_whole = false;
    let mut in_optional = false;
    let mut in_rest = false;
    let mut in_key = false;
    let mut in_aux = false;
    let mut allow_other_keys = false;

    for (index, pattern) in child.children.iter().enumerate() {
        if is_dotted_list_separator(pattern) {
            if in_key || in_aux {
                return Err(UnsupportedLambdaList::NotSupportedAfter {
                    construct: "dotted destructuring lists".to_owned(),
                    after: if in_key { "&key" } else { "&aux" }.to_string(),
                }
                .into());
            }
            if rest.is_some() || in_rest {
                return Err(UnsupportedLambdaList::AtMostOne {
                    construct: "inner &rest or &body destructuring parameter".to_owned(),
                }
                .into());
            }
            if index == 0 {
                return Err(UnsupportedLambdaList::Requirement {
                    subject: "inner dotted destructuring".to_owned(),
                    requirement: "begin with a binding pattern".to_owned(),
                }
                .into());
            }
            let tail = child.children.get(index + 1).ok_or_else(|| {
                InlineError::from(UnsupportedLambdaList::MustBeFollowedBy {
                    marker: "inner dotted destructuring".to_owned(),
                    expected: "a binding name".to_owned(),
                })
            })?;
            if index + 2 != child.children.len() {
                return Err(UnsupportedLambdaList::Requirement {
                    subject: "inner dotted destructuring".to_owned(),
                    requirement: "end after the tail binding".to_owned(),
                }
                .into());
            }
            rest = Some(Box::new(InlineDestructurePattern::Name(
                dotted_tail_parameter_name(tail)?.to_owned(),
            )));
            break;
        }
        if let Some(marker) = atom_text(pattern).filter(|name| name.starts_with('&')) {
            if in_whole {
                return Err(UnsupportedLambdaList::MustBeFollowedBy {
                    marker: "inner &whole".to_owned(),
                    expected: "a binding name".to_owned(),
                }
                .into());
            }
            if in_rest {
                return Err(UnsupportedLambdaList::MustBeFollowedBy {
                    marker: "inner &rest or &body".to_owned(),
                    expected: "a binding pattern".to_owned(),
                }
                .into());
            }
            if in_aux {
                return Err(UnsupportedLambdaList::NotSupportedAfter {
                    construct: format!("inner {marker} destructuring"),
                    after: "&aux".to_owned(),
                }
                .into());
            }

            match marker {
                "&whole" => {
                    if whole.is_some()
                        || !required.is_empty()
                        || !optional.is_empty()
                        || rest.is_some()
                        || !keys.is_empty()
                        || !aux.is_empty()
                        || in_optional
                        || in_key
                        || in_aux
                        || allow_other_keys
                    {
                        return Err(UnsupportedLambdaList::Requirement {
                            subject: "inner &whole".to_owned(),
                            requirement: "appear before any other destructuring parameter"
                                .to_owned(),
                        }
                        .into());
                    }
                    in_whole = true;
                    in_optional = false;
                    in_rest = false;
                    in_key = false;
                    in_aux = false;
                    continue;
                }
                "&optional" => {
                    if rest.is_some() {
                        return Err(UnsupportedLambdaList::NotSupportedAfter {
                            construct: "inner &optional destructuring parameters".to_owned(),
                            after: "&rest or &body".to_owned(),
                        }
                        .into());
                    }
                    in_optional = true;
                    in_rest = false;
                    in_key = false;
                    in_aux = false;
                    continue;
                }
                "&rest" | "&body" => {
                    if in_key {
                        return Err(UnsupportedLambdaList::NotSupportedAfter {
                            construct: format!("inner {marker} destructuring"),
                            after: "&key".to_owned(),
                        }
                        .into());
                    }
                    if rest.is_some() || in_rest {
                        return Err(UnsupportedLambdaList::AtMostOne {
                            construct: "inner &rest or &body destructuring parameter".to_owned(),
                        }
                        .into());
                    }
                    in_optional = false;
                    in_rest = true;
                    in_key = false;
                    in_aux = false;
                    continue;
                }
                "&key" => {
                    in_optional = false;
                    in_rest = false;
                    in_key = true;
                    in_aux = false;
                    continue;
                }
                "&allow-other-keys" if in_key => {
                    allow_other_keys = true;
                    continue;
                }
                "&aux" => {
                    if in_aux || !aux.is_empty() {
                        return Err(UnsupportedLambdaList::AtMostOne {
                            construct: "inner &aux destructuring section".to_owned(),
                        }
                        .into());
                    }
                    in_optional = false;
                    in_rest = false;
                    in_key = false;
                    in_aux = true;
                    continue;
                }
                _ => {
                    return Err(UnsupportedLambdaList::SupportsOnly {
                        supported: format!(
                            "inner &whole, &optional, &rest, &body, &key, &allow-other-keys, and &aux destructuring markers in defmacro parameter lists; found {marker}"
                        ),
                    }
                    .into());
                }
            }
        }

        if in_whole {
            whole = Some(
                atom_text(pattern)
                    .ok_or_else(|| {
                        InlineError::from(UnsupportedLambdaList::SupportsOnly {
                            supported: "simple symbol inner &whole destructuring parameters"
                                .to_owned(),
                        })
                    })?
                    .to_owned(),
            );
            in_whole = false;
            continue;
        }

        if in_key {
            keys.push(parse_macro_key_destructure_pattern(input, pattern)?);
        } else if in_rest {
            rest = Some(Box::new(parse_macro_destructure_pattern(input, pattern)?));
            in_rest = false;
        } else if in_optional {
            optional.push(parse_macro_optional_destructure_pattern(input, pattern)?);
        } else if rest.is_some() {
            return Err(UnsupportedLambdaList::NotSupportedAfter {
                construct: "required destructuring parameters".to_owned(),
                after: "inner &rest or &body".to_owned(),
            }
            .into());
        } else if in_aux {
            aux.push(aux_parameter(input, pattern)?);
        } else {
            required.push(parse_macro_destructure_pattern(input, pattern)?);
        }
    }
    if in_whole {
        return Err(UnsupportedLambdaList::MustBeFollowedBy {
            marker: "inner &whole".to_owned(),
            expected: "a binding name".to_owned(),
        }
        .into());
    }
    if in_rest {
        return Err(UnsupportedLambdaList::MustBeFollowedBy {
            marker: "inner &rest or &body".to_owned(),
            expected: "a binding pattern".to_owned(),
        }
        .into());
    }
    Ok(InlineDestructurePattern::List(
        InlineDestructureListPattern {
            whole,
            required,
            optional,
            rest,
            keys,
            aux,
            allow_other_keys,
        },
    ))
}

fn parse_macro_optional_destructure_pattern(
    input: &str,
    child: &ExpressionView,
) -> InlineResult<InlineDestructureOptionalPattern> {
    if atom_text(child).is_some() {
        return Ok(InlineDestructureOptionalPattern {
            binding: parse_macro_destructure_pattern(input, child)?,
            default_value: None,
            supplied_p: None,
        });
    }

    let binding = child.children.first().ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::SupportsOnly {
            supported: "simple or destructuring inner &optional parameter specifications"
                .to_owned(),
        })
    })?;
    let supplied_p = match child.children.len() {
        0..=2 => None,
        3 => Some(
            atom_text(&child.children[2])
                .ok_or_else(|| {
                    InlineError::from(UnsupportedLambdaList::SupportsOnly {
                        supported:
                            "atom supplied-p names in inner &optional parameter specifications"
                                .to_owned(),
                    })
                })?
                .to_owned(),
        ),
        _ => {
            return Err(UnsupportedLambdaList::SupportsOnly {
                supported: "simple or destructuring inner &optional parameter specifications"
                    .to_owned(),
            }
            .into());
        }
    };

    Ok(InlineDestructureOptionalPattern {
        binding: parse_macro_destructure_pattern(input, binding)?,
        default_value: child
            .children
            .get(1)
            .map(|value| value.span.slice(input).to_owned()),
        supplied_p,
    })
}

fn parse_macro_key_destructure_pattern(
    input: &str,
    child: &ExpressionView,
) -> InlineResult<InlineDestructureKeyPattern> {
    if let Some(name) = atom_text(child) {
        if name.starts_with(':') {
            return Err(UnsupportedLambdaList::RequiresBindingName {
                parameter: format!("inner &key destructuring parameter {name}"),
            }
            .into());
        }
        return Ok(InlineDestructureKeyPattern {
            binding: InlineDestructurePattern::Name(name.to_owned()),
            keyword: format!(":{name}"),
            default_value: None,
            supplied_p: None,
        });
    }

    let binding = child.children.first().ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::SupportsOnly {
            supported: "simple inner &key destructuring specifications".to_owned(),
        })
    })?;
    let (binding, keyword) = parse_macro_key_binding(input, binding)?;
    Ok(InlineDestructureKeyPattern {
        binding,
        keyword,
        default_value: keyword_parameter_default_value(input, child),
        supplied_p: keyword_parameter_supplied_p(child)?,
    })
}

fn parse_macro_key_binding(
    input: &str,
    binding: &ExpressionView,
) -> InlineResult<(InlineDestructurePattern, String)> {
    if let Some(name) = atom_text(binding) {
        if name.starts_with(':') {
            return Err(UnsupportedLambdaList::RequiresBindingName {
                parameter: format!("inner &key destructuring parameter {name}"),
            }
            .into());
        }
        return Ok((
            InlineDestructurePattern::Name(name.to_owned()),
            format!(":{name}"),
        ));
    }

    if binding.children.len() == 2
        && atom_text(&binding.children[0]).is_some_and(|name| name.starts_with(':'))
    {
        let external = atom_text(&binding.children[0]).ok_or_else(|| {
            InlineError::from(UnsupportedLambdaList::SupportsOnly {
                supported: "atom inner &key external names".to_owned(),
            })
        })?;
        let internal = parse_macro_destructure_pattern(input, &binding.children[1])?;
        return Ok((internal, external.to_owned()));
    }

    let pattern = parse_macro_destructure_pattern(input, binding)?;
    let first_name = pattern.binding_names().first().cloned().ok_or_else(|| {
        InlineError::from(UnsupportedLambdaList::Requirement {
            subject: "inner &key destructuring pattern".to_owned(),
            requirement: "bind at least one name".to_owned(),
        })
    })?;
    Ok((pattern, format!(":{first_name}")))
}
