use super::*;
use crate::application::usecase::convert_if_to_when::plan_convert_if_to_when;
pub(super) type ConvertIfToWhenArgs = super::conditional_conversion::ConditionalConversionArgs;
pub(super) fn convert_if_to_when(args: ConvertIfToWhenArgs) -> Result<()> {
    super::conditional_conversion::run(args, plan_convert_if_to_when)
}
