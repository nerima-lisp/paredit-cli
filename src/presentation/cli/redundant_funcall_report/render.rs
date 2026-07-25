use anyhow::Result;
use serde_json::json;

use crate::application::usecase::redundant_funcall_report::{
    RedundantFuncallPolicy, RedundantFuncallSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_redundant_funcall_report(
    summary: &RedundantFuncallSummary,
    policy: &RedundantFuncallPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("funcall_form_count\t{}", summary.funcall_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\tcallee={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.callee),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "funcall_form_count": summary.funcall_form_count,
                    "violation_count": summary.violations.len(),
                    "policy": {
                        "fail_on_violation": policy.fail_on_violation,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "violations": summary.violations
                        .iter()
                        .map(|item| json!({
                            "path": item.path.display().to_string(),
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "callee": item.callee,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
