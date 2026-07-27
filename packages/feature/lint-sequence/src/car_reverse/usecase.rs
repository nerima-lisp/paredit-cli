//! Car-reverse ((car (reverse x)) is (car (last x))) detection.

pub use crate::car_reverse::domain::{
    CarReverseItem, CarReversePolicy, CarReversePolicyOptions, CarReverseSummary,
    collect_car_reverses, evaluate_car_reverse_policy, summarize_car_reverses,
};
