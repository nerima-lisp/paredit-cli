//! GiantConditionalForm (a let/let*/cond/case/typecase form exceeding a size threshold, a candidate for splitting) detection.

pub use crate::giant_conditional_form::domain::{
    GiantConditionalFormItem, GiantConditionalFormPolicy, GiantConditionalFormPolicyOptions,
    GiantConditionalFormSummary, collect_giant_conditional_form,
    evaluate_giant_conditional_form_policy, summarize_giant_conditional_form,
};
