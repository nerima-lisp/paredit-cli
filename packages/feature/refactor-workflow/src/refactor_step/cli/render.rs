use paredit_core_cli::CliResult;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::refactor_step::domain::{Selection, Step};

pub fn print_steps(
    steps: &[Step],
    selection: &Selection,
    written: &[String],
    wrote: bool,
    output: OutputFormat,
) -> CliResult<()> {
    let skipped = steps.len() - selection.count();
    match output {
        OutputFormat::Text => {
            println!("step_count\t{}", steps.len());
            println!("selected\t{}", selection.count());
            println!("skipped\t{skipped}");
            println!("written\t{}", written.len());
            for step in steps {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    if selection.contains(step.number) {
                        "take"
                    } else {
                        "leave"
                    },
                    step.number,
                    safe_text!(step.path),
                    step.line,
                    safe_text!(step.before),
                    safe_text!(step.after),
                );
            }
            for path in written {
                println!("wrote\t{}", safe_text!(path));
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "report": "refactor step",
                "step_count": steps.len(),
                "selected": selection.count(),
                "skipped": skipped,
                // Whether a write was *asked for*, separate from what it did.
                // A run that selected nothing and one that was only listing
                // both write no files, and a caller has to tell them apart.
                "write_requested": wrote,
                "written": written,
                "steps": steps
                    .iter()
                    .map(|step| json!({
                        "number": step.number,
                        "selected": selection.contains(step.number),
                        "path": step.path,
                        "line": step.line,
                        "span": {
                            "start": step.span.start().get(),
                            "end": step.span.end().get(),
                        },
                        "before": step.before,
                        "after": step.after,
                        "context": step.context,
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}
