//! The five command entry points.

use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::gate::gate_failure;

use crate::clone_report::domain::{
    CloneClassOptions, CloneSequenceOptions, CloneThresholdOptions, SequenceSource,
    build_clone_class_report, build_clone_external_report, build_clone_genealogy_report,
    build_clone_sequence_report, build_clone_threshold_report,
};
use crate::form_similarity::CloneType;
use crate::similarity_report::domain::SimilarityReportOptions;

use super::args::{
    CloneClassReportArgs, CloneExternalReportArgs, CloneGenealogyReportArgs, CloneMatchArgs,
    CloneSequenceReportArgs, CloneThresholdReportArgs,
};
use super::collect::{collect_candidates, collect_candidates_with_generated, collect_sources};
use super::git::GitCloneHistory;
use super::render::{
    print_clone_classes, print_clone_external, print_clone_genealogy, print_clone_sequences,
    print_clone_threshold,
};

pub fn clone_classes(args: CloneClassReportArgs) -> CommandResult {
    let options = similarity_options(&args.matching)?;
    let corpus = collect_candidates(&args.discovery.roots, &args.discovery, &options)?;
    let class_options = CloneClassOptions {
        overlap_policy: args.matching.overlap_policy,
        min_members: args.min_members.max(2),
        max_classes: args.max_classes,
        clone_type: args.clone_type.map(clone_type_from_number),
        helper_overhead_lines: args.extraction.helper_overhead_lines,
    };
    if args.min_members < 2 {
        return Err(paredit_core_cli::ArgumentError::FlagCombination {
            message: "--min-members must be at least 2".to_owned(),
        }
        .into());
    }
    if args.max_classes == Some(0) {
        return Err(paredit_core_cli::ArgumentError::FlagCombination {
            message: "--max-classes must be at least 1".to_owned(),
        }
        .into());
    }

    let report = build_clone_class_report(
        corpus.candidates,
        corpus.omitted_candidates,
        &options,
        &class_options,
    )?;
    print_clone_classes(
        &report,
        &corpus.summary,
        &corpus.errors,
        args.discovery.output,
    )?;

    if args.fail_on_clones && !report.classes.is_empty() {
        return Err(gate_failure(format!(
            "inspect clone-classes policy failed: {} clone class(es) found",
            report.classes.len()
        )));
    }
    Ok(())
}

pub fn clone_sequences(args: CloneSequenceReportArgs) -> CommandResult {
    let options = CloneSequenceOptions {
        min_run_length: args.min_run_length,
        max_run_length: args.max_run_length,
        min_occurrences: args.min_occurrences,
        min_run_nodes: args.min_run_nodes,
        match_mode: args.match_mode,
        overlap_policy: args.overlap_policy,
        max_groups: args.max_groups,
        include_parent_clones: args.include_parent_clones,
        helper_overhead_lines: args.extraction.helper_overhead_lines,
    };
    options.validate()?;

    let corpus = collect_sources(&args.discovery.roots, &args.discovery)?;
    let sources = corpus
        .sources
        .iter()
        .map(|source| SequenceSource {
            path: source.path.as_path(),
            dialect: source.dialect,
            tree: &source.tree,
            text: source.text.as_str(),
        })
        .collect::<Vec<_>>();
    let report = build_clone_sequence_report(&sources, &options)?;
    print_clone_sequences(
        &report,
        &corpus.summary,
        &corpus.errors,
        args.discovery.output,
    )?;

    if args.fail_on_clones && !report.groups.is_empty() {
        return Err(gate_failure(format!(
            "inspect clone-sequences policy failed: {} duplicated sequence(s) found",
            report.groups.len()
        )));
    }
    Ok(())
}

