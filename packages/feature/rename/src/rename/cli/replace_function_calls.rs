use crate::error::RenamePlanError;
use paredit_core_cli::CommandResult;

use super::args::ReplaceFunctionCallsArgs;
use super::render::replace_call::print_replace_function_calls_report;
use super::shared::evaluate_call_site_policy;
use super::types::{PendingReplaceFunctionCallsFile, ReplaceFunctionCallsFileReport};
use crate::rename::usecase::{self as rename_usecase, ReplaceFunctionCallsScope};
use paredit_core_cli::shared::{read_input_and_dialect, write_files_with_rollback};

pub fn replace_function_calls(args: ReplaceFunctionCallsArgs) -> CommandResult {
    if args.all_calls != args.call_paths.is_empty() {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
            "replace-function-calls requires either --all-calls or repeated --call-path",
        )
        .into());
    }

    let mut pending = Vec::with_capacity(args.files.len());
    for file in &args.files {
        let (input, dialect) = read_input_and_dialect(Some(file.clone()), args.dialect)?;
        let scope = if args.all_calls {
            ReplaceFunctionCallsScope::AllCalls
        } else {
            ReplaceFunctionCallsScope::ExplicitPaths(args.call_paths.clone())
        };
        let plan = rename_usecase::plan_replace_function_calls(
            rename_usecase::ReplaceFunctionCallsRequest {
                input: &input.text,
                dialect,
                from: args.from.clone(),
                to: args.to.clone(),
                scope,
            },
        )
        .map_err(|source| RenamePlanError {
            operation: "replace-function-calls".to_owned(),
            path: file.display().to_string(),
            source,
        })?;
        pending.push(PendingReplaceFunctionCallsFile {
            path: file.clone(),
            dialect: plan.dialect,
            calls: plan.calls,
            rewritten: plan.rewritten,
            changed: plan.changed,
        });
    }

    let selected_call_count = pending.iter().map(|file| file.calls.len()).sum::<usize>();
    let policy = evaluate_call_site_policy(
        selected_call_count,
        args.fail_on_no_change,
        args.require_calls,
    );
    if !policy.passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "replace-function-calls policy failed: {}",
            policy.violations.join("; ")
        )));
    }

    let written_files = pending
        .iter()
        .filter(|file| args.write && file.changed)
        .map(|file| (file.path.clone(), file.rewritten.clone()))
        .collect::<Vec<_>>();
    if !written_files.is_empty() {
        write_files_with_rollback(written_files)?;
    }

    let mut reports = Vec::with_capacity(pending.len());
    for file in pending {
        let written = args.write && file.changed;
        reports.push(ReplaceFunctionCallsFileReport {
            path: file.path,
            dialect: file.dialect,
            calls: file.calls,
            changed: file.changed,
            written,
            rewritten: file.rewritten,
        });
    }

    Ok(print_replace_function_calls_report(
        &reports,
        &args,
        &policy,
        args.output,
    )?)
}
