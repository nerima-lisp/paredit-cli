use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::effect_report::cli::args::PurityFilter;
use crate::effect_report::usecase::{DefinitionEffect, EffectPolicy, EffectReportFile};

pub fn print_effect_report(
    reports: &[EffectReportFile],
    policy: &EffectPolicy,
    filter: Option<PurityFilter>,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => print_text(reports, policy, filter),
        OutputFormat::Json => print_json(reports, policy, filter)?,
    }
    Ok(())
}

/// Whether a definition survives the `--purity` filter.
///
/// Compared through the label rather than by converting the CLI enum into the
/// domain one: the two are the same three words, and a conversion function
/// would be a second place for them to drift apart.
fn selected(definition: &DefinitionEffect, filter: Option<PurityFilter>) -> bool {
    filter.is_none_or(|filter| filter.label() == definition.purity.label())
}

fn print_text(reports: &[EffectReportFile], policy: &EffectPolicy, filter: Option<PurityFilter>) {
    println!("files\t{}", reports.len());
    println!("definition_count\t{}", policy.definition_count);
    for (purity, count) in &policy.by_purity {
        println!("{}\t{count}", purity.label());
    }
    if policy.fail_on_unknown {
        println!("policy\tfail_on_unknown=true\tpassed={}", policy.passed);
    }

    for report in reports {
        if !report.dialect_modelled {
            println!(
                "unmodelled\t{}\t{}\tthe effect tables cover Common Lisp only",
                safe_text!(report.path.display()),
                report.dialect.label(),
            );
            continue;
        }
        for definition in report
            .definitions
            .iter()
            .filter(|definition| selected(definition, filter))
        {
            println!(
                "{}\t{}\t{}\t{}\thead={}\tcause={}\tinherited={}",
                definition.purity.label(),
                safe_text!(report.path.display()),
                definition.line,
                safe_text!(definition.name),
                safe_text!(definition.head),
                safe_text!(definition.cause.as_deref().unwrap_or("-")),
                definition.inherited,
            );
        }
    }
}

fn print_json(
    reports: &[EffectReportFile],
    policy: &EffectPolicy,
    filter: Option<PurityFilter>,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "definition_count": policy.definition_count,
            "by_purity": policy
                .by_purity
                .iter()
                .map(|(purity, count)| json!({
                    "purity": purity.label(),
                    "count": count,
                }))
                .collect::<Vec<_>>(),
            "purity_filter": filter.map(PurityFilter::label),
            "policy": {
                "fail_on_unknown": policy.fail_on_unknown,
                "passed": policy.passed,
                "violations": &policy.violations,
            },
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "dialect_modelled": report.dialect_modelled,
                    "definitions": report
                        .definitions
                        .iter()
                        .filter(|definition| selected(definition, filter))
                        .map(|definition| json!({
                            "name": definition.name,
                            "head": definition.head,
                            "purity": definition.purity.label(),
                            "cause": definition.cause,
                            "inherited": definition.inherited,
                            "calls": definition.calls,
                            "line": definition.line,
                            "span": {
                                "start": definition.span.start().get(),
                                "end": definition.span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
