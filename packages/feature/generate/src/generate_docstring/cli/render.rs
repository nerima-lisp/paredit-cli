use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::generate_docstring::usecase::DocstringOutcome;

pub fn print_docstring_plan(
    outcome: &DocstringOutcome,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => match outcome {
            DocstringOutcome::Ready(plan) => {
                println!("status\tready");
                println!("name\t{}", safe_text!(plan.name));
                println!("inserted\t{}", safe_text!(plan.insertion_text.trim()));
                println!("written\t{written}");
            }
            DocstringOutcome::AlreadyDocumented => {
                println!("status\talready-documented");
                println!("written\t{written}");
            }
            DocstringOutcome::Unsupported { .. } => unreachable!("refused before printing"),
        },
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&match outcome {
                DocstringOutcome::Ready(plan) => json!({
                    "schema_version": 1,
                    "report": "generate docstring",
                    "status": "ready",
                    "name": plan.name,
                    "inserted": plan.insertion_text,
                    "written": written,
                }),
                DocstringOutcome::AlreadyDocumented => json!({
                    "schema_version": 1,
                    "report": "generate docstring",
                    "status": "already-documented",
                    "written": written,
                }),
                DocstringOutcome::Unsupported { .. } => unreachable!("refused before printing"),
            })?
        ),
    }

    Ok(())
}
