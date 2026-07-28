use crate::error::{CallBindingError, InlineResult};

use paredit_core_syntax::sexpr::SymbolName;

use super::super::super::definition::{InlineParameter, InlineParameterKind};
use super::super::keyword_args::is_allow_other_keys_keyword;
use super::super::types::CallSideAllowOtherKeys;

pub fn validate_unknown_keyword_arguments(
    keyword_args: &[String],
    keyword_params: &[InlineParameter],
    rest_index: Option<usize>,
    function_name: &SymbolName,
    accepts_other_keys: bool,
    allow_drop_arguments: bool,
    call_side_allow_other_keys: &CallSideAllowOtherKeys,
) -> InlineResult<()> {
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
            return Err(CallBindingError::UnsupportedKeyword {
                function: function_name.to_string(),
                keyword: key.to_string(),
            }
            .into());
        }
    }

    Ok(())
}

fn should_tolerate_unknown_keyword(
    rest_index: Option<usize>,
    accepts_other_keys: bool,
    allow_drop_arguments: bool,
    call_side_allow_other_keys: &CallSideAllowOtherKeys,
) -> InlineResult<bool> {
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
) -> InlineResult<bool> {
    match call_side_allow_other_keys {
        CallSideAllowOtherKeys::True => Ok(true),
        CallSideAllowOtherKeys::Unknown(value) => Err(CallBindingError::AllowOtherKeysNotLiteral {
            qualifier: String::new(),
            value: value.to_string(),
        }
        .into()),
        CallSideAllowOtherKeys::AbsentOrFalse => Ok(false),
    }
}
