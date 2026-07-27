use crate::convert_when_to_if::usecase::plan_convert_when_to_if;
use anyhow::Result;
pub type ConvertWhenToIfArgs = crate::conditional_conversion::cli::ConditionalConversionArgs;
pub fn convert_when_to_if(args: ConvertWhenToIfArgs) -> Result<()> {
    crate::conditional_conversion::cli::run(args, plan_convert_when_to_if)
}
