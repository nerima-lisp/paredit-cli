use anyhow::Result;
use serde_json::json;

use crate::application::usecase::constant_when_test_report::{
    ConstantWhenTestPolicy, ConstantWhenTestSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_constant_when_test_report(
    summary: &ConstantWhenTestSummary,
    policy: &ConstantWhenTestPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("when_form_count\t{}", summary.when_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\t{}\t{}\t{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    item.head,
                    item.test,
                    if item.always_runs { "progn" } else { "dead" },
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "when_form_count": summary.when_form_count,
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
                            "head": item.head,
                            "test": item.test,
                            "always_runs": item.always_runs,
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
