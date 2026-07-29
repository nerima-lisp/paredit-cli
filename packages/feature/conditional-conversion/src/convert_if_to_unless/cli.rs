use crate::convert_if_to_unless::usecase::plan_convert_if_to_unless;
use paredit_core_cli::CliResult;
pub type ConvertIfToUnlessArgs = crate::conditional_conversion::cli::ConditionalConversionArgs;
pub fn convert_if_to_unless(args: ConvertIfToUnlessArgs) -> CliResult<()> {
    crate::conditional_conversion::cli::run(args, plan_convert_if_to_unless)
}
