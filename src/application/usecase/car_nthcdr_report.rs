//! Car-nthcdr ((car (nthcdr n x)) is (nth n x)) detection.

pub use crate::domain::car_nthcdr_report::{
    CarNthcdrItem, CarNthcdrPolicy, CarNthcdrPolicyOptions, CarNthcdrSummary, collect_car_nthcdrs,
    evaluate_car_nthcdr_policy, summarize_car_nthcdrs,
};
