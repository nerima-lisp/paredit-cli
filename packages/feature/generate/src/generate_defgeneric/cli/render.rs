use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::generate_defgeneric::usecase::Candidate;

pub fn print_defgeneric_plan(
    candidates: &[Candidate],
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    let ready = candidates
        .iter()
        .filter(|candidate| matches!(candidate, Candidate::Ready(_)))
        .count();

    match output {
        OutputFormat::Text => {
            println!("candidate_count\t{}", candidates.len());
            println!("ready_count\t{ready}");
            println!("written\t{written}");
            for candidate in candidates {
                match candidate {
                    Candidate::Ready(generic) => println!(
                        "ready\t{}\trequired_arity={}\tmethods={}\t{}",
                        safe_text!(generic.name),
                        generic.required_arity,
                        generic.method_count,
                        safe_text!(generic.generated.trim_end())
                    ),
                    Candidate::Unready(generic) => println!(
                        "unready\t{}\tmethods={}\treason={}",
                        safe_text!(generic.name),
                        generic.method_count,
                        safe_text!(generic.reason)
                    ),
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "report": "generate defgeneric",
                "candidate_count": candidates.len(),
                "ready_count": ready,
                "written": written,
                "candidates": candidates
                    .iter()
                    .map(|candidate| match candidate {
                        Candidate::Ready(generic) => json!({
                            "name": generic.name,
                            "status": "ready",
                            "required_arity": generic.required_arity,
                            "method_count": generic.method_count,
                            "generated": generic.generated,
                        }),
                        Candidate::Unready(generic) => json!({
                            "name": generic.name,
                            "status": "unready",
                            "method_count": generic.method_count,
                            "reason": generic.reason,
                        }),
                    })
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}
