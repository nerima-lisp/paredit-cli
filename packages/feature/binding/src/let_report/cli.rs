use super::*;
use crate::application::usecase::let_report::{
    LetFormReport, LetReportPolicy, LetReportPolicyOptions, build_let_report,
    evaluate_let_report_policy,
};
use anyhow::anyhow;

#[derive(Debug, Args)]
pub(super) struct LetReportArgs {
    /// Files to scan. Omit to read a single snippet from stdin; pass two or
    /// more to get a per-file breakdown with an aggregated policy.
    files: Vec<PathBuf>,
    #[arg(long)]
    dialect: Option<DialectArg>,
    #[arg(long)]
    fail_on_duplicate_evaluation: bool,
    #[arg(long)]
    fail_on_unused_binding: bool,
    #[arg(long)]
    require_inlineable_bindings: Option<usize>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub(super) fn let_report(args: LetReportArgs) -> Result<()> {
    let options = LetReportPolicyOptions::new(
        args.fail_on_duplicate_evaluation,
        args.fail_on_unused_binding,
        args.require_inlineable_bindings,
    )
    .map_err(crate::presentation::cli::gate::gate_failure)?;

    if args.files.len() > 1 {
        type FileLetReport = (PathBuf, Dialect, Vec<LetFormReport>);
        // Per-file read+parse+analysis is independent, so it fans out across
        // workers; results are reassembled by index so both the per-file
        // output order and first-error selection match the sequential loop.
        let worker_count = std::thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1)
            .clamp(1, args.files.len());
        let mut ordered: Vec<Option<Result<FileLetReport>>> =
            (0..args.files.len()).map(|_| None).collect();
        std::thread::scope(|scope| -> Result<()> {
            let files = &args.files;
            let explicit = args.dialect;
            let handles: Vec<_> = (0..worker_count)
                .map(|worker| {
                    scope.spawn(move || {
                        files
                            .iter()
                            .enumerate()
                            .skip(worker)
                            .step_by(worker_count)
                            .map(|(index, file)| {
                                let report =
                                    read_input_dialect_and_tree(Some(file.clone()), explicit)
                                        .and_then(|(input, dialect, tree)| {
                                            Ok((
                                                file.clone(),
                                                dialect,
                                                build_let_report(dialect, &input.text, &tree)?,
                                            ))
                                        });
                                (index, report)
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            for handle in handles {
                for (index, report) in handle
                    .join()
                    .map_err(|_| anyhow!("let-report worker thread panicked"))?
                {
                    ordered[index] = Some(report);
                }
            }
            Ok(())
        })?;
        let mut per_file = Vec::with_capacity(args.files.len());
        let mut all_reports = Vec::new();
        for entry in ordered.into_iter().flatten() {
            let (file, dialect, reports) = entry?;
            all_reports.extend(reports.iter().cloned());
            per_file.push((file, dialect, reports));
        }
        let policy = evaluate_let_report_policy(&all_reports, &options);
        print_multi_file_let_report(&per_file, &policy, args.output)?;
        if !policy.passed {
            return Err(crate::presentation::cli::gate::gate_failure(format!(
                "let-report policy failed: {}",
                policy.violations.join("; ")
            )));
        }
        return Ok(());
    }

    let (input, dialect, tree) =
        read_input_dialect_and_tree(args.files.into_iter().next(), args.dialect)?;
    let reports = build_let_report(dialect, &input.text, &tree)?;
    let policy = evaluate_let_report_policy(&reports, &options);
    print_let_report(dialect, &reports, &policy, args.output)?;
    if !policy.passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "let-report policy failed: {}",
            policy.violations.join("; ")
        )));
    }
    Ok(())
}

fn print_let_report(
    dialect: Dialect,
    reports: &[LetFormReport],
    policy: &LetReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", dialect.label());
            println!("let_forms\t{}", reports.len());
            print_policy_summary_text(policy);
            print_let_forms_text(reports);
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": dialect.label(),
                "let_form_count": reports.len(),
                "summary": policy_summary_json(policy),
                "policy": policy_json(policy),
                "let_forms": let_forms_json(reports),
            }))?
        ),
    }
    Ok(())
}

