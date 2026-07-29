use paredit_core_cli::CliResult;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::identical_if_branches::usecase::{IdenticalIfBranchPolicy, IdenticalIfBranchSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_identical_if_branch_report(
    summary: &IdenticalIfBranchSummary,
    policy: &IdenticalIfBranchPolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("if_form_count\t{}", summary.if_form_count);
            println!("identical_count\t{}", summary.identical.len());
            if policy.fail_on_identical {
                println!("policy\tfail_on_identical=true\tpassed={}", policy.passed);
            }
            for item in &summary.identical {
                println!(
                    "identical-if-branch\t{}\t{}\tbranch={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.branch),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "if_form_count": summary.if_form_count,
                    "identical_count": summary.identical.len(),
                    "policy": {
                        "fail_on_identical": policy.fail_on_identical,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "identical": summary.identical
                        .iter()
                        .map(|item| json!({
                            "path": item.path.display().to_string(),
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "branch": &item.branch,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
