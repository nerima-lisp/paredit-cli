use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::read_input_dialect_and_tree;

use crate::structural_diff::cli::args::StructuralDiffArgs;
use crate::structural_diff::cli::render::print_structural_diff;
use crate::structural_diff::usecase::{DiffPolicy, diff_documents};

pub fn structural_diff(args: StructuralDiffArgs) -> CommandResult {
    let (old_input, _, old_tree) =
        read_input_dialect_and_tree(Some(args.old.clone()), args.dialect)?;
    let (new_input, _, new_tree) =
        read_input_dialect_and_tree(Some(args.new.clone()), args.dialect)?;

    let mut changes = diff_documents(
        &old_tree.root_view(),
        &old_input.text,
        &new_tree.root_view(),
        &new_input.text,
    );
    // The gate reads the *unfiltered* count. `--max-depth` is a display choice,
    // and a run that hid its only change behind it must not thereby pass a gate
    // that says the documents are structurally identical.
    let policy = DiffPolicy::evaluate(args.fail_on_change, changes.len());
    if let Some(floor) = args.max_depth {
        changes.retain(|change| change.depth <= floor);
    }

    print_structural_diff(
        &args.old.display().to_string(),
        &args.new.display().to_string(),
        &changes,
        &policy,
        args.output,
    )?;

    if !policy.passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect diff policy failed: {} structural change(s) between {} and {}",
            policy.change_count,
            args.old.display(),
            args.new.display(),
        )));
    }

    Ok(())
}
