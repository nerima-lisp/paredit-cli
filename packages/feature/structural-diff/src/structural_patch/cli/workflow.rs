use paredit_core_cli::CommandResult;

use paredit_core_cli::color::{Painter, colorize_diff};
use paredit_core_cli::shared::{
    apply_byte_span_edits, read_input_dialect_and_tree, unified_diff, write_file_with_rollback,
};
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::structural_diff::usecase::diff_documents;
use crate::structural_patch::cli::args::StructuralPatchArgs;
use crate::structural_patch::cli::render::{PatchSubjects, print_patch_plan};
use crate::structural_patch::usecase::{Outcome, plan_patch};

pub fn structural_patch(args: StructuralPatchArgs) -> CommandResult {
    let (from_input, _, from_tree) =
        read_input_dialect_and_tree(Some(args.from.clone()), args.dialect)?;
    let (to_input, _, to_tree) = read_input_dialect_and_tree(Some(args.to.clone()), args.dialect)?;
    let (target_input, _, target_tree) =
        read_input_dialect_and_tree(Some(args.apply_to.clone()), args.dialect)?;

    let changes = diff_documents(
        &from_tree.root_view(),
        &from_input.text,
        &to_tree.root_view(),
        &to_input.text,
    );
    let plan = plan_patch(&changes, &target_tree.root_view(), args.all);
    let patched = apply_byte_span_edits(&target_input.text, plan.edits.clone())?;

    // A structural patch composes replacements taken from another file, so
    // nothing guarantees the result is still balanced — a change whose "after"
    // side ends mid-form would produce a document this tool could not read
    // back. Refusing here is the difference between a failed patch and a
    // corrupted file.
    if let Err(error) = SyntaxTree::parse(&patched) {
        return Err(crate::error::PatchWouldNotReparse {
            path: args.apply_to.display().to_string(),
            reason: error.to_string(),
        }
        .into());
    }

    if args.diff {
        print!(
            "{}",
            colorize_diff(
                Painter::stdout(),
                &unified_diff(&args.apply_to, &target_input.text, &patched)
            )
        );
        return gate(&plan, &args);
    }

    let written = args.write && patched != target_input.text;
    if written {
        write_file_with_rollback(args.apply_to.clone(), patched.clone())?;
    }

    print_patch_plan(
        &plan,
        &PatchSubjects {
            from: args.from.display().to_string(),
            to: args.to.display().to_string(),
            apply_to: args.apply_to.display().to_string(),
            written,
        },
        args.output,
    )?;

    gate(&plan, &args)
}

/// `--fail-on-unapplied`: every change had to land, or the port is partial.
///
/// A partial port is the failure worth gating on. It is the one that looks like
/// success — the file was written, some sites are fixed — while leaving the
/// rest of the change behind for nobody to notice.
fn gate(
    plan: &crate::structural_patch::usecase::PatchPlan,
    args: &StructuralPatchArgs,
) -> CommandResult {
    if !args.fail_on_unapplied {
        return Ok(());
    }
    let unapplied = plan.resolutions.len() - plan.count_with(Outcome::Applied);
    if unapplied == 0 {
        return Ok(());
    }
    Err(paredit_core_cli::gate::gate_failure(format!(
        "refactor patch policy failed: {unapplied} of {} change(s) did not carry over to {}",
        plan.resolutions.len(),
        args.apply_to.display(),
    )))
}
