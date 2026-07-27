use anyhow::Result;
use crate::convert_unless_to_if::usecase::plan_convert_unless_to_if;
pub type ConvertUnlessToIfArgs = crate::conditional_conversion::cli::ConditionalConversionArgs;
pub fn convert_unless_to_if(args: ConvertUnlessToIfArgs) -> Result<()> {
    crate::conditional_conversion::cli::run(args, plan_convert_unless_to_if)
}
