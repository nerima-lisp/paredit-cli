use crate::convert_if_to_when::usecase::plan_convert_if_to_when;
use paredit_core_cli::CliResult;
pub type ConvertIfToWhenArgs = crate::conditional_conversion::cli::ConditionalConversionArgs;
pub fn convert_if_to_when(args: ConvertIfToWhenArgs) -> CliResult<()> {
    crate::conditional_conversion::cli::run(args, plan_convert_if_to_when)
}
