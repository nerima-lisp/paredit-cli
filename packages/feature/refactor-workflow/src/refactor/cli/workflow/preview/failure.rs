use anyhow::Result;

pub fn finish_refactor_preview_failure(
    failure_label: &'static str,
    policy_passed: bool,
    policy_message: &str,
    write_parse_refused: bool,
) -> Result<()> {
    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "{failure_label} policy failed: {policy_message}"
        )));
    }
    if write_parse_refused {
        anyhow::bail!("{failure_label} write refused because rewritten output failed to parse");
    }

    Ok(())
}
