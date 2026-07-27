use crate::domain::rename::selection::select_outermost_call_sites;

use super::UnwrapFunctionCallSite;

pub(super) fn select_outermost_unwrap_call_sites(
    candidates: Vec<UnwrapFunctionCallSite>,
) -> (Vec<UnwrapFunctionCallSite>, Vec<UnwrapFunctionCallSite>) {
    select_outermost_call_sites(candidates)
}
