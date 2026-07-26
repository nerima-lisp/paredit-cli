use super::super::*;
use crate::application::usecase::duplicate_report::{
    DuplicateFormReport, DuplicateShapeReport, ReplacementPlanBatch,
};

pub(in crate::presentation::cli) fn print_duplicate_report(
    reports: &[DuplicateShapeReport],
    output: OutputFormat,
) -> Result<()> {
    let form_count = reports
        .iter()
        .map(|report| report.forms.len())
        .sum::<usize>();

    match output {
        OutputFormat::Text => {
            println!("group_count\t{}", reports.len());
            println!("form_count\t{form_count}");
            for report in reports {
                println!(
                    "shape\t{}\tcount={}",
                    safe_text!(report.shape),
                    report.count
                );
                for form in &report.forms {
                    println!(
                        "\t{}\t{}\t{}\t{}..{}\tnodes={}\thead={}",
                        safe_text!(form.path.display()),
                        form.dialect.label(),
                        safe_text!(form.form_path),
                        form.span.start().get(),
                        form.span.end().get(),
                        form.node_count,
                        safe_text!(form.head.as_deref().unwrap_or(""))
                    );
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "group_count": reports.len(),
                "form_count": form_count,
                "groups": reports
                    .iter()
                    .map(|report| json!({
                        "shape": report.shape.as_str(),
                        "count": report.count,
                        "forms": report
                            .forms
                            .iter()
                            .map(|form| json!({
                                "path": form.path.display().to_string(),
                                "dialect": form.dialect.label(),
                                "form_path": form.form_path.to_string(),
                                "span": {
                                    "start": form.span.start().get(),
                                    "end": form.span.end().get(),
                                },
                                "node_count": form.node_count,
                                "head": form.head.as_deref(),
                                "text": form.text.as_str(),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}

pub(in crate::presentation::cli) fn print_replacement_plan(
    batches: &[ReplacementPlanBatch],
    output: OutputFormat,
) -> Result<()> {
    let form_count = batches
        .iter()
        .map(|batch| replacement_forms(batch).len())
        .sum::<usize>();
    let candidate_form_count = batches.iter().map(|batch| batch.forms.len()).sum::<usize>();

    match output {
        OutputFormat::Text => {
            println!("batch_count\t{}", batches.len());
            println!("form_count\t{form_count}");
            println!("candidate_form_count\t{candidate_form_count}");
            for batch in batches {
                let command_args = replace_forms_command_args(batch);
                let targets = replacement_forms(batch);
                println!(
                    "batch\t{}\t{}\tcount={}\tcandidate_count={}\tshape={}",
                    safe_text!(batch.file.display()),
                    batch.dialect.label(),
                    targets.len(),
                    batch.forms.len(),
                    safe_text!(batch.shape)
                );
                if let Some(form) = kept_form(batch) {
                    println!(
                        "kept\t{}\t{}..{}\tnodes={}\thead={}",
                        safe_text!(form.form_path),
                        form.span.start().get(),
                        form.span.end().get(),
                        form.node_count,
                        safe_text!(form.head.as_deref().unwrap_or(""))
                    );
                }
                println!("command\t{}", safe_text!(format_command(&command_args)));
                for form in targets {
                    println!(
                        "\t{}\t{}..{}\tnodes={}\thead={}",
                        safe_text!(form.form_path),
                        form.span.start().get(),
                        form.span.end().get(),
                        form.node_count,
                        safe_text!(form.head.as_deref().unwrap_or(""))
                    );
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "batch_count": batches.len(),
                "form_count": form_count,
                "candidate_form_count": candidate_form_count,
                "batches": batches
                    .iter()
                    .map(|batch| {
                        let command_args = replace_forms_command_args(batch);
                        let targets = replacement_forms(batch);
                        json!({
                            "file": batch.file.display().to_string(),
                            "dialect": batch.dialect.label(),
                            "shape": batch.shape.as_str(),
                            "count": targets.len(),
                            "candidate_count": batch.forms.len(),
                            "replacement_count": targets.len(),
                            "keep_first": batch.keep_first,
                            "kept_form": kept_form(batch).map(|form| json!({
                                "form_path": form.form_path.to_string(),
                                "span": {
                                    "start": form.span.start().get(),
                                    "end": form.span.end().get(),
                                },
                                "node_count": form.node_count,
                                "head": form.head.as_deref(),
                                "text": form.text.as_str(),
                            })),
                            "replacement": batch.replacement.as_str(),
                            "paths": targets
                                .iter()
                                .map(|form| form.form_path.to_string())
                                .collect::<Vec<_>>(),
                            "replace_forms_args": command_args,
                            "command": format_command(&replace_forms_command_args(batch)),
                            "forms": batch
                                .forms
                                .iter()
                                .map(|form| json!({
                                    "form_path": form.form_path.to_string(),
                                    "span": {
                                        "start": form.span.start().get(),
                                        "end": form.span.end().get(),
                                    },
                                    "node_count": form.node_count,
                                    "head": form.head.as_deref(),
                                    "text": form.text.as_str(),
                                }))
                                .collect::<Vec<_>>(),
                        })
                    })
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}

fn replace_forms_command_args(batch: &ReplacementPlanBatch) -> Vec<String> {
    let mut args = vec![
        "paredit".to_owned(),
        "replace-forms".to_owned(),
        "--file".to_owned(),
        batch.file.display().to_string(),
    ];

    for form in replacement_forms(batch) {
        args.push("--path".to_owned());
        args.push(form.form_path.to_string());
    }

    args.extend([
        "--with".to_owned(),
        batch.replacement.clone(),
        "--require-same-shape".to_owned(),
        "--output".to_owned(),
        "json".to_owned(),
    ]);

    args
}

fn replacement_forms(batch: &ReplacementPlanBatch) -> &[DuplicateFormReport] {
    if batch.keep_first {
        batch.forms.get(1..).unwrap_or(&[])
    } else {
        &batch.forms
    }
}

fn kept_form(batch: &ReplacementPlanBatch) -> Option<&DuplicateFormReport> {
    batch.keep_first.then(|| batch.forms.first()).flatten()
}

fn format_command(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg.bytes().all(|byte| {
            matches!(
                byte,
                b'a'..=b'z'
                    | b'A'..=b'Z'
                    | b'0'..=b'9'
                    | b'/'
                    | b'.'
                    | b'_'
                    | b'-'
                    | b':'
                    | b'='
            )
        })
    {
        return arg.to_owned();
    }

    format!("'{}'", arg.replace('\'', "'\\''"))
}
