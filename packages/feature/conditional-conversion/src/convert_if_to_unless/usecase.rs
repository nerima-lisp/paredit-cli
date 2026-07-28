//! Use case for converting `if` to `unless`.
pub use crate::conditional_sugar::domain::{
    ConditionalConversionPlan as ConvertIfToUnlessPlan,
    ConditionalConversionRequest as ConvertIfToUnlessRequest, plan_convert_if_to_unless,
};
