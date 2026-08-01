//! `schema check`.
//!
//! Both files this command reads — the schema and the instance — are read
//! with the same bounded, regular-file-only reader every source file in this
//! tool goes through, and both are parsed with Common Lisp's reader
//! unconditionally rather than by the extension on either path: a
//! `defschema` file is this tool's own data format, not source code in one
//! of the ten dialects it edits, and an instance is arbitrary S-expression
//! data with the same property. Neither file is ever evaluated.

use paredit_core_cli::CommandResult;
use paredit_core_cli::shared::{MAX_SOURCE_INPUT_BYTES, read_text_file_with_limit};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::cli::args::{SchemaCheckArgs, SchemaCommand};
use crate::cli::render::print_schema_check_report;
use crate::dsl::parse_schemas;
use crate::error::SchemaCheckError;
use crate::usecase::{build_schema_check_report, evaluate_fail_on_violation_policy, select_schema};

/// Dispatches the namespace's one leaf.
pub fn schema(command: SchemaCommand) -> CommandResult {
    match command {
        SchemaCommand::Check(args) => check(args),
    }
}

fn check(args: SchemaCheckArgs) -> CommandResult {
    let schema_path = args.schema.display().to_string();
    let schema_text =
        read_text_file_with_limit(&args.schema, MAX_SOURCE_INPUT_BYTES).map_err(|source| {
            SchemaCheckError::SchemaFileUnreadable {
                path: schema_path.clone(),
                source,
            }
        })?;
    let schemas = parse_schemas(&schema_text, &schema_path).map_err(|source| {
        SchemaCheckError::SchemaFileMalformed {
            path: schema_path.clone(),
            source,
        }
    })?;
    let schema = select_schema(&schemas, args.schema_name.as_deref(), &schema_path)?;

    let instance_path = args.instance.display().to_string();
    let instance_text =
        read_text_file_with_limit(&args.instance, MAX_SOURCE_INPUT_BYTES).map_err(|source| {
            SchemaCheckError::InstanceFileUnreadable {
                path: instance_path.clone(),
                source,
            }
        })?;
    let tree =
        SyntaxTree::parse_with_dialect(&instance_text, Dialect::CommonLisp).map_err(|error| {
            SchemaCheckError::InstanceFileUnparsable {
                path: instance_path.clone(),
                reason: error.to_string(),
            }
        })?;

    let report = build_schema_check_report(&args.instance, &tree, schema).ok_or_else(|| {
        SchemaCheckError::InstanceShapeUnrecognized {
            path: instance_path.clone(),
        }
    })?;
    let reports = [report];

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let message = policy.violations.join("; ");
    print_schema_check_report(&reports, &policy, args.output, args.verbosity)?;

    if !policy.passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "schema check policy failed: {message}"
        )));
    }
    Ok(())
}
