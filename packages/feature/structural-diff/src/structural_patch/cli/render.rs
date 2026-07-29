use paredit_core_cli::CliResult;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::structural_patch::usecase::{Outcome, PatchPlan};

/// What the run was pointed at, so the plan says which three files it means.
#[derive(Debug, Clone)]
pub struct PatchSubjects {
    pub from: String,
    pub to: String,
    pub apply_to: String,
    pub written: bool,
}

pub fn print_patch_plan(
    plan: &PatchPlan,
    subjects: &PatchSubjects,
    output: OutputFormat,
) -> CliResult<()> {
    let outcomes = [
        Outcome::Applied,
        Outcome::NotFound,
        Outcome::Ambiguous,
        Outcome::Unportable,
    ];

    match output {
        OutputFormat::Text => {
            println!("from\t{}", safe_text!(subjects.from));
            println!("to\t{}", safe_text!(subjects.to));
            println!("apply_to\t{}", safe_text!(subjects.apply_to));
            println!("change_count\t{}", plan.resolutions.len());
            for outcome in outcomes {
                println!("{}\t{}", outcome.label(), plan.count_with(outcome));
            }
            println!("written\t{}", subjects.written);
            for resolution in &plan.resolutions {
                println!(
                    "{}\t{}\t{}\thead={}\tsites={}\twidened={}",
                    resolution.outcome.label(),
                    resolution.kind.label(),
                    safe_text!(resolution.source_path),
                    safe_text!(resolution.head.clone().unwrap_or_default()),
                    resolution.targets.len(),
                    resolution.widened,
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "report": "refactor patch",
                "from": subjects.from,
                "to": subjects.to,
                "apply_to": subjects.apply_to,
                "written": subjects.written,
                "change_count": plan.resolutions.len(),
                "by_outcome": outcomes
                    .iter()
                    .map(|outcome| json!({
                        "outcome": outcome.label(),
                        "count": plan.count_with(*outcome),
                    }))
                    .collect::<Vec<_>>(),
                "changes": plan.resolutions
                    .iter()
                    .map(|resolution| json!({
                        "outcome": resolution.outcome.label(),
                        "kind": resolution.kind.label(),
                        "source_path": resolution.source_path,
                        "head": resolution.head,
                        "before": resolution.before,
                        "after": resolution.after,
                        "anchor": resolution.anchor,
                        "anchor_widened": resolution.widened,
                        "sites": resolution.targets
                            .iter()
                            .map(|span| json!({
                                "start": span.start().get(),
                                "end": span.end().get(),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}
