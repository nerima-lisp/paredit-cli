use anyhow::Result;

use crate::application::usecase::semantic_coverage::{
    DiscoveredSemanticCoverageFile, SemanticCoverageInventory, SemanticCoveragePolicy,
    SemanticCoverageRequest, SemanticCoverageSourcePort, SemanticCoverageWorkflowError,
    build_semantic_coverage_report,
};
use crate::presentation::cli::gate;
use crate::presentation::cli::semantic_coverage_report::args::SemanticCoverageReportArgs;
use crate::presentation::cli::semantic_coverage_report::render::print_semantic_coverage_report;
use crate::presentation::cli::shared::expand_input_files;

/// Reads files this command already expanded from directories, so discovery
/// stays the CLI shell's responsibility and the usecase only turns bytes into
/// a report — the same split [`SemanticCoverageSourcePort`]'s own doc comment
/// describes.
struct CliSemanticCoverageSource;

impl SemanticCoverageSourcePort for CliSemanticCoverageSource {
    fn discover(
        &mut self,
        request: &SemanticCoverageRequest,
    ) -> anyhow::Result<SemanticCoverageInventory> {
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
) -> Result<()> {
    let paths = expand_input_files(&args.files, args.dialect)?;
    let mut source = CliSemanticCoverageSource;
    let report = build_semantic_coverage_report(
        &mut source,
        SemanticCoverageRequest {
            paths,
            dialect: args.dialect.map(Into::into),
        },
    )
    // `map_err(|e| anyhow!("{e}"))` would format through `Display` — which
    // for `Source` is the fixed string "semantic coverage source failed" —
    // into a brand-new error with no `source()` chain, discarding the real
    // cause. `.context` on the wrapped error keeps both.
    .map_err(|SemanticCoverageWorkflowError::Source(source)| {
        source.context("semantic coverage source failed")
    })?;

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
