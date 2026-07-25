//! Sharp-quoted-`lambda` (`#'(lambda …)` is `(lambda …)`) detection across
//! explicit files.

pub use crate::domain::sharp_quoted_lambda_report::{
    SharpQuotedLambdaItem, SharpQuotedLambdaPolicy, SharpQuotedLambdaPolicyOptions,
    SharpQuotedLambdaSummary, collect_sharp_quoted_lambdas, evaluate_sharp_quoted_lambda_policy,
    summarize_sharp_quoted_lambdas,
};
