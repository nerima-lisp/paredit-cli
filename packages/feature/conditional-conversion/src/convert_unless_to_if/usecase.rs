//! Use case for converting `unless` to `if`.
pub use crate::conditional_sugar::domain::{
    ConditionalConversionPlan as ConvertUnlessToIfPlan,
    ConditionalConversionRequest as ConvertUnlessToIfRequest, plan_convert_unless_to_if,
};
