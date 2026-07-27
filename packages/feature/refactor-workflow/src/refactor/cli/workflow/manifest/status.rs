use super::super::super::super::*;
use super::super::super::args::RefactorStatusArgs;
use super::super::super::manifest::check::build_refactor_check_result;
use super::super::super::manifest::status::refactor_status_decision;
use super::super::super::render::print_refactor_status_result;
use super::super::super::types::status::{RefactorStatusResult, RefactorStatusWriteTarget};

pub(in crate::presentation::cli) fn refactor_status(args: RefactorStatusArgs) -> Result<()> {
    let check = build_refactor_check_result(
        &args.manifest,
        args.root.as_deref(),
        args.expect_manifest_hash.as_deref(),
    )?;
    let decision = refactor_status_decision(&check);
    let write_plan = if check.summary.can_apply {
        check
            .files
            .iter()
            .filter(|file| file.changed)
            .map(|file| RefactorStatusWriteTarget {
                path: file.path.clone(),
                edit_count: file.edit_count,
                input_hash: file.input_hash.clone(),
                output_hash: file.output_hash.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let result = RefactorStatusResult {
        manifest: check.manifest,
        root: check.root,
        manifest_policy_passed: check.manifest_policy_passed,
        manifest_outputs_parse: check.manifest_outputs_parse,
        status: decision.status,
        next_action: decision.next_action,
        blocked_reasons: decision.blocked_reasons,
        write_plan,
        files: check.files,
        summary: check.summary,
    };

    print_refactor_status_result(&result, args.output)
}
