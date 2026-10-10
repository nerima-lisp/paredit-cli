use std::path::PathBuf;

use clap::Args;
use paredit_core_cli::CommandResult;
use paredit_core_cli::args::{DialectArg, OutputFormat};
use paredit_core_cli::shared::{
    MAX_SOURCE_INPUT_BYTES, read_input_dialect_tree_and_expected_target, read_text_file_with_limit,
    stable_text_hash, terminal_safe, unified_diff, write_files_with_rollback_expected,
};

use super::domain::{BulkRequest, plan_bulk};

/// Apply disjoint structural operations to one original source snapshot.
/// Syntactic rename does not resolve bindings or guarantee semantic equivalence.
#[derive(Debug, Args)]
pub struct RefactorBulkArgs {
    pub file: PathBuf,
    /// Strict version-1 JSON with input_hash and typed operations. Paths refer
    /// to the original file. Supported kinds: wrap and rename-in-form.
    #[arg(long, value_name = "FILE")]
    pub request: PathBuf,
    #[arg(long, value_enum)]
    pub dialect: Option<DialectArg>,
    /// Print a validated unified diff without writing.
    #[arg(long, conflicts_with = "write")]
    pub diff: bool,
    /// Write only after all operations and the final document are validated.
    #[arg(long, conflicts_with = "diff")]
    pub write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

pub fn refactor_bulk(args: RefactorBulkArgs) -> CommandResult {
    let request_text = read_text_file_with_limit(&args.request, MAX_SOURCE_INPUT_BYTES)?;
    let request: BulkRequest = serde_json::from_str(&request_text)?;
    let (input, dialect, tree, expected) =
        read_input_dialect_tree_and_expected_target(args.file.clone(), args.dialect)?;
    let plan = plan_bulk(&input.text, &tree, dialect, request)?;
    if args.diff {
        print!(
            "{}",
            terminal_safe(unified_diff(&args.file, &input.text, &plan.rewritten))
        );
        return Ok(());
    }
    if args.write && plan.changed {
        write_files_with_rollback_expected([(
            args.file.clone(),
            plan.rewritten.clone(),
            expected,
        )])?;
    }
    // The final tree is the planner's validated tree, not a second parse for
    // presentation. Unedited bytes, including line endings, remain verbatim.
    let report = serde_json::json!({
        "schema_version": 1,
        "version": 1,
        "file": args.file,
        "input_hash": stable_text_hash(&input.text),
        "output_hash": stable_text_hash(&plan.rewritten),
        "operation_count": plan.operation_count,
        "edit_count": plan.edit_count,
        "forms_before": plan.forms_before,
        "forms_after": plan.tree.root_view().children.len(),
        "changed": plan.changed,
        "written": args.write && plan.changed,
        "rewritten": plan.rewritten,
    });
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        OutputFormat::Text => println!("{}", terminal_safe(&plan.rewritten)),
    }
    Ok(())
}
