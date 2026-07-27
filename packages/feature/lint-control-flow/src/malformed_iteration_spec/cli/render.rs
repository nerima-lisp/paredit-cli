use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::malformed_iteration_spec::usecase::{
    MalformedIterationSpecPolicy, MalformedIterationSpecSummary,
};
use paredit_core_cli::args::OutputFormat;

pub fn print_malformed_iteration_spec_report(
    summary: &MalformedIterationSpecSummary,
    policy: &MalformedIterationSpecPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("iteration_form_count\t{}", summary.iteration_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\thead={}\telements={}\tspec={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.head),
                    item.element_count,
                    safe_text!(item.spec),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "iteration_form_count": summary.iteration_form_count,
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
                            "head": &item.head,
                            "element_count": item.element_count,
                            "spec": &item.spec,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
