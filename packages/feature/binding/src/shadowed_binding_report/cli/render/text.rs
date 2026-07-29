use super::*;
use paredit_core_cli::safe_text;

pub fn print_shadowed_binding_report(
    reports: &[ShadowedBindingReportFile],
    policy: &ShadowedBindingPolicy,
) {
    println!("files\t{}", reports.len());
    println!("scope_count\t{}", policy.scope_count);
    println!("shadowed_count\t{}", policy.shadowed_count);
    if policy.fail_on_shadowed {
        println!("policy\tfail_on_shadowed=true\tpassed={}", policy.passed);
    }

    for report in reports {
        for item in &report.shadowed {
            println!(
                "shadowed\t{}\t{}\tname={}\tinner={}..{}\touter_kind={}\touter={}..{}",
                safe_text!(report.path.display()),
                report.dialect.label(),
                safe_text!(item.name),
                item.inner_span.start().get(),
                item.inner_span.end().get(),
                item.outer_kind.label(),
                item.outer_span.start().get(),
                item.outer_span.end().get(),
            );
        }
    }
}
