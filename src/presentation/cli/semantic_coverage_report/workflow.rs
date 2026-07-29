use paredit_core_cli::CommandResult;

use crate::presentation::cli::gate;
use crate::presentation::cli::semantic_coverage_report::args::SemanticCoverageReportArgs;
use crate::presentation::cli::semantic_coverage_report::render::print_semantic_coverage_report;
use crate::presentation::cli::shared::expand_input_files;
use crate::semantic_coverage::{
    DiscoveredSemanticCoverageFile, SemanticCoverageInventory, SemanticCoveragePolicy,
    SemanticCoverageRequest, SemanticCoverageSourcePort, SemanticCoverageWorkflowError,
    build_semantic_coverage_report,
};

/// Reads files this command already expanded from directories, so discovery
/// stays the CLI shell's responsibility and the usecase only turns bytes into
/// a report — the same split [`SemanticCoverageSourcePort`]'s own doc comment
/// describes.
struct CliSemanticCoverageSource;

impl SemanticCoverageSourcePort for CliSemanticCoverageSource {
    type Error = paredit_core_cli::CliError;

    fn discover(
        &mut self,
        request: &SemanticCoverageRequest,
    ) -> Result<SemanticCoverageInventory, Self::Error> {
        Ok(SemanticCoverageInventory {
            files: request
                .paths
                .iter()
                .cloned()
                .map(|path| DiscoveredSemanticCoverageFile { path })
                .collect(),
        })
    }

    fn load(&self, file: &DiscoveredSemanticCoverageFile) -> std::result::Result<Vec<u8>, String> {
        std::fs::read(&file.path).map_err(|error| error.to_string())
    }
}

pub(in crate::presentation::cli) fn semantic_coverage_report(
    args: SemanticCoverageReportArgs,
) -> CommandResult {
    let paths = expand_input_files(&args.files, args.dialect)?;
    let mut source = CliSemanticCoverageSource;
    let report = build_semantic_coverage_report(
        &mut source,
        SemanticCoverageRequest {
            paths,
            dialect: args.dialect.map(Into::into),
        },
    )
    // The wrapper this used to add existed to work around `anyhow`: `Display`
    // for `Source` was the fixed string "semantic coverage source failed", so
    // `.context` was needed to keep the real cause reachable. The variant now
    // carries a `CliError`, which already renders its own chain and carries a
    // classification, so unwrapping it is strictly more informative.
    .map_err(|SemanticCoverageWorkflowError::Source(source)| *source)?;

    let policy = SemanticCoveragePolicy::evaluate(args.fail_under, &report);
    let policy_passed = policy.passed;
    let policy_message = policy.message.clone();

    print_semantic_coverage_report(&report, &policy, args.top, args.output)?;

    if !policy_passed {
        return Err(gate::gate_failure(format!(
            "inspect semantic-coverage policy failed: {}",
            policy_message.unwrap_or_default()
        )));
    }

    Ok(())
}
