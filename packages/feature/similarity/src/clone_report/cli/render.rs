//! Text and JSON rendering for the five clone reports.
//!
//! Both formats carry the same facts. The text form is tab-separated with a
//! leading key on every line so `grep`, `cut` and a human all work; the JSON
//! form is what an agent should read, which is why it is the default.

use anyhow::Result;
use serde_json::{Value, json};

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::clone_report::domain::{
    CloneClassReport, CloneExternalReport, CloneGenealogyReport, CloneSequenceReport,
    CloneThresholdReport,
};
use crate::similarity_report::domain::SimilarityFormReport;

use super::collect::{CorpusFileError, DiscoverySummary};

pub fn print_clone_classes(
    report: &CloneClassReport,
    summary: &DiscoverySummary,
    errors: &[CorpusFileError],
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "class_count": report.classes.len(),
                "summary": {
                    "discovery": discovery_json(summary, errors),
                    "total_classes": report.total_classes,
                    "filtered_classes": report.filtered_classes,
                    "nested_classes": report.nested_classes,
                    "truncated_classes": report.truncated_classes,
                    "unclassified_edges": report.unclassified_edges,
                    "estimated_saved_lines": report.saved_lines(),
                    "possible_pairs": report.summary.possible_pairs(),
                    "evaluated_pairs": report.summary.evaluated_pairs(),
                    "matched_pairs": report.summary.matched_pairs(),
                    "reported_pairs": report.summary.reported_pairs(),
                    "candidate_limit_reached": report.summary.candidate_limit_reached(),
                    "omitted_candidates": report.summary.omitted_candidates(),
                    "comparison_limit_reached": report.summary.comparison_limit_reached(),
                    "truncated": report.summary.truncated(),
                },
                "errors": errors_json(errors),
                "classes": report.classes.iter().map(|class| json!({
                    "rank": class.rank,
                    "clone_type": class.clone_type.label(),
                    "clone_type_number": class.clone_type.number(),
                    "consistent_renaming": class.consistent_renaming,
                    "renamed_atoms": class.renamed_atoms,
                    "member_count": class.members.len(),
                    "edge_count": class.edge_count,
                    "unclassified_edges": class.unclassified_edges,
                    "min_similarity": class.min_similarity,
                    "max_similarity": class.max_similarity,
                    "mean_similarity": class.mean_similarity,
                    "extraction": extraction_json(&class.extraction),
                    "members": class.members.iter().map(|form| form_json(form)).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            }))?
        ),
        OutputFormat::Text => {
            println!("schema_version\t1");
            print_discovery_text(summary, errors);
            println!("class_count\t{}", report.classes.len());
            println!("total_classes\t{}", report.total_classes);
            println!("filtered_classes\t{}", report.filtered_classes);
            println!("nested_classes\t{}", report.nested_classes);
            println!("truncated_classes\t{}", report.truncated_classes);
            println!("unclassified_edges\t{}", report.unclassified_edges);
            println!("estimated_saved_lines\t{}", report.saved_lines());
            println!("possible_pairs\t{}", report.summary.possible_pairs());
            println!("evaluated_pairs\t{}", report.summary.evaluated_pairs());
            println!("matched_pairs\t{}", report.summary.matched_pairs());
            print_errors_text(errors);
            for class in &report.classes {
                println!(
                    "class\t{}\t{}\tmembers={}\tedges={}\tsaved_lines={}\tconsistent_renaming={}\tsimilarity={:.6}..{:.6}",
                    class.rank,
                    class.clone_type.label(),
                    class.members.len(),
                    class.edge_count,
                    class.extraction.saved_lines,
                    class.consistent_renaming,
                    class.min_similarity,
                    class.max_similarity,
                );
                for form in &class.members {
                    print_form_text("member", form);
                }
            }
        }
    }
    Ok(())
}

