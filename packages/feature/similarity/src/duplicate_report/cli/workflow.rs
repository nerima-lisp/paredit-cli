use super::args::{DuplicateReportArgs, ReplacementPlanArgs};
use super::render::{print_duplicate_report, print_replacement_plan};
use super::workspace::discover_duplicate_report_files;
use crate::duplicate_report::usecase::{
    DuplicateCandidateAccumulator, DuplicateCandidateGroups, build_duplicate_shape_reports,
    collect_replacement_plan_batches,
};
use paredit_core_cli::CliResult;
use paredit_core_cli::shared::{
    analyze_files_raw, note_partial_file_failures, read_input_dialect_and_tree, total_file_failure,
};

pub fn duplicate_report(args: DuplicateReportArgs) -> CliResult<()> {
    ensure_thresholds(args.min_group_size, args.min_node_count)?;
    let grouped = collect_duplicate_candidate_groups(
        &args.files,
        args.dialect,
        args.min_node_count,
        args.min_group_size,
    )?;
    let reports = build_duplicate_shape_reports(grouped, args.min_group_size);

    print_duplicate_report(&reports, args.output)
}

pub fn replacement_plan(args: ReplacementPlanArgs) -> CliResult<()> {
    ensure_thresholds(args.min_group_size, args.min_node_count)?;
    let grouped = collect_duplicate_candidate_groups(
        &args.files,
        args.dialect,
        args.min_node_count,
        args.min_group_size,
    )?;
    let mut batches = collect_replacement_plan_batches(
        grouped,
        args.min_group_size,
        args.replacement,
        args.keep_first,
    );
    batches.sort_by(|left, right| {
        right
            .forms
            .len()
            .cmp(&left.forms.len())
            .then_with(|| left.file.cmp(&right.file))
            .then_with(|| left.shape.cmp(&right.shape))
    });

    print_replacement_plan(&batches, args.output)
}

fn collect_duplicate_candidate_groups(
    roots: &[std::path::PathBuf],
    dialect: Option<paredit_core_cli::args::DialectArg>,
    min_node_count: usize,
    min_group_size: usize,
) -> CliResult<DuplicateCandidateGroups> {
    let files = discover_duplicate_report_files(roots)?;
    // The read and parse of each file are independent, but `add_source`
    // itself is not: it appends to a shared candidate map in call order, and
    // that order becomes the order occurrences are reported within a
    // duplicate group. Only the read+parse step runs in parallel; `finish`'s
    // fold below stays a plain sequential loop over `analysis.succeeded`,
    // which preserves input order regardless of which file's read finished
    // first.
    let analysis = analyze_files_raw(&files, |file| {
        let (_input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), dialect)?;
        CliResult::Ok((tree, file.clone(), dialect))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);

    let mut candidates = DuplicateCandidateAccumulator::new(min_node_count);
    for (tree, file, dialect) in analysis.succeeded {
        candidates.add_source(tree, file, dialect)?;
    }

    Ok(candidates.finish(min_group_size)?)
}

fn ensure_thresholds(min_group_size: usize, min_node_count: usize) -> CliResult<()> {
    if min_group_size < 2 {
        return Err(paredit_core_cli::ArgumentError::FlagCombination {
            message: "--min-group-size must be at least 2".to_owned(),
        }
        .into());
    }
    if min_node_count < 2 {
        return Err(paredit_core_cli::ArgumentError::FlagCombination {
            message: "--min-node-count must be at least 2".to_owned(),
        }
        .into());
    }
    Ok(())
}
