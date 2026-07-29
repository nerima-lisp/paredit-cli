//! `query find` over one file, and the gate over the whole run.

pub use crate::find_report::domain::{DEFAULT_PREVIEW_BYTES, PatternHit};

use std::collections::HashMap;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::{LineIndex, Pattern, match_all, stable_selector_ids};
use paredit_core_syntax::sexpr::{ExpressionPath, SyntaxTree};
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
    // Indexed rather than scanned. `stable_selector_ids` returns one entry per
    // form in the file, and looking each match up with a linear `find` over
    // that — comparing an `ExpressionPath`, which is a `Vec`, every step — is
    // quadratic in a file's size. It showed: a 700 KB source took two seconds
    // to *report* matches that took 70 ms to rewrite.
    let all_ids = stable_selector_ids(tree, dialect);
    let ids: HashMap<&ExpressionPath, &str> = all_ids
        .iter()
        .map(|(path, id)| (path, id.as_str()))
        .collect();

    let hits: Vec<PatternHit> = match_all(tree, pattern, dialect)
        .iter()
        .map(|found| {
            let id = ids.get(&found.path).map(|id| (*id).to_owned());
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
