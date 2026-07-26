use anyhow::{Context, Result};

use crate::domain::sexpr::{Delimiter, ExpressionView};

use super::super::syntax::atom_text;
use super::lambda_list::parameters::{
    aux_parameter, dotted_tail_parameter_name, is_dotted_list_separator,
    keyword_parameter_default_value, keyword_parameter_supplied_p,
};
use super::types::{
    InlineDestructureKeyPattern, InlineDestructureListPattern, InlineDestructureOptionalPattern,
    InlineDestructurePattern,
};

pub(super) fn parse_macro_destructure_pattern(
    input: &str,
    child: &ExpressionView,
) -> Result<InlineDestructurePattern> {
    if let Some(name) = atom_text(child) {
        if name.starts_with('&') {
            anyhow::bail!(
                "inline-function currently supports only required destructuring patterns in defmacro parameter lists"
            );
        }
        return Ok(InlineDestructurePattern::Name(name.to_owned()));
    }

    match child.delimiter {
        Some(Delimiter::Paren | Delimiter::Bracket) => {}
        _ => {
            anyhow::bail!(
                "inline-function currently supports only list destructuring patterns in defmacro parameter lists"
            );
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
                anyhow::bail!(
                    "inline-function does not support dotted destructuring lists after {}",
                    if in_key { "&key" } else { "&aux" }
                );
            }
            if rest.is_some() || in_rest {
                anyhow::bail!(
                    "inline-function supports at most one inner &rest or &body destructuring parameter"
                );
            }
            if index == 0 {
                anyhow::bail!(
                    "inline-function inner dotted destructuring must begin with a binding pattern"
                );
            }
            let tail = child.children.get(index + 1).context(
                "inline-function inner dotted destructuring must be followed by a binding name",
            )?;
            if index + 2 != child.children.len() {
                anyhow::bail!(
                    "inline-function inner dotted destructuring must end after the tail binding"
                );
            }
            rest = Some(Box::new(InlineDestructurePattern::Name(
                dotted_tail_parameter_name(tail)?.to_owned(),
            )));
            break;
        }
        if let Some(marker) = atom_text(pattern).filter(|name| name.starts_with('&')) {
            if in_whole {
                anyhow::bail!("inline-function inner &whole must be followed by a binding name");
            }
            if in_rest {
                anyhow::bail!(
                    "inline-function inner &rest or &body must be followed by a binding pattern"
                );
            }
            if in_aux {
                anyhow::bail!(
                    "inline-function does not support inner {marker} destructuring after &aux"
                );
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
                        anyhow::bail!(
                            "inline-function inner &whole must appear before any other destructuring parameter"
                        );
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
                        anyhow::bail!(
                            "inline-function does not support inner &optional destructuring parameters after &rest or &body"
                        );
                    }
                    in_optional = true;
                    in_rest = false;
                    in_key = false;
                    in_aux = false;
                    continue;
                }
                "&rest" | "&body" => {
                    if in_key {
                        anyhow::bail!(
                            "inline-function does not support inner {marker} destructuring after &key"
                        );
                    }
                    if rest.is_some() || in_rest {
                        anyhow::bail!(
                            "inline-function supports at most one inner &rest or &body destructuring parameter"
                        );
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
                        anyhow::bail!(
                            "inline-function supports at most one inner &aux destructuring section"
                        );
                    }
                    in_optional = false;
                    in_rest = false;
                    in_key = false;
                    in_aux = true;
                    continue;
                }
                _ => anyhow::bail!(
                    "inline-function currently supports only inner &whole, &optional, &rest, &body, &key, &allow-other-keys, and &aux destructuring markers in defmacro parameter lists; found {marker}"
                ),
            }
        }

        if in_whole {
            whole = Some(
                atom_text(pattern)
                    .context(
                        "inline-function currently supports only simple symbol inner &whole destructuring parameters",
                    )?
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
            anyhow::bail!(
                "inline-function does not support required destructuring parameters after inner &rest or &body"
            );
        } else if in_aux {
            aux.push(aux_parameter(input, pattern)?);
        } else {
            required.push(parse_macro_destructure_pattern(input, pattern)?);
        }
    }
    if in_whole {
        anyhow::bail!("inline-function inner &whole must be followed by a binding name");
    }
    if in_rest {
        anyhow::bail!("inline-function inner &rest or &body must be followed by a binding pattern");
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
) -> Result<InlineDestructureOptionalPattern> {
    if atom_text(child).is_some() {
        return Ok(InlineDestructureOptionalPattern {
            binding: parse_macro_destructure_pattern(input, child)?,
            default_value: None,
            supplied_p: None,
        });
    }

    let binding = child.children.first().context(
        "inline-function currently supports only simple or destructuring inner &optional parameter specifications",
    )?;
    let supplied_p = match child.children.len() {
        0..=2 => None,
        3 => Some(
            atom_text(&child.children[2])
                .context(
                    "inline-function currently supports only atom supplied-p names in inner &optional parameter specifications",
                )?
                .to_owned(),
        ),
        _ => anyhow::bail!(
            "inline-function currently supports only simple or destructuring inner &optional parameter specifications"
        ),
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
) -> Result<InlineDestructureKeyPattern> {
    if let Some(name) = atom_text(child) {
        if name.starts_with(':') {
            anyhow::bail!(
                "inline-function requires a binding name for inner &key destructuring parameter {name}"
            );
        }
        return Ok(InlineDestructureKeyPattern {
            binding: InlineDestructurePattern::Name(name.to_owned()),
            keyword: format!(":{name}"),
            default_value: None,
            supplied_p: None,
        });
    }

    let binding = child.children.first().context(
        "inline-function currently supports only simple inner &key destructuring specifications",
    )?;
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
) -> Result<(InlineDestructurePattern, String)> {
    if let Some(name) = atom_text(binding) {
        if name.starts_with(':') {
            anyhow::bail!(
                "inline-function requires a binding name for inner &key destructuring parameter {name}"
            );
        }
        return Ok((
            InlineDestructurePattern::Name(name.to_owned()),
            format!(":{name}"),
        ));
    }

    if binding.children.len() == 2
        && atom_text(&binding.children[0]).is_some_and(|name| name.starts_with(':'))
    {
        let external = atom_text(&binding.children[0])
            .context("inline-function currently supports only atom inner &key external names")?;
        let internal = parse_macro_destructure_pattern(input, &binding.children[1])?;
        return Ok((internal, external.to_owned()));
    }

    let pattern = parse_macro_destructure_pattern(input, binding)?;
    let first_name =
        pattern.binding_names().first().cloned().context(
            "inline-function inner &key destructuring pattern must bind at least one name",
        )?;
    Ok((pattern, format!(":{first_name}")))
}
