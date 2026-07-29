use std::path::Path;

use paredit_core_cli::CliResult;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::generate_tests::usecase::TestStub;

pub fn print_test_stub_plan(
    stubs: &[TestStub],
    skipped_dialect: &[std::path::PathBuf],
    written: bool,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("stub_count\t{}", stubs.len());
            println!("skipped_dialect_count\t{}", skipped_dialect.len());
            println!("written\t{written}");
            for stub in stubs {
                println!(
                    "stub\t{}\t{}",
                    safe_text!(stub.subject),
                    safe_text!(stub.generated.trim_end())
                );
            }
            for file in skipped_dialect {
                println!("skipped\t{}", safe_text!(path_text(file)));
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "report": "generate tests",
                "stub_count": stubs.len(),
                "written": written,
                "stubs": stubs
                    .iter()
                    .map(|stub| json!({
                        "subject": stub.subject,
                        "generated": stub.generated,
                    }))
                    .collect::<Vec<_>>(),
                "skipped_dialect": skipped_dialect
                    .iter()
                    .map(|file| path_text(file))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}
