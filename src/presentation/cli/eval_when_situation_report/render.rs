use anyhow::Result;
use serde_json::json;

use crate::application::usecase::eval_when_situation_report::{
    EvalWhenSituationPolicy, EvalWhenSituationSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_eval_when_situation_report(
    summary: &EvalWhenSituationSummary,
    policy: &EvalWhenSituationPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("eval_when_form_count\t{}", summary.eval_when_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\tsituation={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.situation),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "eval_when_form_count": summary.eval_when_form_count,
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
                            "situation": &item.situation,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
