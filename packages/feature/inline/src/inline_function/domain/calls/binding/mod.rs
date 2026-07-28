use crate::error::{CallBindingError, InlineInternalError, InlineResult};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SymbolName;

use self::parameter_binding::{
    bind_keyword_parameter, bind_positional_parameter, bound_parameter_argument_entries,
};
use super::super::definition::{InlineParameter, InlineParameterKind};
use super::types::{InlineArgumentBindings, InlineFunctionCall};

mod keyword_validation;
mod parameter_binding;

use keyword_validation::validate_unknown_keyword_arguments;

struct InlineBindingContext<'a> {
    dialect: Dialect,
    params: &'a [InlineParameter],
    non_aux_params: &'a [InlineParameter],
    keyword_params: &'a [InlineParameter],
    rest_index: Option<usize>,
    keyword_start: usize,
    aux_start: usize,
    positional_count: usize,
    raw_args: Vec<String>,
    whole_call: String,
    function_name: &'a SymbolName,
    accepts_other_keys: bool,
    allow_drop_arguments: bool,
}

pub fn bind_inline_function_arguments(
    dialect: Dialect,
    params: &[InlineParameter],
    call: InlineFunctionCall,
    function_name: &SymbolName,
    accepts_other_keys: bool,
    allow_drop_arguments: bool,
) -> InlineResult<InlineArgumentBindings> {
    let InlineFunctionCall {
        raw_args,
        whole_call,
    } = call;
    let aux_start = params
        .iter()
        .position(|param| matches!(param.kind, InlineParameterKind::Aux))
        .unwrap_or(params.len());
    let non_aux_params = &params[..aux_start];
    let rest_index = params
        .iter()
        .position(|param| matches!(param.kind, InlineParameterKind::Rest));
    let keyword_start = non_aux_params
        .iter()
        .position(|param| matches!(param.kind, InlineParameterKind::Keyword { .. }))
        .unwrap_or(non_aux_params.len());
    let keyword_end = aux_start;
    let keyword_params = &params[keyword_start..keyword_end];
    let positional_count = non_aux_params[..keyword_start]
        .iter()
        .filter(|param| matches!(param.kind, InlineParameterKind::Positional { .. }))
        .count();
    let required_positional_count = non_aux_params[..keyword_start]
        .iter()
        .filter(|param| {
            matches!(
                param.kind,
                InlineParameterKind::Positional { optional: false }
            )
        })
        .count();

    if raw_args.len() < required_positional_count {
        return Err(CallBindingError::PositionalArity {
            function: function_name.to_string(),
            required: required_positional_count,
            actual: raw_args.len(),
        }
        .into());
    }

    if keyword_params.is_empty() {
        let ctx = InlineBindingContext {
            dialect,
            params,
            non_aux_params,
            keyword_params,
            rest_index,
            keyword_start,
            aux_start,
            positional_count,
            raw_args,
            whole_call,
            function_name,
            accepts_other_keys,
            allow_drop_arguments,
        };
        return bind_non_keyword_arguments(&ctx);
    }

    let ctx = InlineBindingContext {
        dialect,
        params,
        non_aux_params,
        keyword_params,
        rest_index,
        keyword_start,
        aux_start,
        positional_count,
        raw_args,
        whole_call,
        function_name,
        accepts_other_keys,
        allow_drop_arguments,
    };
    bind_keyword_arguments(&ctx)
}

fn bind_non_keyword_arguments(
    ctx: &InlineBindingContext<'_>,
) -> InlineResult<InlineArgumentBindings> {
    if ctx.rest_index.is_none() && ctx.raw_args.len() > ctx.positional_count {
        return Err(CallBindingError::ParameterArity {
            function: ctx.function_name.to_string(),
            parameters: ctx.positional_count,
            arguments: ctx.raw_args.len(),
        }
        .into());
    }

    let mut body_bindings = Vec::new();
    let mut argument_bindings = Vec::new();
    let mut default_scope = Vec::new();
    bind_leading_parameters(
        ctx.dialect,
        &ctx.non_aux_params[..ctx.keyword_start],
        &ctx.raw_args,
        &ctx.whole_call,
        ctx.allow_drop_arguments,
        &mut body_bindings,
        &mut argument_bindings,
        &mut default_scope,
    )?;
    if let Some(rest_index) = ctx.rest_index {
        let rest_argument = list_argument(&ctx.raw_args[ctx.positional_count..]);
        let rest_entries = bound_parameter_argument_entries(
            ctx.dialect,
            &ctx.params[rest_index],
            rest_argument,
            ctx.allow_drop_arguments,
        )?;
        default_scope.extend(rest_entries.clone());
        argument_bindings.extend(rest_entries);
    }
    for param in &ctx.params[ctx.aux_start..] {
        let binding = bind_aux_parameter(ctx.dialect, param, &default_scope)?;
        default_scope.extend(binding.default_scope_entries.clone());
        body_bindings.extend(binding.body_entries);
    }
    Ok(InlineArgumentBindings {
        body_bindings,
        argument_bindings,
    })
}

