use crate::rename::domain::selection::select_outermost_call_sites;

use super::UnwrapFunctionCallSite;

pub fn select_outermost_unwrap_call_sites(
    candidates: Vec<UnwrapFunctionCallSite>,
) -> (Vec<UnwrapFunctionCallSite>, Vec<UnwrapFunctionCallSite>) {
    select_outermost_call_sites(candidates)
}
