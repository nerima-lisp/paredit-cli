use anyhow::{Context, Result};

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionKind, ExpressionView, Path, SyntaxTree};

use super::super::definition::{
    InlineDestructureKeyPattern, InlineDestructureListPattern, InlineDestructureOptionalPattern,
    InlineDestructurePattern,
};
use super::super::substitution::substitute_expression;
use super::binding::{bind_aux_parameter, destructured_binding_entries};
use super::keyword_args::{call_side_allow_other_keys_from_views, is_allow_other_keys_keyword};
use super::types::CallSideAllowOtherKeys;

pub(super) fn destructure_argument_entries(
    dialect: Dialect,
    pattern: &InlineDestructurePattern,
    argument: &str,
    allow_drop_arguments: bool,
) -> Result<Vec<(String, String)>> {
    let tree = SyntaxTree::parse_with_dialect(argument, dialect).with_context(|| {
        format!("inline-function could not parse macro destructuring argument: {argument}")
    })?;
    let argument_expression = tree
        .select_path(&Path::root_child(0))
        .context("inline-function expected a single argument expression for destructuring")?
        .view();
    if tree.root_children().len() != 1 {
        anyhow::bail!("inline-function expected a single argument expression for destructuring");
    }

    let mut entries = Vec::new();
    collect_destructured_argument_entries(
        dialect,
        pattern,
        &argument_expression,
        argument,
        allow_drop_arguments,
        &mut entries,
    )?;
    Ok(entries)
}

fn collect_destructured_argument_entries(
    dialect: Dialect,
    pattern: &InlineDestructurePattern,
    argument: &ExpressionView,
    source: &str,
    allow_drop_arguments: bool,
    output: &mut Vec<(String, String)>,
) -> Result<()> {
    match pattern {
        InlineDestructurePattern::Name(name) => {
            output.push((name.clone(), argument.span.slice(source).to_owned()));
            Ok(())
        }
        InlineDestructurePattern::List(items) => {
            if argument.kind != ExpressionKind::List {
                anyhow::bail!(
                    "inline-function macro destructuring expected a list argument, found {}",
                    argument.span.slice(source)
                );
            }
            collect_destructured_list_argument_entries(
                dialect,
                items,
                argument,
                source,
                allow_drop_arguments,
                output,
            )
        }
    }
}

fn collect_destructured_list_argument_entries(
    dialect: Dialect,
    pattern: &InlineDestructureListPattern,
    argument: &ExpressionView,
    source: &str,
    allow_drop_arguments: bool,
    output: &mut Vec<(String, String)>,
) -> Result<()> {
    if argument.children.len() < pattern.required.len() {
        anyhow::bail!(
            "inline-function macro destructuring arity mismatch: pattern expects at least {} element(s), argument has {} element(s)",
            pattern.required.len(),
            argument.children.len()
        );
    }
    let max_len = pattern.required.len() + pattern.optional.len();
    if pattern.keys.is_empty() && pattern.rest.is_none() && argument.children.len() > max_len {
        anyhow::bail!(
            "inline-function macro destructuring arity mismatch: pattern expects at most {} element(s), argument has {} element(s)",
            max_len,
            argument.children.len()
        );
    }
    let key_start = max_len.min(argument.children.len());
    let key_args = &argument.children[key_start..];
    if !pattern.keys.is_empty() && key_args.len() % 2 != 0 {
        anyhow::bail!(
            "inline-function inner &key destructuring arguments must be supplied as keyword/value pairs"
        );
    }
    let call_side_allow_other_keys = call_side_allow_other_keys_from_views(key_args, source);

    let mut default_scope = Vec::new();
    if let Some(whole) = &pattern.whole {
        let entry = (whole.clone(), argument.span.slice(source).to_owned());
        default_scope.push(entry.clone());
        output.push(entry);
    }

    for (item, child) in pattern.required.iter().zip(argument.children.iter()) {
        let entries =
            destructured_binding_entries(dialect, item, child.span.slice(source).to_owned())?;
        default_scope.extend(entries.clone());
        output.extend(entries);
    }

    for (index, item) in pattern.optional.iter().enumerate() {
        let child = argument.children.get(pattern.required.len() + index);
        let binding =
            bind_optional_destructure_pattern(dialect, item, child, source, &default_scope)?;
        default_scope.extend(binding.clone());
        output.extend(binding);
    }

    if let Some(rest) = &pattern.rest {
        let rest_argument = list_destructure_argument(key_args, source);
        let entries = destructured_binding_entries(dialect, rest, rest_argument)?;
        default_scope.extend(entries.clone());
        output.extend(entries);
    }

    for item in &pattern.keys {
        let mut matched = None;
        for pair in key_args.chunks_exact(2) {
            let key = pair[0].span.slice(source);
            let value = pair[1].span.slice(source);
            if !key.starts_with(':') {
                anyhow::bail!(
                    "inline-function inner &key destructuring expected keyword argument, found {key}"
                );
            }
            if key == item.keyword {
                if matched.is_some() {
                    anyhow::bail!(
                        "inline-function macro destructuring argument supplies duplicate keyword {}",
                        item.keyword
                    );
                }
                matched = Some(value.to_owned());
            }
        }
        let binding = bind_key_destructure_pattern(dialect, item, matched, &default_scope)?;
        default_scope.extend(binding.clone());
        output.extend(binding);
    }

    for item in &pattern.aux {
        let binding = bind_aux_parameter(dialect, item, &default_scope)?;
        default_scope.extend(binding.default_scope_entries.clone());
        output.extend(binding.body_entries);
    }

    validate_unknown_destructure_keywords(
        pattern,
        key_args,
        source,
        allow_drop_arguments,
        &call_side_allow_other_keys,
    )?;

    Ok(())
}