fn bind_keyword_arguments(ctx: &InlineBindingContext<'_>) -> InlineResult<InlineArgumentBindings> {
    let keyword_args = &ctx.raw_args[ctx.positional_count..];
    let keyword_arg_count = keyword_args.len();
    if keyword_arg_count % 2 != 0 {
        return Err(CallBindingError::KeywordPairsRequired {
            function: ctx.function_name.to_string(),
        }
        .into());
    }
    let call_side_allow_other_keys =
        super::keyword_args::call_side_allow_other_keys_from_strings(keyword_args);

    let mut body_bindings = Vec::new();
    let mut argument_bindings = Vec::new();
    let mut default_scope = Vec::new();
    bind_leading_parameters(
        ctx.dialect,
        &ctx.non_aux_params[..ctx.keyword_start],
        &ctx.raw_args,
        &ctx.whole_call,
        ctx.allow_drop_arguments,
        &mut body_bindings,
        &mut argument_bindings,
        &mut default_scope,
    )?;
    if let Some(rest_index) = ctx.rest_index {
        let rest_argument = list_argument(keyword_args);
        let rest_entries = bound_parameter_argument_entries(
            ctx.dialect,
            &ctx.params[rest_index],
            rest_argument,
            ctx.allow_drop_arguments,
        )?;
        default_scope.extend(rest_entries.clone());
        argument_bindings.extend(rest_entries);
    }
    for param in ctx.keyword_params {
        let InlineParameterKind::Keyword { keyword } = &param.kind else {
            return Err(InlineInternalError::KeywordParameterMissingKeyword.into());
        };
        let mut matched = None;
        for pair in keyword_args.chunks_exact(2) {
            let key = &pair[0];
            let value = &pair[1];
            if !key.starts_with(':') {
                return Err(CallBindingError::ExpectedKeyword {
                    function: ctx.function_name.to_string(),
                    found: key.to_string(),
                }
                .into());
            }
            if key == keyword {
                if matched.is_some() {
                    return Err(CallBindingError::DuplicateKeyword {
                        function: ctx.function_name.to_string(),
                        keyword: keyword.to_string(),
                    }
                    .into());
                }
                matched = Some(value.clone());
            }
        }
        let binding = bind_keyword_parameter(
            ctx.dialect,
            param,
            matched,
            &default_scope,
            ctx.allow_drop_arguments,
        )?;
        default_scope.extend(binding.default_scope_entries.clone());
        body_bindings.extend(binding.body_entries);
        argument_bindings.extend(binding.argument_entries);
    }
    for param in &ctx.params[ctx.aux_start..] {
        let binding = bind_aux_parameter(ctx.dialect, param, &default_scope)?;
        default_scope.extend(binding.default_scope_entries.clone());
        body_bindings.extend(binding.body_entries);
    }

    validate_unknown_keyword_arguments(
        keyword_args,
        ctx.keyword_params,
        ctx.rest_index,
        ctx.function_name,
        ctx.accepts_other_keys,
        ctx.allow_drop_arguments,
        &call_side_allow_other_keys,
    )?;

    Ok(InlineArgumentBindings {
        body_bindings,
        argument_bindings,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "leading-parameter binding threads the argument buffers plus dialect"
)]
fn bind_leading_parameters(
    dialect: Dialect,
    params: &[InlineParameter],
    raw_args: &[String],
    whole_call: &str,
    allow_drop_arguments: bool,
    body_bindings: &mut Vec<(String, String)>,
    argument_bindings: &mut Vec<(String, String)>,
    default_scope: &mut Vec<(String, String)>,
) -> InlineResult<()> {
    let mut positional_index = 0usize;
    for param in params {
        match &param.kind {
            InlineParameterKind::Whole => {
                let whole_entries = bound_parameter_argument_entries(
                    dialect,
                    param,
                    whole_call.to_owned(),
                    allow_drop_arguments,
                )?;
                default_scope.extend(whole_entries.clone());
                body_bindings.extend(whole_entries);
            }
            InlineParameterKind::Environment | InlineParameterKind::Rest => {}
            InlineParameterKind::Positional { .. } => {
                let argument = raw_args.get(positional_index).cloned();
                positional_index += 1;
                let binding = bind_positional_parameter(
                    dialect,
                    param,
                    argument,
                    default_scope,
                    allow_drop_arguments,
                )?;
                default_scope.extend(binding.default_scope_entries.clone());
                body_bindings.extend(binding.body_entries);
                argument_bindings.extend(binding.argument_entries);
            }
            InlineParameterKind::Keyword { .. } | InlineParameterKind::Aux => {}
        }
    }
    Ok(())
}

fn list_argument(arguments: &[String]) -> String {
    if arguments.is_empty() {
        return "()".to_owned();
    }

    format!("({})", arguments.join(" "))
}

pub fn bind_aux_parameter(
    dialect: Dialect,
    param: &InlineParameter,
    default_scope: &[(String, String)],
) -> InlineResult<super::types::ParameterBinding> {
    parameter_binding::bind_aux_parameter(dialect, param, default_scope)
}

pub fn destructured_binding_entries(
    dialect: Dialect,
    pattern: &super::super::definition::InlineDestructurePattern,
    argument: String,
) -> InlineResult<Vec<(String, String)>> {
    parameter_binding::destructured_binding_entries(dialect, pattern, argument)
}
