use crate::rename::domain::selection::select_outermost_call_sites;

use super::WrapFunctionCallSite;

pub fn select_outermost_wrap_call_sites(
    candidates: Vec<WrapFunctionCallSite>,
) -> (Vec<WrapFunctionCallSite>, Vec<WrapFunctionCallSite>) {
    select_outermost_call_sites(candidates)
}