pub fn print_clone_sequences(
    report: &CloneSequenceReport,
    summary: &DiscoverySummary,
    errors: &[CorpusFileError],
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "group_count": report.groups.len(),
                "summary": {
                    "discovery": discovery_json(summary, errors),
                    "scanned_files": report.scanned_files,
                    "scanned_parents": report.scanned_parents,
                    "candidate_runs": report.candidate_runs,
                    "total_groups": report.total_groups,
                    "suppressed_parent_clone_groups": report.suppressed_parent_clone_groups,
                    "suppressed_overlapping_groups": report.suppressed_overlapping_groups,
                    "truncated_groups": report.truncated_groups,
                    "occurrence_count": report.occurrence_count(),
                    "estimated_saved_lines": report.saved_lines(),
                },
                "errors": errors_json(errors),
                "groups": report.groups.iter().map(|group| json!({
                    "rank": group.rank,
                    "run_length": group.run_length,
                    "clone_type": group.clone_type.label(),
                    "clone_type_number": group.clone_type.number(),
                    "consistent_renaming": group.consistent_renaming,
                    "occurrence_count": group.occurrences.len(),
                    "extraction": extraction_json(&group.extraction),
                    "occurrences": group.occurrences.iter().map(|occurrence| json!({
                        "path": occurrence.path.display().to_string(),
                        "dialect": occurrence.dialect.label(),
                        "parent_path": occurrence.parent_path.to_string(),
                        "parent_head": occurrence.parent_head,
                        "first_child_index": occurrence.first_child_index,
                        "run_length": occurrence.run_length,
                        "span": {
                            "start": occurrence.span.start().get(),
                            "end": occurrence.span.end().get(),
                        },
                        "node_count": occurrence.node_count,
                        "line_span": occurrence.line_span,
                        "text": occurrence.text,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            }))?
        ),
        OutputFormat::Text => {
            println!("schema_version\t1");
            print_discovery_text(summary, errors);
            println!("group_count\t{}", report.groups.len());
            println!("scanned_parents\t{}", report.scanned_parents);
            println!("candidate_runs\t{}", report.candidate_runs);
            println!("total_groups\t{}", report.total_groups);
            println!(
                "suppressed_parent_clone_groups\t{}",
                report.suppressed_parent_clone_groups
            );
            println!(
                "suppressed_overlapping_groups\t{}",
                report.suppressed_overlapping_groups
            );
            println!("truncated_groups\t{}", report.truncated_groups);
            println!("occurrence_count\t{}", report.occurrence_count());
            println!("estimated_saved_lines\t{}", report.saved_lines());
            print_errors_text(errors);
            for group in &report.groups {
                println!(
                    "group\t{}\trun_length={}\t{}\toccurrences={}\tsaved_lines={}",
                    group.rank,
                    group.run_length,
                    group.clone_type.label(),
                    group.occurrences.len(),
                    group.extraction.saved_lines,
                );
                for occurrence in &group.occurrences {
                    println!(
                        "\trun\t{}\t{}\t{}\tchild={}\t{}..{}\tnodes={}\tlines={}",
                        safe_text!(occurrence.path.display()),
                        occurrence.dialect.label(),
                        safe_text!(occurrence.parent_path),
                        occurrence.first_child_index,
                        occurrence.span.start().get(),
                        occurrence.span.end().get(),
                        occurrence.node_count,
                        occurrence.line_span,
                    );
                }
            }
        }
    }
    Ok(())
}

pub fn print_clone_external(
    report: &CloneExternalReport,
    project: &DiscoverySummary,
    reference: &DiscoverySummary,
    errors: &[CorpusFileError],
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "match_count": report.matches.len(),
                "summary": {
                    "project": {
                        "discovery": discovery_json(project, errors),
                        "candidates": report.project.candidates,
                    },
                    "reference": {
                        "discovery": discovery_json(reference, &[]),
                        "candidates": report.reference.candidates,
                    },
                    "possible_pairs": report.possible_pairs,
                    "evaluated_pairs": report.evaluated_pairs,
                    "pruned_by_size": report.pruned_by_size,
                    "pruned_by_bound": report.pruned_by_bound,
                    "matched_pairs": report.matched_pairs,
                    "truncated": report.truncated,
                    "comparison_limit_reached": report.comparison_limit_reached,
                },
                "errors": errors_json(errors),
                "matches": report.matches.iter().map(|external| json!({
                    "similarity": external.similarity,
                    "clone_type": external.clone_type.label(),
                    "clone_type_number": external.clone_type.number(),
                    "consistent_renaming": external.consistent_renaming,
                    "project": form_json(&external.project),
                    "reference": form_json(&external.reference),
                })).collect::<Vec<_>>(),
            }))?
        ),
        OutputFormat::Text => {
            println!("schema_version\t1");
            print_discovery_text(project, errors);
            println!("reference_files\t{}", reference.scanned_files);
            println!("project_candidates\t{}", report.project.candidates);
            println!("reference_candidates\t{}", report.reference.candidates);
            println!("possible_pairs\t{}", report.possible_pairs);
            println!("evaluated_pairs\t{}", report.evaluated_pairs);
            println!("pruned_by_size\t{}", report.pruned_by_size);
            println!("pruned_by_bound\t{}", report.pruned_by_bound);
            println!("matched_pairs\t{}", report.matched_pairs);
            println!("match_count\t{}", report.matches.len());
            println!("truncated\t{}", report.truncated);
            println!(
                "comparison_limit_reached\t{}",
                report.comparison_limit_reached
            );
            print_errors_text(errors);
            for external in &report.matches {
                println!(
                    "match\tsimilarity={:.6}\t{}\tconsistent_renaming={}",
                    external.similarity,
                    external.clone_type.label(),
                    external.consistent_renaming,
                );
                print_form_text("project", &external.project);
                print_form_text("reference", &external.reference);
            }
        }
    }
    Ok(())
}