fn validate_unknown_destructure_keywords(
    pattern: &InlineDestructureListPattern,
    key_args: &[ExpressionView],
    source: &str,
    allow_drop_arguments: bool,
    call_side_allow_other_keys: &CallSideAllowOtherKeys,
) -> Result<()> {
    if pattern.keys.is_empty() {
        return Ok(());
    }

    for pair in key_args.chunks_exact(2) {
        let key = pair[0].span.slice(source);
        if !pattern.keys.iter().any(|item| item.keyword == key) {
            if is_allow_other_keys_keyword(key) {
                continue;
            }
            if should_tolerate_unknown_destructure_keyword(
                pattern,
                allow_drop_arguments,
                call_side_allow_other_keys,
            )? {
                continue;
            }
            anyhow::bail!(
                "inline-function macro destructuring argument supplies unsupported keyword {key}"
            );
        }
    }

    Ok(())
}

fn should_tolerate_unknown_destructure_keyword(
    pattern: &InlineDestructureListPattern,
    allow_drop_arguments: bool,
    call_side_allow_other_keys: &CallSideAllowOtherKeys,
) -> Result<bool> {
    if pattern.rest.is_some() {
        if pattern.allow_other_keys {
            return Ok(true);
        }
        return call_side_allows_other_keys(call_side_allow_other_keys);
    }
    if allow_drop_arguments {
        if pattern.allow_other_keys {
            return Ok(true);
        }
        return call_side_allows_other_keys(call_side_allow_other_keys);
    }
    Ok(false)
}

fn call_side_allows_other_keys(
    call_side_allow_other_keys: &CallSideAllowOtherKeys,
) -> Result<bool> {
    match call_side_allow_other_keys {
        CallSideAllowOtherKeys::True => Ok(true),
        CallSideAllowOtherKeys::Unknown(value) => {
            anyhow::bail!(
                "inline-function cannot determine whether inner :allow-other-keys value {value} suppresses unknown keyword"
            );
        }
        CallSideAllowOtherKeys::AbsentOrFalse => Ok(false),
    }
}

fn list_destructure_argument(arguments: &[ExpressionView], source: &str) -> String {
    if arguments.is_empty() {
        return "()".to_owned();
    }

    format!(
        "({})",
        arguments
            .iter()
            .map(|argument| argument.span.slice(source))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn bind_optional_destructure_pattern(
    dialect: Dialect,
    pattern: &InlineDestructureOptionalPattern,
    argument: Option<&ExpressionView>,
    source: &str,
    default_scope: &[(String, String)],
) -> Result<Vec<(String, String)>> {
    let mut entries = if let Some(argument) = argument {
        let mut entries = destructured_binding_entries(
            dialect,
            &pattern.binding,
            argument.span.slice(source).to_owned(),
        )?;
        if let Some(supplied_p) = &pattern.supplied_p {
            entries.push((supplied_p.clone(), "t".to_owned()));
        }
        entries
    } else {
        let default_value = resolve_destructure_default_value(dialect, pattern, default_scope)?;
        let mut entries = destructured_binding_entries(dialect, &pattern.binding, default_value)?;
        if let Some(supplied_p) = &pattern.supplied_p {
            entries.push((supplied_p.clone(), "nil".to_owned()));
        }
        entries
    };
    Ok(std::mem::take(&mut entries))
}

fn resolve_destructure_default_value(
    dialect: Dialect,
    pattern: &InlineDestructureOptionalPattern,
    default_scope: &[(String, String)],
) -> Result<String> {
    let Some(value) = pattern.default_value.as_ref() else {
        return Ok("nil".to_owned());
    };
    let (names, arguments): (Vec<_>, Vec<_>) = default_scope.iter().cloned().unzip();
    substitute_expression(dialect, value, &names, &arguments)
}

fn bind_key_destructure_pattern(
    dialect: Dialect,
    pattern: &InlineDestructureKeyPattern,
    argument: Option<String>,
    default_scope: &[(String, String)],
) -> Result<Vec<(String, String)>> {
    let mut entries = if let Some(argument) = argument {
        let mut entries = destructured_binding_entries(dialect, &pattern.binding, argument)?;
        if let Some(supplied_p) = &pattern.supplied_p {
            entries.push((supplied_p.clone(), "t".to_owned()));
        }
        entries
    } else {
        let default_value = resolve_key_destructure_default_value(dialect, pattern, default_scope)?;
        let mut entries = destructured_binding_entries(dialect, &pattern.binding, default_value)?;
        if let Some(supplied_p) = &pattern.supplied_p {
            entries.push((supplied_p.clone(), "nil".to_owned()));
        }
        entries
    };
    Ok(std::mem::take(&mut entries))
}

fn resolve_key_destructure_default_value(
    dialect: Dialect,
    pattern: &InlineDestructureKeyPattern,
    default_scope: &[(String, String)],
) -> Result<String> {
    let Some(value) = pattern.default_value.as_ref() else {
        return Ok("nil".to_owned());
    };
    let (names, arguments): (Vec<_>, Vec<_>) = default_scope.iter().cloned().unzip();
    substitute_expression(dialect, value, &names, &arguments)
}