fn print_multi_file_let_report(
    per_file: &[(PathBuf, Dialect, Vec<LetFormReport>)],
    policy: &LetReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("file_count\t{}", per_file.len());
            print_policy_summary_text(policy);
            for (file, dialect, reports) in per_file {
                println!(
                    "file\t{}\tdialect={}\tlet_forms={}",
                    safe_text!(file.display()),
                    dialect.label(),
                    reports.len()
                );
                print_let_forms_text(reports);
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "file_count": per_file.len(),
                "summary": policy_summary_json(policy),
                "policy": policy_json(policy),
                "files": per_file
                    .iter()
                    .map(|(file, dialect, reports)| json!({
                        "path": file.display().to_string(),
                        "dialect": dialect.label(),
                        "let_form_count": reports.len(),
                        "let_forms": let_forms_json(reports),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}

fn print_policy_summary_text(policy: &LetReportPolicy) {
    println!("binding_count\t{}", policy.binding_count);
    println!(
        "inlineable_binding_count\t{}",
        policy.inlineable_binding_count
    );
    println!("unused_binding_count\t{}", policy.unused_binding_count);
    println!(
        "duplicate_evaluation_count\t{}",
        policy.duplicate_evaluation_count
    );
    println!("policy_passed\t{}", policy.passed);
    for violation in &policy.violations {
        println!("policy_violation\t{}", safe_text!(violation));
    }
}

fn print_let_forms_text(reports: &[LetFormReport]) {
    for report in reports {
        println!(
            "{}\t{}\t{}..{}\tbindings={}\tbody_count={}\tinline_supported={}",
            safe_text!(report.path),
            safe_text!(report.form),
            report.span.start().get(),
            report.span.end().get(),
            report.bindings.len(),
            report.body_count,
            report.inline_supported_by_inline_let
        );
        for binding in &report.bindings {
            println!(
                "\t{}\tvalue_span={}..{}\treferences={}\tcan_inline={}\trisks={}",
                safe_text!(binding.name),
                binding.value_span.start().get(),
                binding.value_span.end().get(),
                binding.reference_count,
                binding.can_inline_without_duplication,
                safe_text!(binding.risks.join(","))
            );
        }
    }
}

fn policy_summary_json(policy: &LetReportPolicy) -> Value {
    json!({
        "binding_count": policy.binding_count,
        "inlineable_binding_count": policy.inlineable_binding_count,
        "unused_binding_count": policy.unused_binding_count,
        "duplicate_evaluation_count": policy.duplicate_evaluation_count,
    })
}

fn policy_json(policy: &LetReportPolicy) -> Value {
    json!({
        "fail_on_duplicate_evaluation": policy.fail_on_duplicate_evaluation,
        "fail_on_unused_binding": policy.fail_on_unused_binding,
        "require_inlineable_bindings": policy.require_inlineable_bindings,
        "passed": policy.passed,
        "violations": &policy.violations,
    })
}

fn let_forms_json(reports: &[LetFormReport]) -> Vec<Value> {
    reports
        .iter()
        .map(|report| {
            json!({
                "path": report.path.to_string(),
                "form": report.form.as_str(),
                "span": {
                    "start": report.span.start().get(),
                    "end": report.span.end().get(),
                },
                "binding_style": report.binding_style,
                "body_count": report.body_count,
                "inline_supported_by_inline_let": report.inline_supported_by_inline_let,
                "bindings": report
                    .bindings
                    .iter()
                    .map(|binding| json!({
                        "name": binding.name.as_str(),
                        "value": binding.value.as_str(),
                        "value_span": {
                            "start": binding.value_span.start().get(),
                            "end": binding.value_span.end().get(),
                        },
                        "reference_count": binding.reference_count,
                        "can_inline_without_duplication": binding.can_inline_without_duplication,
                        "risks": &binding.risks,
                    }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>()
}
