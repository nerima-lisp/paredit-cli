//! Car-reverse ((car (reverse x)) is (car (last x))) detection.

pub use crate::domain::car_reverse_report::{
    CarReverseItem, CarReversePolicy, CarReversePolicyOptions, CarReverseSummary,
    collect_car_reverses, evaluate_car_reverse_policy, summarize_car_reverses,
};
