//! Lambda-list-keyword-order (keywords out of the canonical &optional/&rest/
//! &key/&allow-other-keys/&aux order) detection across explicit files.

pub use crate::domain::lambda_list_keyword_order_report::{
    LambdaListKeywordOrderItem, LambdaListKeywordOrderPolicy, LambdaListKeywordOrderPolicyOptions,
    LambdaListKeywordOrderSummary, collect_lambda_list_keyword_order,
    evaluate_lambda_list_keyword_order_policy, summarize_lambda_list_keyword_order,
};
