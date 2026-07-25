use anyhow::Result;
use serde_json::json;

use crate::application::usecase::undefined_package_report::{
    UndefinedPackagePolicy, UndefinedPackageSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_undefined_package_report(
    summary: &UndefinedPackageSummary,
    policy: &UndefinedPackagePolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("in_package_count\t{}", summary.in_package_count);
            println!("undefined_count\t{}", summary.undefined.len());
            if policy.fail_on_undefined {
                println!("policy\tfail_on_undefined=true\tpassed={}", policy.passed);
            }
            for item in &summary.undefined {
                println!(
                    "undefined\t{}\t{}\t{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.name)
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "in_package_count": summary.in_package_count,
                    "undefined_count": summary.undefined.len(),
                    "policy": {
                        "fail_on_undefined": policy.fail_on_undefined,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "undefined": summary.undefined
                        .iter()
                        .map(|item| json!({
                            "path": item.path.display().to_string(),
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "name": &item.name,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
