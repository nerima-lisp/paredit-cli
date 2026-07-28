use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::unwind_protect_no_cleanup::usecase::{
    UnwindProtectNoCleanupPolicy, UnwindProtectNoCleanupSummary,
};
use paredit_core_cli::args::OutputFormat;

pub fn print_unwind_protect_no_cleanup_report(
    summary: &UnwindProtectNoCleanupSummary,
    policy: &UnwindProtectNoCleanupPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!(
                "unwind_protect_form_count\t{}",
                summary.unwind_protect_form_count
            );
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "unwind_protect_form_count": summary.unwind_protect_form_count,
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
                            "form_span": {
                                "start": item.form_span.start().get(),
                                "end": item.form_span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
