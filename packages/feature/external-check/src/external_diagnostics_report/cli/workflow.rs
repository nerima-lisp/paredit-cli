use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use paredit_core_cli::{CliError, CliResult, CommandResult};

use paredit_core_cli::report::render::print_report;
use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
    write_artifact_with_rollback,
};
use paredit_core_safety::external::ExternalError;
use paredit_core_safety::external::sbcl::Diagnostic;
use paredit_core_syntax::dialect::Dialect;

use super::args::ExternalDiagnosticsReportArgs;
use crate::error::ExternalCheckError;
use crate::external_diagnostics_report::domain::{
    Implementation, PlacedDiagnostic, locate_context,
};
use crate::external_diagnostics_report::usecase::{Baseline, compile_and_read, scratch_directory};

/// One file's compile step, resolved down to what the caller does next.
///
/// `analyze_files` treats a per-file `Err` as safe to exclude-with-a-warning,
/// which is right for a file that fails to read or parse as Lisp but wrong
/// for these three: the compiler itself failing to run, producing a
/// transcript this tool cannot read as diagnostics, or timing out. Reporting
/// any of those as "no findings" would be the worst possible answer — a
/// caller gating a refactor on this command would read a failed check as a
/// passed one — so they are carried as data here and turned into the same
/// hard, whole-command error the original serial loop returned via `?`.
enum CompileOutcome {
    SkippedDialect(FileFindings<PlacedDiagnostic>),
    Compiled {
        report: FileFindings<PlacedDiagnostic>,
        diagnostics: Vec<Diagnostic>,
    },
    RunFailed {
        path: PathBuf,
        source: ExternalError,
    },
    Unreadable {
        path: PathBuf,
        exit: String,
        transcript: String,
    },
    TimedOut {
        path: PathBuf,
    },
    /// The scratch directory itself could not be created.
    ///
    /// Not one of the three outcomes `compile_and_read` can report, but the
    /// same severity: the original serial loop aborted the whole command over
    /// this too (a bare `?`, before `compile_and_read` was even called), so it
    /// stays a hard failure rather than becoming a silently-excluded file.
    ScratchDirFailed {
        source: CliError,
    },
}

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

    // Not `index` from an iterator position: `analyze_files` hands files to
    // workers in size order, not input order, so a per-file position would
    // collide across threads. Only uniqueness is needed — the scratch
    // directory is created, used, and removed within one call.
    let next_scratch_index = AtomicUsize::new(0);

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        // The implementation is a Common Lisp one; a Clojure or Fennel file is
        // reported as unmodelled rather than compiled, which is the same
        // contract every Common-Lisp-only report in this tool follows.
        if dialect != Dialect::CommonLisp {
            return CliResult::Ok(CompileOutcome::SkippedDialect(FileFindings::new(
                file.to_path_buf(),
                dialect,
                false,
                tree.source(),
                Vec::new(),
                Vec::new(),
            )));
        }

        let scratch = scratch_directory(next_scratch_index.fetch_add(1, Ordering::Relaxed));
        if let Err(source) = fs::create_dir_all(&scratch).map_err(CliError::io(format!(
            "failed to create scratch directory {}",
            scratch.display()
        ))) {
            return CliResult::Ok(CompileOutcome::ScratchDirFailed { source });
        }
        let outcome = compile_and_read(implementation, &program, file, &scratch, budget);
        // The scratch directory is removed whether or not the compilation
        // worked: a fasl left in /tmp after a failed run is litter, and the
        // caller cannot be expected to know the name.
        let _ = fs::remove_dir_all(&scratch);

        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(source) => {
                return CliResult::Ok(CompileOutcome::RunFailed {
                    path: file.to_path_buf(),
                    source,
                });
            }
        };

        if let Some(transcript) = outcome.unparsed_transcript {
            return CliResult::Ok(CompileOutcome::Unreadable {
                path: file.to_path_buf(),
                exit: outcome
                    .exit_code
                    .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
                transcript,
            });
        }
        if outcome.timed_out {
            return CliResult::Ok(CompileOutcome::TimedOut {
                path: file.to_path_buf(),
            });
        }

        let findings = outcome
            .diagnostics
            .iter()
            .map(|diagnostic| PlacedDiagnostic {
                diagnostic: diagnostic.clone(),
                span: locate_context(tree, diagnostic.context.as_deref()),
                introduced: baseline
                    .as_ref()
                    .is_some_and(|baseline| !baseline.contains(diagnostic)),
            })
            .collect::<Vec<_>>();

        let report = FileFindings::new(
            file.to_path_buf(),
            dialect,
            true,
            tree.source(),
            findings,
            vec![
                ("timed_out", serde_json::json!(outcome.timed_out)),
                ("exit_code", serde_json::json!(outcome.exit_code)),
            ],
        );

        CliResult::Ok(CompileOutcome::Compiled {
            report,
            diagnostics: outcome.diagnostics,
        })
    });

    // A file that fails to read or parse as Lisp is excluded with a warning,
    // same as every other report built on `analyze_files` — that failure has
    // nothing to do with whether the compiler ran cleanly. It cannot make the
    // whole command fail unless every file failed that way.
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);

    let mut reports = Vec::with_capacity(analysis.succeeded.len());
    let mut every_diagnostic = Vec::new();
    // First bad file *in file order* wins, matching the serial loop this
    // replaced: `analysis.succeeded` preserves input order, so the first of
    // these three seen while walking it is the one reported, and later ones
    // are dropped exactly as an early `?` would have dropped them.
    let mut hard_failure: Option<CliError> = None;

    for outcome in analysis.succeeded {
        match outcome {
            CompileOutcome::SkippedDialect(report) => reports.push(report),
            CompileOutcome::Compiled {
                report,
                diagnostics,
            } => {
                every_diagnostic.extend(diagnostics);
                reports.push(report);
            }
            CompileOutcome::RunFailed { path, source } => {
                hard_failure.get_or_insert(
                    ExternalCheckError::RunFailed {
                        implementation: implementation.label(),
                        path: path.display().to_string(),
                        source,
                    }
                    .into(),
                );
            }
            CompileOutcome::Unreadable {
                path,
                exit,
                transcript,
            } => {
                hard_failure.get_or_insert(
                    ExternalCheckError::NoReadableDiagnostics {
                        implementation: implementation.label(),
                        path: path.display().to_string(),
                        exit,
                        transcript,
                    }
                    .into(),
                );
            }
            CompileOutcome::TimedOut { path } => {
                hard_failure.get_or_insert(
                    ExternalCheckError::CompileTimedOut {
                        implementation: implementation.label(),
                        budget_ms: args.compile_timeout_ms,
                        path: path.display().to_string(),
                    }
                    .into(),
                );
            }
            CompileOutcome::ScratchDirFailed { source } => {
                hard_failure.get_or_insert(source);
            }
        }
    }

    if let Some(failure) = hard_failure {
        return Err(failure.into());
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
