use super::super::super::super::*;
use super::super::super::types::preview::RefactorPreview;
use crate::presentation::cli::shared::write_files_with_rollback;

pub(in crate::presentation::cli::refactor::workflow) fn write_refactor_preview(
    preview: &mut RefactorPreview,
) -> Result<()> {
    let write_plan = preview.write_plan();

    if !write_plan.write_requested() {
        return Ok(());
    }

    if let Some(refusal) = write_plan.refusal() {
        match refusal {
            RefactorWriteRefusal::UnparsableOutputs { count } => anyhow::bail!(
                "refactor write refused because {count} rewritten output file(s) failed to parse"
            ),
        }
    }

    let mut written_files = Vec::with_capacity(write_plan.writable_indexes().len());
    for index in write_plan.writable_indexes().iter().copied() {
        let file = &preview.files[index];
        written_files.push((file.path.clone(), file.rewritten.clone()));
    }
    write_files_with_rollback(written_files)?;

    for index in write_plan.writable_indexes().iter().copied() {
        if let Some(file) = preview.files.get_mut(index) {
            file.written = true;
        }
    }
    preview
        .summary
        .set_written_file_count(preview.files.iter().filter(|file| file.written).count())
        .map_err(anyhow::Error::msg)?;

    Ok(())
}