pub fn clone_external(args: CloneExternalReportArgs) -> CommandResult {
    let options = similarity_options(&args.matching)?;
    let project = collect_candidates(&args.discovery.roots, &args.discovery, &options)?;
    // The reference corpus is read with the project's error policy but the
    // opposite generated-directory default, because a dependency checkout lives
    // exactly where discovery would otherwise refuse to look.
    let reference = collect_candidates_with_generated(
        &args.reference,
        &args.discovery,
        &options,
        !args.reference_skip_generated,
    )?;

    let report = build_clone_external_report(
        project.candidates,
        reference.candidates,
        project.summary.scanned_files,
        reference.summary.scanned_files,
        &options,
    )?;
    print_clone_external(
        &report,
        &project.summary,
        &reference.summary,
        &project.errors,
        args.discovery.output,
    )?;

    if args.fail_on_matches && !report.matches.is_empty() {
        return Err(gate_failure(format!(
            "inspect clone-external policy failed: {} external match(es) found",
            report.matches.len()
        )));
    }
    Ok(())
}

pub fn clone_threshold(args: CloneThresholdReportArgs) -> CliResult<()> {
    if !(0.0..=1.0).contains(&args.floor) {
        return Err(paredit_core_cli::ArgumentError::FlagCombination {
            message: "--floor must be between 0.0 and 1.0".to_owned(),
        }
        .into());
    }
    if args.bucket_width <= 0.0 || args.bucket_width > 1.0 {
        return Err(paredit_core_cli::ArgumentError::FlagCombination {
            message: "--bucket-width must be greater than 0.0 and at most 1.0".to_owned(),
        }
        .into());
    }

    let options = similarity_options(&args.matching)?;
    let corpus = collect_candidates(&args.discovery.roots, &args.discovery, &options)?;
    let calibration = CloneThresholdOptions {
        floor: args.floor,
        bucket_width: args.bucket_width,
        min_sample: args.min_sample,
        min_gap_buckets: args.min_gap_buckets,
        default_threshold: args.matching.threshold,
    };

    let report = build_clone_threshold_report(corpus.candidates, &options, &calibration)?;
    print_clone_threshold(
        &report,
        &corpus.summary,
        &corpus.errors,
        args.discovery.output,
    )
}

pub fn clone_genealogy(args: CloneGenealogyReportArgs) -> CommandResult {
    if args.min_members < 2 {
        return Err(paredit_core_cli::ArgumentError::FlagCombination {
            message: "--min-members must be at least 2".to_owned(),
        }
        .into());
    }
    if args.max_classes == Some(0) {
        return Err(paredit_core_cli::ArgumentError::FlagCombination {
            message: "--max-classes must be at least 1".to_owned(),
        }
        .into());
    }

    let options = similarity_options(&args.matching)?;
    let corpus = collect_candidates(&args.discovery.roots, &args.discovery, &options)?;
    let class_options = CloneClassOptions {
        overlap_policy: args.matching.overlap_policy,
        min_members: args.min_members,
        max_classes: args.max_classes,
        clone_type: None,
        helper_overhead_lines: args.extraction.helper_overhead_lines,
    };
    let classes = build_clone_class_report(
        corpus.candidates,
        corpus.omitted_candidates,
        &options,
        &class_options,
    )?;

    let history = GitCloneHistory::default();
    let report = build_clone_genealogy_report(&classes, &history);
    print_clone_genealogy(
        &report,
        &corpus.summary,
        &corpus.errors,
        args.discovery.output,
    )?;

    if args.fail_on_undated && report.undated_members > 0 {
        return Err(gate_failure(format!(
            "inspect clone-genealogy policy failed: {} clone member(s) could not be dated",
            report.undated_members
        )));
    }
    Ok(())
}

fn similarity_options(args: &CloneMatchArgs) -> CliResult<SimilarityReportOptions> {
    Ok(SimilarityReportOptions::new(
        args.threshold,
        args.min_node_count,
        args.min_line_span,
        args.comparison_scope,
        args.form_scope,
        // Always `all` at the pair level. Nesting is suppressed once, on the
        // classes, where there are hundreds of things to compare rather than
        // tens of thousands — the pair-level policy is quadratic in the matched
        // pair count and is the dominant cost on repetitive input.
        crate::similarity_report::domain::SimilarityOverlapPolicy::All,
        args.max_candidates,
        args.max_comparisons,
        args.max_results,
    )?)
}

const fn clone_type_from_number(number: u8) -> CloneType {
    match number {
        1 => CloneType::Type1,
        2 => CloneType::Type2,
        _ => CloneType::Type3,
    }
}
