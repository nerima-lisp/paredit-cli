//! `query find` over one file, and the gate over the whole run.

pub use crate::find_report::domain::{DEFAULT_PREVIEW_BYTES, PatternHit};

use std::path::Path;

use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::{LineIndex, Pattern, match_all, stable_selector_ids};
use paredit_core_syntax::sexpr::SyntaxTree;
use serde_json::json;

/// Every form in `tree` that `pattern` matches.
#[must_use]
pub fn build_find_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    pattern: &Pattern,
    preview_bytes: usize,
) -> FileFindings<PatternHit> {
    let source = tree.source();
    let index = LineIndex::new(source);
    // One pass for the whole file: an id cannot be computed for one form
    // alone, since its ordinal depends on the forms around it.
    let ids = stable_selector_ids(tree, dialect);

    let hits: Vec<PatternHit> = match_all(tree, pattern, dialect)
        .iter()
        .map(|found| {
            let id = ids
                .iter()
                .find(|(path, _)| *path == found.path)
                .map(|(_, id)| id.as_str().to_owned());
            PatternHit::new(found, source, &index, id, preview_bytes)
        })
        .collect();

    let summary = vec![("match_count", json!(hits.len()))];
    // Pattern matching reads the balanced-parens tree and the dialect's own
    // reader, both of which every dialect this build parses provides. There is
    // no dialect for which a match means less than it does here, so the
    // envelope's "not modelled" notice would be a false warning.
    FileFindings::new(path.to_path_buf(), dialect, true, hits, summary)
}

/// Evaluates the two gates, which point in opposite directions.
///
/// `--fail-on-match` is the "this must not appear" gate: a banned form, a
/// deprecated call. `--fail-on-no-match` is "this must appear": a required
/// header, a registration the build depends on. Both exist because a pattern
/// language with no gate is a search tool, and CI cannot use a search tool.
#[must_use]
pub fn evaluate_find_policy(
    fail_on_match: bool,
    fail_on_no_match: bool,
    reports: &[FileFindings<PatternHit>],
) -> ReportPolicy {
    let total: usize = reports.iter().map(|report| report.findings.len()).sum();
    if fail_on_no_match {
        return ReportPolicy {
            gate: Some("--fail-on-no-match"),
            finding_count: total,
            passed: total > 0,
            violations: if total == 0 {
                vec!["no file matches the pattern".to_owned()]
            } else {
                Vec::new()
            },
        };
    }
    ReportPolicy::fail_on_any(
        fail_on_match.then_some("--fail-on-match"),
        reports,
        |report| {
            format!(
                "{} has {} match(es)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
