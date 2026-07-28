use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::narrowing_report::usecase::{NarrowingPolicy, NarrowingReportFile, NarrowingSite};

pub fn print_narrowing_report(
    reports: &[NarrowingReportFile],
    policy: &NarrowingPolicy,
    binding: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => print_text(reports, policy, binding),
        OutputFormat::Json => print_json(reports, policy, binding)?,
    }
    Ok(())
}

/// Whether a site survives the `--binding` filter.
///
/// Compared case-insensitively because the Common Lisp reader folds symbols:
/// a caller who types `--binding x` means the binding the file spells `X`.
fn selected(site: &NarrowingSite, binding: Option<&str>) -> bool {
    binding.is_none_or(|name| site.binding.eq_ignore_ascii_case(name))
}

fn print_text(reports: &[NarrowingReportFile], policy: &NarrowingPolicy, binding: Option<&str>) {
    println!("files\t{}", reports.len());
    println!("site_count\t{}", policy.site_count);
    if policy.fail_on_none {
        println!("policy\tfail_on_none=true\tpassed={}", policy.passed);
    }

    for report in reports {
        if !report.dialect_modelled {
            println!(
                "unmodelled\t{}\t{}\tthe type layer models Common Lisp only",
                safe_text!(report.path.display()),
                report.dialect.label(),
            );
            continue;
        }
        for site in report.sites.iter().filter(|site| selected(site, binding)) {
            println!(
                "narrowing\t{}\t{}\t{}\t{}\tto={}\tbranch={}\tsource={}\tform={}\ttest={}",
                safe_text!(report.path.display()),
                site.line,
                site.span.start().get(),
                safe_text!(site.binding),
                site.narrowed_to.as_str(),
                site.branch.label(),
                site.source.label(),
                safe_text!(site.form),
                safe_text!(site.test),
            );
        }
    }
}

fn print_json(
    reports: &[NarrowingReportFile],
    policy: &NarrowingPolicy,
    binding: Option<&str>,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "site_count": policy.site_count,
            "binding_filter": binding,
            "policy": {
                "fail_on_none": policy.fail_on_none,
                "passed": policy.passed,
                "violations": &policy.violations,
            },
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "dialect_modelled": report.dialect_modelled,
                    "sites": report
                        .sites
                        .iter()
                        .filter(|site| selected(site, binding))
                        .map(|site| json!({
                            "binding": site.binding,
                            "narrowed_to": site.narrowed_to.as_str(),
                            "branch": site.branch.label(),
                            "source": site.source.label(),
                            "form": site.form,
                            "test": site.test,
                            "line": site.line,
                            "span": {
                                "start": site.span.start().get(),
                                "end": site.span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
