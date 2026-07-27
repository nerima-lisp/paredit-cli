use std::fs;
use std::path::Path as FsPath;

use anyhow::{Context, Result};

use crate::application::usecase::split_file::{SplitFileRequest, plan_split_file};

use super::super::shared::{
    detect_dialect, read_file_or_empty, read_input_and_dialect, write_files_with_rollback,
};
use super::args::SplitFileArgs;
use super::render::split_file::print_split_file_plan;
use super::shared::same_file_path;

pub(in crate::presentation::cli) fn split_file(args: SplitFileArgs) -> Result<()> {
    if same_file_path(&args.from_file, &args.to_file) {
        anyhow::bail!("--from-file and --to-file must refer to different files");
    }

    let (from_input, from_dialect) =
        read_input_and_dialect(Some(args.from_file.clone()), args.dialect)?;
    let (to_input, to_file_existed) = read_file_or_empty(&args.to_file)?;
    let to_parent_existed = args
        .to_file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(FsPath::exists)
        .unwrap_or(true);
    let to_dialect = detect_dialect(&to_input, args.dialect);

    let plan = plan_split_file(SplitFileRequest {
        from_file: args.from_file.clone(),
        to_file: args.to_file.clone(),
        from_input: &from_input.text,
        to_input: &to_input.text,
        from_dialect,
        to_dialect,
        paths: args.paths,
        names: args.names,
        categories: args.categories,
        to_file_existed,
        to_parent_existed,
        write: args.write,
    })?;

    if plan.written {
        if let Some(parent) = args
            .to_file
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        write_files_with_rollback([
            (args.from_file.clone(), plan.from_rewritten.clone()),
            (args.to_file.clone(), plan.to_rewritten.clone()),
        ])?;
    }

    print_split_file_plan(&plan, args.output)
}
