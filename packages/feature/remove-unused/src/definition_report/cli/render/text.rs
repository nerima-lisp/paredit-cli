use super::*;

pub(super) fn print_definition_report(
    reports: &[DefinitionReportFile],
    summary: &DefinitionReportSummary,
) {
    println!("files\t{}", reports.len());
    println!("definition_count\t{}", summary.definition_count);
    for (category, count) in &summary.by_category {
        println!("category\t{}\t{count}", category.label());
    }
    for report in reports {
        println!(
            "{}\t{}\tdefinitions={}\tpackage={}",
            safe_text!(report.path.display()),
            report.dialect.label(),
            report.definitions.len(),
            safe_text!(report.package.as_deref().unwrap_or(""))
        );
        for definition in &report.definitions {
            println!(
                "\t{}\t{}\t{}\t{}..{}\tparams={}\tbody={}\tpackage={}",
                definition.category.label(),
                safe_text!(definition.head),
                safe_text!(definition.name.as_deref().unwrap_or("")),
                definition.span.start().get(),
                definition.span.end().get(),
                definition
                    .parameter_count
                    .map(|count| count.to_string())
                    .unwrap_or_default(),
                definition
                    .body_form_count
                    .map(|count| count.to_string())
                    .unwrap_or_default(),
                safe_text!(definition.package.as_deref().unwrap_or(""))
            );
        }
    }
}

pub(super) fn print_unused_definition_report(
    reports: &[UnusedDefinitionFile],
    policy: &UnusedDefinitionPolicy,
) {
    println!("files\t{}", reports.len());
    println!("definition_count\t{}", policy.definition_count);
    println!("candidate_count\t{}", policy.candidate_count);
    println!(
        "actionable_candidate_count\t{}",
        policy.actionable_candidate_count
    );
    println!("policy_passed\t{}", policy.passed);
    for violation in &policy.violations {
        println!("policy_violation\t{}", safe_text!(violation));
    }
    for report in reports {
        println!(
            "{}\t{}\tdefinitions={}\tcandidates={}\tpackage={}",
            safe_text!(report.path.display()),
            report.dialect.label(),
            report.definitions.len(),
            unused_candidate_count(report),
            safe_text!(report.package.as_deref().unwrap_or(""))
        );
        for item in &report.definitions {
            let definition = &item.definition;
            println!(
                "\t{}\t{}\t{}\t{}..{}\treferences={}\tunused={}\tbulk_removable={}",
                definition.category.label(),
                safe_text!(definition.head),
                safe_text!(definition.name.as_deref().unwrap_or("")),
                definition.span.start().get(),
                definition.span.end().get(),
                item.references.len(),
                item.references.is_empty(),
                definition.category.is_bulk_removable()
            );
        }
    }
}
