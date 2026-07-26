use crate::domain::sexpr::ExpressionView;

use super::types::CallSideAllowOtherKeys;

pub(super) fn call_side_allow_other_keys_from_strings(
    keyword_args: &[String],
) -> CallSideAllowOtherKeys {
    for pair in keyword_args.chunks_exact(2) {
        if is_allow_other_keys_keyword(&pair[0]) {
            return parse_call_side_allow_other_keys_value(&pair[1]);
        }
    }
    CallSideAllowOtherKeys::AbsentOrFalse
}

pub(super) fn call_side_allow_other_keys_from_views(
    keyword_args: &[ExpressionView],
    source: &str,
) -> CallSideAllowOtherKeys {
    for pair in keyword_args.chunks_exact(2) {
        if is_allow_other_keys_keyword(pair[0].span.slice(source)) {
            return parse_call_side_allow_other_keys_value(pair[1].span.slice(source));
        }
    }
    CallSideAllowOtherKeys::AbsentOrFalse
}

pub(super) fn is_allow_other_keys_keyword(key: &str) -> bool {
    key.eq_ignore_ascii_case(":allow-other-keys")
}

fn parse_call_side_allow_other_keys_value(value: &str) -> CallSideAllowOtherKeys {
    let normalized = value.trim();
    if normalized.eq_ignore_ascii_case("nil")
        || normalized.eq_ignore_ascii_case("cl:nil")
        || normalized.eq_ignore_ascii_case("common-lisp:nil")
    {
        return CallSideAllowOtherKeys::AbsentOrFalse;
    }
    if normalized.eq_ignore_ascii_case("t")
        || normalized.eq_ignore_ascii_case("cl:t")
        || normalized.eq_ignore_ascii_case("common-lisp:t")
    {
        return CallSideAllowOtherKeys::True;
    }
    CallSideAllowOtherKeys::Unknown(normalized.to_owned())
}
