use crate::convert_if_to_unless::usecase::plan_convert_if_to_unless;
use anyhow::Result;
pub type ConvertIfToUnlessArgs = crate::conditional_conversion::cli::ConditionalConversionArgs;
pub fn convert_if_to_unless(args: ConvertIfToUnlessArgs) -> Result<()> {
    crate::conditional_conversion::cli::run(args, plan_convert_if_to_unless)
}
