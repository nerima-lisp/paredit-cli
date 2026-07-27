use super::super::super::super::*;

pub(in crate::presentation::cli::refactor::workflow) fn finish_refactor_preview_failure(
    failure_label: &'static str,
    policy_passed: bool,
    policy_message: &str,
    write_parse_refused: bool,
) -> Result<()> {
    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "{failure_label} policy failed: {policy_message}"
        )));
    }
    if write_parse_refused {
        anyhow::bail!("{failure_label} write refused because rewritten output failed to parse");
    }

    Ok(())
}
