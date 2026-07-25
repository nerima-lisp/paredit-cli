use super::*;

pub(super) fn print_unused_local_callable_report(
    reports: &[UnusedLocalCallableReportFile],
    policy: &UnusedLocalCallablePolicy,
) {
    println!("files\t{}", reports.len());
    println!("checked_binding_count\t{}", policy.checked_binding_count);
    println!("unused_count\t{}", policy.unused_count);
    if policy.fail_on_unused {
        println!("policy\tfail_on_unused=true\tpassed={}", policy.passed);
    }

    for report in reports {
        for item in &report.unused {
            println!(
                "unused-local-callable\t{}\t{}\t{}\tname={}\tspan={}..{}",
                safe_text!(report.path.display()),
                report.dialect.label(),
                safe_text!(item.form_head),
                safe_text!(item.name),
                item.form_span.start().get(),
                item.form_span.end().get(),
            );
        }
    }
}
