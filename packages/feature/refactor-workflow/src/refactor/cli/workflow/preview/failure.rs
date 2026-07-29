use paredit_core_cli::CommandResult;

pub fn finish_refactor_preview_failure(
    failure_label: &'static str,
    policy_passed: bool,
    policy_message: &str,
    write_parse_refused: bool,
) -> CommandResult {
    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "{failure_label} policy failed: {policy_message}"
        )));
    }
    if write_parse_refused {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::RefusalRewriteDoesNotReparse,
            format!("{failure_label} write refused because rewritten output failed to parse"),
        )
        .into());
    }

    Ok(())
}