pub fn print_clone_threshold(
    report: &CloneThresholdReport,
    summary: &DiscoverySummary,
    errors: &[CorpusFileError],
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "recommended_threshold": report.recommended.threshold,
                "recommendation": {
                    "method": report.recommended.method.label(),
                    "threshold": report.recommended.threshold,
                    "pairs_at_or_above": report.recommended.pairs_at_or_above,
                    "well_supported": report.well_supported,
                    "widest_gap_buckets": report.widest_gap_buckets,
                },
                "summary": {
                    "discovery": discovery_json(summary, errors),
                    "candidate_forms": report.candidate_forms,
                    "possible_pairs": report.possible_pairs,
                    "evaluated_pairs": report.evaluated_pairs,
                    "scored_pairs": report.scored_pairs,
                    "default_threshold": report.default_threshold,
                    "pairs_at_default": report.pairs_at_default,
                },
                "errors": errors_json(errors),
                "candidates": report.candidates.iter().map(|candidate| json!({
                    "method": candidate.method.label(),
                    "threshold": candidate.threshold,
                    "pairs_at_or_above": candidate.pairs_at_or_above,
                })).collect::<Vec<_>>(),
                "histogram": {
                    "floor": report.histogram.floor,
                    "bucket_width": report.histogram.bucket_width,
                    "sampled_pairs": report.histogram.sampled_pairs,
                    "buckets": report.histogram.buckets.iter().map(|bucket| json!({
                        "lower": bucket.lower,
                        "upper": bucket.upper,
                        "count": bucket.count,
                    })).collect::<Vec<_>>(),
                },
            }))?
        ),
        OutputFormat::Text => {
            println!("schema_version\t1");
            print_discovery_text(summary, errors);
            println!("candidate_forms\t{}", report.candidate_forms);
            println!("possible_pairs\t{}", report.possible_pairs);
            println!("evaluated_pairs\t{}", report.evaluated_pairs);
            println!("scored_pairs\t{}", report.scored_pairs);
            println!(
                "recommended_threshold\t{:.6}\t{}\twell_supported={}",
                report.recommended.threshold,
                report.recommended.method.label(),
                report.well_supported,
            );
            println!("widest_gap_buckets\t{}", report.widest_gap_buckets);
            println!("default_threshold\t{:.6}", report.default_threshold);
            println!("pairs_at_default\t{}", report.pairs_at_default);
            print_errors_text(errors);
            for candidate in &report.candidates {
                println!(
                    "candidate\t{}\t{:.6}\tpairs={}",
                    candidate.method.label(),
                    candidate.threshold,
                    candidate.pairs_at_or_above,
                );
            }
            for bucket in &report.histogram.buckets {
                if bucket.count > 0 {
                    println!(
                        "bucket\t{:.6}\t{:.6}\t{}",
                        bucket.lower, bucket.upper, bucket.count
                    );
                }
            }
        }
    }
    Ok(())
}

