use anyhow::Result;
use serde_json::json;

use crate::application::usecase::similarity_report::{
    SimilarityFormReport, SimilarityProcessingStage, SimilarityReportPlan,
};
use crate::domain::dialect::Dialect;

use super::super::OutputFormat;
use super::args::SimilarityReportArgs;

pub(super) fn print_similarity_report(
    plan: &SimilarityReportPlan,
    args: &SimilarityReportArgs,
) -> Result<()> {
    match args.output {
        OutputFormat::Text => print_text(plan, args),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json_report(plan, args))?
        ),
    }
    Ok(())
}

fn print_text(plan: &SimilarityReportPlan, args: &SimilarityReportArgs) {
    let report = plan.report();
    let inventory = plan.inventory();
    let errors = plan.errors();
    let summary = &report.summary;
    println!("schema_version\t1");
    println!("threshold\t{:.6}", args.threshold);
    println!("min_node_count\t{}", args.min_node_count);
    println!("min_line_span\t{}", args.min_line_span);
    println!("comparison_scope\t{}", args.comparison_scope.label());
    println!("form_scope\t{}", args.form_scope.label());
    println!("overlap_policy\t{}", args.overlap_policy.label());
    println!("max_comparisons\t{}", optional_usize(args.max_comparisons));
    println!("max_candidates\t{}", optional_usize(args.max_candidates));
    println!("max_results\t{}", optional_usize(args.max_results));
    println!("error_policy\t{}", args.error_policy.label());
    println!("scanned_files\t{}", inventory.files.len());
    println!(
        "processed_files\t{}",
        inventory.files.len().saturating_sub(errors.len())
    );
    println!("skipped_error_files\t{}", errors.len());
    println!("skipped_unknown\t{}", inventory.skipped_unknown_count);
    println!("skipped_hidden\t{}", inventory.skipped_hidden_count);
    println!("skipped_generated\t{}", inventory.skipped_generated_count);
    println!("skipped_symlink\t{}", inventory.skipped_symlink_count);
    println!("skipped_excluded\t{}", inventory.skipped_excluded_count);
    println!("possible_pairs\t{}", summary.possible_pairs());
    println!(
        "candidate_limit_reached\t{}",
        summary.candidate_limit_reached()
    );
    println!("omitted_candidates\t{}", summary.omitted_candidates());
    println!("evaluated_pairs\t{}", summary.evaluated_pairs());
    println!("pruned_by_size\t{}", summary.pruned_by_size());
    println!(
        "resource_skipped_pairs\t{}",
        summary.resource_skipped_pairs()
    );
    println!(
        "comparison_limit_reached\t{}",
        summary.comparison_limit_reached()
    );
    println!("unprocessed_pairs\t{}", summary.unprocessed_pairs());
    println!("matched_pairs\t{}", summary.matched_pairs());
    println!("suppressed_pairs\t{}", summary.suppressed_pairs());
    println!("reported_pairs\t{}", summary.reported_pairs());
    println!("pair_count\t{}", summary.reported_pairs());
    println!("truncated\t{}", summary.truncated());

    for error in errors {
        println!(
            "error\t{}\t{}\t{}",
            safe_text!(error.path.display()),
            safe_text!(cli_stage_label(error.stage)),
            safe_text!(error.message)
        );
    }

    for pair in &report.pairs {
        println!(
            "pair\tsimilarity={:.6}\tscore={:.6}",
            pair.similarity().as_f64(),
            pair.score().as_f64()
        );
        print_text_form("left", pair.left());
        print_text_form("right", pair.right());
    }
}

fn json_report(plan: &SimilarityReportPlan, args: &SimilarityReportArgs) -> serde_json::Value {
    let report = plan.report();
    let inventory = plan.inventory();
    let errors = plan.errors();
    json!({
        "schema_version": 1,
        "pair_count": report.summary.reported_pairs(),
        "options": {
            "roots": args.roots.iter().map(|root| root.display().to_string()).collect::<Vec<_>>(),
            "dialect": args.dialect.map(|dialect| Dialect::from(dialect).label()),
            "threshold": args.threshold,
            "min_node_count": args.min_node_count,
            "min_line_span": args.min_line_span,
            "comparison_scope": args.comparison_scope.label(),
            "form_scope": args.form_scope.label(),
            "overlap_policy": args.overlap_policy.label(),
            "max_comparisons": args.max_comparisons,
            "max_candidates": args.max_candidates,
            "max_results": args.max_results,
            "error_policy": args.error_policy.label(),
            "include_unknown": args.include_unknown,
            "include_hidden": args.include_hidden,
            "include_generated": args.include_generated,
            "max_depth": args.max_depth,
            "exclude": args.exclude.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
            "fail_on_duplicates": args.fail_on_duplicates,
        },
        "summary": {
            "scanned_files": inventory.files.len(),
            "processed_files": inventory.files.len().saturating_sub(errors.len()),
            "skipped_error_files": errors.len(),
            "skipped_unknown": inventory.skipped_unknown_count,
            "skipped_hidden": inventory.skipped_hidden_count,
            "skipped_generated": inventory.skipped_generated_count,
            "skipped_symlink": inventory.skipped_symlink_count,
            "skipped_excluded": inventory.skipped_excluded_count,
            "possible_pairs": report.summary.possible_pairs(),
            "candidate_limit_reached": report.summary.candidate_limit_reached(),
            "omitted_candidates": report.summary.omitted_candidates(),
            "evaluated_pairs": report.summary.evaluated_pairs(),
            "pruned_by_size": report.summary.pruned_by_size(),
            "resource_skipped_pairs": report.summary.resource_skipped_pairs(),
            "comparison_limit_reached": report.summary.comparison_limit_reached(),
            "unprocessed_pairs": report.summary.unprocessed_pairs(),
            "matched_pairs": report.summary.matched_pairs(),
            "suppressed_pairs": report.summary.suppressed_pairs(),
            "reported_pairs": report.summary.reported_pairs(),
            "truncated": report.summary.truncated(),
        },
        "errors": errors.iter().map(|error| json!({
            "path": error.path.display().to_string(),
            "stage": cli_stage_label(error.stage),
            "message": error.message,
        })).collect::<Vec<_>>(),
        "pairs": report.pairs.iter().map(|pair| json!({
            "similarity": pair.similarity().as_f64(),
            "score": pair.score().as_f64(),
            "left": form_json(pair.left()),
            "right": form_json(pair.right()),
        })).collect::<Vec<_>>(),
    })
}

fn optional_usize(value: Option<usize>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}

const fn cli_stage_label(stage: SimilarityProcessingStage) -> &'static str {
    match stage {
        SimilarityProcessingStage::Decode => "read",
        _ => stage.label(),
    }
}

fn print_text_form(side: &str, form: &SimilarityFormReport) {
    println!(
        "\t{side}\t{}\t{}\t{}\t{}..{}\tnodes={}\thead={}",
        safe_text!(form.path().display()),
        form.dialect().label(),
        safe_text!(form.form_path()),
        form.span().start().get(),
        form.span().end().get(),
        form.node_count(),
        safe_text!(form.head().map_or("", |head| head.as_str()))
    );
}

fn form_json(form: &SimilarityFormReport) -> serde_json::Value {
    json!({
        "path": form.path().display().to_string(),
        "dialect": form.dialect().label(),
        "form_path": form.form_path().to_string(),
        "span": { "start": form.span().start().get(), "end": form.span().end().get() },
        "node_count": form.node_count(),
        "head": form.head().map(|head| head.as_str()),
        "text": form.text().as_ref(),
    })
}
