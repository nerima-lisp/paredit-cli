use super::*;
use crate::application::usecase::convert_if_to_unless::plan_convert_if_to_unless;
pub(super) type ConvertIfToUnlessArgs = super::conditional_conversion::ConditionalConversionArgs;
pub(super) fn convert_if_to_unless(args: ConvertIfToUnlessArgs) -> Result<()> {
    super::conditional_conversion::run(args, plan_convert_if_to_unless)
}
