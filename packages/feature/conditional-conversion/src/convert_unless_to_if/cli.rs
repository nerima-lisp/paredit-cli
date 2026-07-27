use super::*;
use crate::application::usecase::convert_unless_to_if::plan_convert_unless_to_if;
pub(super) type ConvertUnlessToIfArgs = super::conditional_conversion::ConditionalConversionArgs;
pub(super) fn convert_unless_to_if(args: ConvertUnlessToIfArgs) -> Result<()> {
    super::conditional_conversion::run(args, plan_convert_unless_to_if)
}
