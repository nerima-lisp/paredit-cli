use super::*;
use crate::application::usecase::convert_when_to_if::plan_convert_when_to_if;
pub(super) type ConvertWhenToIfArgs = super::conditional_conversion::ConditionalConversionArgs;
pub(super) fn convert_when_to_if(args: ConvertWhenToIfArgs) -> Result<()> {
    super::conditional_conversion::run(args, plan_convert_when_to_if)
}
