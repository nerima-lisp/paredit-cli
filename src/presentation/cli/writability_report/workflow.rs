use paredit_core_cli::CliResult;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::shared::check_writable;

use super::args::WritabilityReportArgs;

/// Reports whether a write to `args.file` would succeed, without writing
/// anything. See [`check_writable`] for what "would succeed" reuses from the
/// real write path.
pub(in crate::presentation::cli) fn writability(args: WritabilityReportArgs) -> CliResult<()> {
    let display_path = args.file.display().to_string();
    let check = check_writable(args.file);

    match args.output {
        OutputFormat::Text => {
            if check.writable {
                println!("writable: {display_path}");
            } else {
                println!(
                    "not writable: {display_path}: {}",
                    check.reason.as_deref().unwrap_or("unknown reason")
                );
            }
        }
        OutputFormat::Json => {
            let report = json!({
                "schema_version": 1,
                "status": if check.writable { "ok" } else { "error" },
                "file": display_path,
                "target_existed": check.target_existed,
                "writable": check.writable,
                "reason": check.reason,
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    if check.writable {
        Ok(())
    } else {
        Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::RefusalWriteTarget,
            format!(
                "{display_path} is not writable: {}",
                check.reason.unwrap_or_else(|| "unknown reason".to_owned())
            ),
        )
        .into())
    }
}