pub fn print_clone_genealogy(
    report: &CloneGenealogyReport,
    summary: &DiscoverySummary,
    errors: &[CorpusFileError],
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "genealogy_count": report.genealogies.len(),
                "summary": {
                    "discovery": discovery_json(summary, errors),
                    "total_classes": report.total_classes,
                    "dated_members": report.dated_members,
                    "undated_members": report.undated_members,
                    "unavailable_reasons": report.unavailable_reasons,
                },
                "errors": errors_json(errors),
                "genealogies": report.genealogies.iter().map(|genealogy| json!({
                    "rank": genealogy.rank,
                    "clone_type": genealogy.clone_type.label(),
                    "clone_type_number": genealogy.clone_type.number(),
                    "origin_commit": genealogy.origin_commit,
                    "origin_author": genealogy.origin_author,
                    "origin_date": genealogy.origin_date,
                    "span_days": genealogy.span_days,
                    "dated_members": genealogy.dated_members,
                    "undated_members": genealogy.undated_members,
                    "file_count": genealogy.file_count,
                    "extraction": extraction_json(&genealogy.extraction),
                    "members": genealogy.members.iter().map(|member| json!({
                        "role": member.role.label(),
                        "lag_days": member.lag_days,
                        "start_line": member.start_line,
                        "end_line": member.end_line,
                        "commit": member.history.commit,
                        "author": member.history.author,
                        "date": member.history.date,
                        "unavailable": member.history.unavailable,
                        "form": form_json(&member.form),
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            }))?
        ),
        OutputFormat::Text => {
            println!("schema_version\t1");
            print_discovery_text(summary, errors);
            println!("genealogy_count\t{}", report.genealogies.len());
            println!("total_classes\t{}", report.total_classes);
            println!("dated_members\t{}", report.dated_members);
            println!("undated_members\t{}", report.undated_members);
            for reason in &report.unavailable_reasons {
                println!("unavailable\t{}", safe_text!(reason));
            }
            print_errors_text(errors);
            for genealogy in &report.genealogies {
                println!(
                    "genealogy\t{}\t{}\torigin={}\tspan_days={}\tfiles={}",
                    genealogy.rank,
                    genealogy.clone_type.label(),
                    safe_text!(genealogy.origin_commit.as_deref().unwrap_or("-")),
                    genealogy
                        .span_days
                        .map_or_else(|| "-".to_owned(), |days| days.to_string()),
                    genealogy.file_count,
                );
                for member in &genealogy.members {
                    println!(
                        "\t{}\t{}\t{}..{}\t{}\t{}\tlag_days={}",
                        member.role.label(),
                        safe_text!(member.form.path().display()),
                        member.start_line,
                        member.end_line,
                        safe_text!(member.history.commit.as_deref().unwrap_or("-")),
                        safe_text!(member.history.author.as_deref().unwrap_or("-")),
                        member
                            .lag_days
                            .map_or_else(|| "-".to_owned(), |days| days.to_string()),
                    );
                }
            }
        }
    }
    Ok(())
}

fn extraction_json(extraction: &crate::clone_report::domain::ExtractionEstimate) -> Value {
    json!({
        "member_count": extraction.member_count,
        "file_count": extraction.file_count,
        "total_lines": extraction.total_lines,
        "total_nodes": extraction.total_nodes,
        "representative_lines": extraction.representative_lines,
        "representative_nodes": extraction.representative_nodes,
        "helper_overhead_lines": extraction.helper_overhead_lines,
        "call_site_lines": extraction.call_site_lines,
        "retained_lines": extraction.retained_lines,
        "saved_lines": extraction.saved_lines,
        "saved_nodes": extraction.saved_nodes,
    })
}

fn form_json(form: &SimilarityFormReport) -> Value {
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

fn print_form_text(role: &str, form: &SimilarityFormReport) {
    println!(
        "\t{role}\t{}\t{}\t{}\t{}..{}\tnodes={}\thead={}",
        safe_text!(form.path().display()),
        form.dialect().label(),
        safe_text!(form.form_path()),
        form.span().start().get(),
        form.span().end().get(),
        form.node_count(),
        safe_text!(form.head().map_or("", |head| head.as_str())),
    );
}

fn discovery_json(summary: &DiscoverySummary, errors: &[CorpusFileError]) -> Value {
    json!({
        "scanned_files": summary.scanned_files,
        "processed_files": summary.scanned_files.saturating_sub(errors.len()),
        "skipped_error_files": errors.len(),
        "skipped_unknown": summary.skipped_unknown,
        "skipped_hidden": summary.skipped_hidden,
        "skipped_generated": summary.skipped_generated,
        "skipped_symlink": summary.skipped_symlink,
        "skipped_excluded": summary.skipped_excluded,
    })
}

fn errors_json(errors: &[CorpusFileError]) -> Value {
    json!(
        errors
            .iter()
            .map(|error| json!({
                "path": error.path.display().to_string(),
                "stage": error.stage,
                "message": error.message,
            }))
            .collect::<Vec<_>>()
    )
}

fn print_discovery_text(summary: &DiscoverySummary, errors: &[CorpusFileError]) {
    println!("scanned_files\t{}", summary.scanned_files);
    println!(
        "processed_files\t{}",
        summary.scanned_files.saturating_sub(errors.len())
    );
    println!("skipped_error_files\t{}", errors.len());
    println!("skipped_unknown\t{}", summary.skipped_unknown);
    println!("skipped_hidden\t{}", summary.skipped_hidden);
    println!("skipped_generated\t{}", summary.skipped_generated);
    println!("skipped_symlink\t{}", summary.skipped_symlink);
    println!("skipped_excluded\t{}", summary.skipped_excluded);
}

fn print_errors_text(errors: &[CorpusFileError]) {
    for error in errors {
        println!(
            "error\t{}\t{}\t{}",
            safe_text!(error.path.display()),
            error.stage,
            safe_text!(error.message),
        );
    }
}
