use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::generate_accessors::usecase::AccessorsOutcome;

pub fn print_accessors_plan(
    outcome: &AccessorsOutcome,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => match outcome {
            AccessorsOutcome::Ready {
                class_name,
                edits,
                already_had_one,
            } => {
                println!("status\tready");
                println!("class\t{}", safe_text!(class_name));
                println!("edited_count\t{}", edits.len());
                println!("already_had_one\t{already_had_one}");
                println!("written\t{written}");
                for edit in edits {
                    println!("slot\t{}", safe_text!(edit.slot_name));
                }
            }
            AccessorsOutcome::Nothing { class_name } => {
                println!("status\tnothing-to-do");
                println!("class\t{}", safe_text!(class_name));
                println!("written\t{written}");
            }
            AccessorsOutcome::Unsupported { .. } => unreachable!("refused before printing"),
        },
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&match outcome {
                AccessorsOutcome::Ready {
                    class_name,
                    edits,
                    already_had_one,
                } => json!({
                    "schema_version": 1,
                    "report": "generate accessors",
                    "status": "ready",
                    "class": class_name,
                    "already_had_one": already_had_one,
                    "written": written,
                    "slots": edits.iter().map(|edit| json!({
                        "slot": edit.slot_name,
                        "replacement": edit.replacement,
                    })).collect::<Vec<_>>(),
                }),
                AccessorsOutcome::Nothing { class_name } => json!({
                    "schema_version": 1,
                    "report": "generate accessors",
                    "status": "nothing-to-do",
                    "class": class_name,
                    "written": written,
                }),
                AccessorsOutcome::Unsupported { .. } => unreachable!("refused before printing"),
            })?
        ),
    }

    Ok(())
}
