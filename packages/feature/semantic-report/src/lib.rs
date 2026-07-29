#![doc = include_str!("../README.md")]

pub mod constant_report;
pub mod effect_report;
pub mod fold_constants;
pub mod narrowing_report;
pub mod type_report;
pub mod value_propagation_report;

mod shared;

// The composition root sees each slice's Args type and run fn (section 4.2).
pub use constant_report::cli::{ConstantReportArgs, constant_report};
pub use effect_report::cli::{EffectReportArgs, effect_report};
pub use fold_constants::cli::{FoldConstantsArgs, fold_constants};
pub use narrowing_report::cli::{NarrowingReportArgs, narrowing_report};
pub use type_report::cli::{TypeReportArgs, type_report};
pub use value_propagation_report::cli::{ValuePropagationReportArgs, value_propagation_report};
