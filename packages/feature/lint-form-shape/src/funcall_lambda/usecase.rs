//! Funcall-of-lambda (`(funcall (lambda (x) …) a)` is `((lambda (x) …) a)`)
//! detection across explicit files.

pub use crate::funcall_lambda::domain::{
    FuncallLambdaItem, FuncallLambdaPolicy, FuncallLambdaPolicyOptions, FuncallLambdaSummary,
    collect_funcall_lambdas, evaluate_funcall_lambda_policy, summarize_funcall_lambdas,
};
