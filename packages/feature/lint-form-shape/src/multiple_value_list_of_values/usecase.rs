//! Multiple-value-list-of-values ((multiple-value-list (values a b)) is (list a b)) detection.

pub use crate::domain::multiple_value_list_of_values_report::{
    MultipleValueListOfValuesItem, MultipleValueListOfValuesPolicy,
    MultipleValueListOfValuesPolicyOptions, MultipleValueListOfValuesSummary,
    collect_multiple_value_list_of_values, evaluate_multiple_value_list_of_values_policy,
    summarize_multiple_value_list_of_values,
};
