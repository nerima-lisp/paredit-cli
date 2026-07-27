use anyhow::Result;
use serde_json::json;

use crate::application::usecase::unused_nickname_report::{
    UnusedNicknamePolicy, UnusedNicknameSummary,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_unused_nickname_report(
    summary: &UnusedNicknameSummary,
    policy: &UnusedNicknamePolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("declared_count\t{}", summary.declared_count);
            println!("unused_count\t{}", summary.unused.len());
            if policy.fail_on_unused {
                println!("policy\tfail_on_unused=true\tpassed={}", policy.passed);
            }
            for item in &summary.unused {
                println!(
                    "unused\t{}\t{}\t{}:{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.package),
                    safe_text!(item.nickname)
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "declared_count": summary.declared_count,
                    "unused_count": summary.unused.len(),
                    "policy": {
                        "fail_on_unused": policy.fail_on_unused,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "unused": summary.unused
                        .iter()
                        .map(|item| json!({
                            "path": item.path.display().to_string(),
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "package": &item.package,
                            "nickname": &item.nickname,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
