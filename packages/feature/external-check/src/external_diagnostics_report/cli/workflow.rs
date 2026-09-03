use std::fs;
use std::time::Duration;

use paredit_core_cli::{CliError, CommandResult};

use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::shared::{
    expand_input_files, read_input_dialect_and_tree, write_artifact_with_rollback,
};
use paredit_core_syntax::dialect::Dialect;

use super::args::ExternalDiagnosticsReportArgs;
use crate::error::ExternalCheckError;
use crate::external_diagnostics_report::domain::{
    Implementation, PlacedDiagnostic, locate_context,
};
use crate::external_diagnostics_report::usecase::{Baseline, compile_and_read, scratch_directory};

pub fn external_diagnostics_report(args: ExternalDiagnosticsReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;
    let implementation = Implementation::from(args.implementation);
    let program = args
        .implementation_path
        .clone()
        .unwrap_or_else(|| implementation.default_program().to_owned());
    let budget = Some(Duration::from_millis(args.compile_timeout_ms));

    let baseline = match args.baseline.as_deref() {
        Some(path) => {
            let text = fs::read_to_string(path).map_err(CliError::io(format!(
                "failed to read baseline {}",
                path.display()
            )))?;
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|source| CliError::Json {
                    context: format!("baseline {} is not JSON", path.display()),
                    source,
                })?;
            Some(Baseline::from_json(&value).map_err(|reason| {
                ExternalCheckError::BaselineUnusable {
                    path: path.to_path_buf(),
                    reason,
                }
            })?)
        }
        None => None,
    };

    let mut reports = Vec::with_capacity(files.len());
    let mut every_diagnostic = Vec::new();

    for (index, file) in files.iter().enumerate() {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;

        // The implementation is a Common Lisp one; a Clojure or Fennel file is
        // reported as unmodelled rather than compiled, which is the same
        // contract every Common-Lisp-only report in this tool follows.
        if dialect != Dialect::CommonLisp {
            reports.push(FileFindings::new(
                file.clone(),
                dialect,
                false,
                tree.source(),
                Vec::new(),
                Vec::new(),
            ));
            continue;
        }

        let scratch = scratch_directory(index);
        fs::create_dir_all(&scratch).map_err(CliError::io(format!(
            "failed to create scratch directory {}",
            scratch.display()
        )))?;
        let outcome = compile_and_read(implementation, &program, file, &scratch, budget);
        // The scratch directory is removed whether or not the compilation
        // worked: a fasl left in /tmp after a failed run is litter, and the
        // caller cannot be expected to know the name.
        let _ = fs::remove_dir_all(&scratch);

        let outcome = outcome.map_err(|source| ExternalCheckError::RunFailed {
            implementation: implementation.label(),
            path: file.display().to_string(),
            source,
        })?;

        // The implementation ran and said something this tool could not read
        // as diagnostics: a missing binary (the shell's 127), a heap
        // exhaustion, a `--script` that was not there. Reporting that as "no
        // findings" would be the worst possible answer — a caller gating a
        // refactor on this command would read a failed check as a passed one —
        // so it is a hard error rather than a note.
        if let Some(transcript) = outcome.unparsed_transcript {
            return Err(ExternalCheckError::NoReadableDiagnostics {
                implementation: implementation.label(),
                path: file.display().to_string(),
                exit: outcome
                    .exit_code
                    .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
                transcript,
            }
            .into());
        }
        if outcome.timed_out {
            return Err(ExternalCheckError::CompileTimedOut {
                implementation: implementation.label(),
                budget_ms: args.compile_timeout_ms,
                path: file.display().to_string(),
            }
            .into());
        }

        let findings = outcome
            .diagnostics
            .iter()
            .map(|diagnostic| PlacedDiagnostic {
                diagnostic: diagnostic.clone(),
                span: locate_context(&tree, diagnostic.context.as_deref()),
                introduced: baseline
                    .as_ref()
                    .is_some_and(|baseline| !baseline.contains(diagnostic)),
            })
            .collect::<Vec<_>>();

        every_diagnostic.extend(outcome.diagnostics);
        reports.push(FileFindings::new(
            file.clone(),
            dialect,
            true,
            tree.source(),
            findings,
            vec![
                ("timed_out", serde_json::json!(outcome.timed_out)),
                ("exit_code", serde_json::json!(outcome.exit_code)),
            ],
        ));
    }

    if let Some(path) = args.save_baseline.as_deref() {
        let baseline = Baseline::from_diagnostics(&every_diagnostic);
        write_artifact_with_rollback(
            path.to_path_buf(),
            format!("{}\n", serde_json::to_string_pretty(&baseline.to_json())?),
        )
        .map_err(|source| ExternalCheckError::BaselineWriteFailed {
            path: path.display().to_string(),
            source: Box::new(source),
        })?;
    }

    let policy = evaluate_policy(&args, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_report(
        "external-diagnostics",
        &reports,
        &policy,
        args.output,
        args.verbosity,
    )?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect external-diagnostics policy failed: {message}"
        )));
    }
    Ok(())
}

/// Decides the gate, keeping the headline count honest.
///
/// `--fail-on-introduced` fires on a *subset* of findings, so the gate is
/// evaluated over that subset and the reported `finding_count` is restored to
/// the whole set: a caller reading "3 findings, gate failed" should see three
/// findings, not the one that tripped it.
fn evaluate_policy(
    args: &ExternalDiagnosticsReportArgs,
    reports: &[FileFindings<PlacedDiagnostic>],
) -> ReportPolicy {
    let total = reports.iter().map(|report| report.findings.len()).sum();

    let mut policy = if args.fail_on_introduced {
        let introduced = reports
            .iter()
            .map(|report| report.retained(|finding| finding.introduced))
            .collect::<Vec<_>>();
        ReportPolicy::fail_on_any(Some("--fail-on-introduced"), &introduced, |report| {
            format!(
                "{} has {} diagnostic(s) absent from the baseline",
                report.path.display(),
                report.findings.len()
            )
        })
    } else if args.fail_on_diagnostics {
        ReportPolicy::fail_on_any(Some("--fail-on-diagnostics"), reports, |report| {
            format!(
                "{} has {} diagnostic(s)",
                report.path.display(),
                report.findings.len()
            )
        })
    } else {
        ReportPolicy::fail_on_any(None, reports, |report| report.path.display().to_string())
    };

    policy.finding_count = total;
    policy
}
