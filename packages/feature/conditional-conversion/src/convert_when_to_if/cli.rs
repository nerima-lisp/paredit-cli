use crate::convert_when_to_if::usecase::plan_convert_when_to_if;
use paredit_core_cli::CliResult;
pub type ConvertWhenToIfArgs = crate::conditional_conversion::cli::ConditionalConversionArgs;
pub fn convert_when_to_if(args: ConvertWhenToIfArgs) -> CliResult<()> {
    crate::conditional_conversion::cli::run(args, plan_convert_when_to_if)
}
