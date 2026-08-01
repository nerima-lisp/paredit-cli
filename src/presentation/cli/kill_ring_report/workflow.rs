use std::path::Path;

use paredit_core_cli::CliResult;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::diagnosis::ErrorCode;
use paredit_core_cli::error::FeatureRefusal;
use paredit_core_cli::kill_ring::{KillRing, ring_path, write_ring};
use paredit_core_cli::shared::read_file_or_empty;
use serde_json::json;

use super::args::KillRingReportArgs;
use super::domain::{Corruption, RingStatus, classify};

/// Diagnoses the kill ring file, optionally resetting it when it is
/// corrupted. See [`super::domain::classify`] for what "corrupted" means.
pub(in crate::presentation::cli) fn kill_ring(args: KillRingReportArgs) -> CliResult<()> {
    let path = ring_path(args.ring.clone());
    let (input, existed) = read_file_or_empty(&path)?;
    let status = classify(existed, &input.text);

    // `--repair-reset` only ever fires once this run has classified the ring
    // as corrupted: a missing file is already `Valid`, and so is a
    // well-formed one, so this can never discard real entries.
    let (reported, repaired) = match (&status, args.repair_reset) {
        (RingStatus::Corrupted(_), true) => {
            write_ring(&path, &KillRing::default())?;
            (RingStatus::Valid { entry_count: 0 }, true)
        }
        _ => (status, false),
    };

    print_report(&path, &reported, repaired, args.output)?;

    match &reported {
        RingStatus::Valid { .. } => Ok(()),
        RingStatus::Corrupted(corruption) => Err(FeatureRefusal::message(
            match corruption {
                Corruption::InvalidJson(_) => ErrorCode::InputUnparsable,
                Corruption::MissingShape | Corruption::SchemaVersionMismatch { .. } => {
                    ErrorCode::InputShapeRefused
                }
            },
            format!(
                "kill ring {} is corrupted: {}. Re-run with --repair-reset to discard it \
                 and start over.",
                path.display(),
                corruption.message()
            ),
        )
        .into()),
    }
}

fn print_report(
    path: &Path,
    status: &RingStatus,
    repaired: bool,
    output: OutputFormat,
) -> CliResult<()> {
    let display_path = path.display().to_string();
    match output {
        OutputFormat::Text => match status {
            RingStatus::Valid { entry_count } if repaired => {
                println!("kill ring {display_path} reset: {entry_count} entries");
            }
            RingStatus::Valid { entry_count } => {
                println!("kill ring {display_path} ok: {entry_count} entries");
            }
            RingStatus::Corrupted(corruption) => {
                println!(
                    "kill ring {display_path} corrupted: {}",
                    corruption.message()
                );
            }
        },
        OutputFormat::Json => {
            let (status_word, entry_count, corruption) = match status {
                RingStatus::Valid { entry_count } => ("ok", Some(*entry_count), None),
                RingStatus::Corrupted(corruption) => (
                    "error",
                    None,
                    Some(json!({ "code": corruption.code(), "detail": corruption.message() })),
                ),
            };
            let report = json!({
                "schema_version": 1,
                "status": status_word,
                "ring": display_path,
                "repaired": repaired,
                "entry_count": entry_count,
                "corruption": corruption,
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }
    Ok(())
}
