use anyhow::Result;

use crate::domain::sexpr::SymbolName;

use super::super::super::definition::{InlineParameter, InlineParameterKind};
use super::super::keyword_args::is_allow_other_keys_keyword;
use super::super::types::CallSideAllowOtherKeys;

pub(super) fn validate_unknown_keyword_arguments(
    keyword_args: &[String],
    keyword_params: &[InlineParameter],
    rest_index: Option<usize>,
    function_name: &SymbolName,
    accepts_other_keys: bool,
    allow_drop_arguments: bool,
    call_side_allow_other_keys: &CallSideAllowOtherKeys,
) -> Result<()> {
    for pair in keyword_args.chunks_exact(2) {
        let key = &pair[0];
        if !keyword_params
            .iter()
            .any(|param| matches!(&param.kind, InlineParameterKind::Keyword { keyword } if keyword == key))
        {
            if is_allow_other_keys_keyword(key) {
                continue;
            }
            if should_tolerate_unknown_keyword(
                rest_index,
                accepts_other_keys,
                allow_drop_arguments,
                call_side_allow_other_keys,
            )? {
                continue;
            }
            anyhow::bail!(
                "inline-function call for {function_name} supplies unsupported keyword {key}"
            );
        }
    }

    Ok(())
}

fn should_tolerate_unknown_keyword(
    rest_index: Option<usize>,
    accepts_other_keys: bool,
    allow_drop_arguments: bool,
    call_side_allow_other_keys: &CallSideAllowOtherKeys,
) -> Result<bool> {
    if rest_index.is_some() {
        if accepts_other_keys {
            return Ok(true);
        }
        return call_side_allows_other_keys(call_side_allow_other_keys);
    }
    if allow_drop_arguments {
        if accepts_other_keys {
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
                "inline-function cannot determine whether :allow-other-keys value {value} suppresses unknown keyword"
            );
        }
        CallSideAllowOtherKeys::AbsentOrFalse => Ok(false),
    }
}
