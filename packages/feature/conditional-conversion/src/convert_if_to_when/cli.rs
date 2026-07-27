use crate::convert_if_to_when::usecase::plan_convert_if_to_when;
use anyhow::Result;
pub type ConvertIfToWhenArgs = crate::conditional_conversion::cli::ConditionalConversionArgs;
pub fn convert_if_to_when(args: ConvertIfToWhenArgs) -> Result<()> {
    crate::conditional_conversion::cli::run(args, plan_convert_if_to_when)
}
