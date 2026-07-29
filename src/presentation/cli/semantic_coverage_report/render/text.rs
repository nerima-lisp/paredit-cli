use crate::presentation::cli::terminal_safe;
use crate::semantic_coverage::{SemanticCoveragePolicy, SemanticCoverageReport};

use super::percentage;

pub(super) fn print_semantic_coverage_report(
    report: &SemanticCoverageReport,
    policy: &SemanticCoveragePolicy,
    top: usize,
) {
    let variable_bindings = report.total_variable_bindings();
    let resolved_bindings = report.total_resolved_bindings();
    let list_expressions = report.total_list_expressions();
    let known_list_expressions = report.total_known_list_expressions();
    let non_resolution = report.total_non_resolution();

    println!("files\t{}", report.files().len());
    println!(
        "variable_bindings\t{resolved_bindings}/{variable_bindings}\t{}",
        display_percent(percentage(resolved_bindings, variable_bindings))
    );
    println!(
        "list_expressions\t{known_list_expressions}/{list_expressions}\t{}",
        display_percent(percentage(known_list_expressions, list_expressions))
    );
    if let Some(threshold) = policy.threshold {
        println!(
            "policy\t--fail-under={threshold:.1}\tpassed={}",
            policy.passed
        );
    }

    // `R2`: per-dialect rows, so Common Lisp's rate sits next to every other
    // dialect this run measured rather than a single corpus-wide number that
    // would hide which dialects the semantic layer does not model yet.
    println!("\n== by dialect ==");
    for (dialect, totals) in report.by_dialect() {
        println!(
            "{}\tfiles={}\tbindings={}/{} ({})\tlists={}/{} ({})",
            dialect.label(),
            totals.file_count,
            totals.resolved_bindings,
            totals.variable_bindings,
            display_percent(percentage(
                totals.resolved_bindings,
                totals.variable_bindings
            )),
            totals.known_list_expressions,
            totals.list_expressions,
            display_percent(percentage(
                totals.known_list_expressions,
                totals.list_expressions
            )),
        );
    }

    println!("\n== per file ==");
    for file in report.files() {
        println!(
            "{}\t{}\tbindings={}/{} ({})\tlists={}/{} ({})",
            terminal_safe(&file.path().display()),
            file.dialect().label(),
            file.resolved_binding_count(),
            file.variable_binding_count(),
            display_percent(percentage(
                file.resolved_binding_count(),
                file.variable_binding_count()
            )),
            file.known_list_expression_count(),
            file.list_expression_count(),
            display_percent(percentage(
                file.known_list_expression_count(),
                file.list_expression_count()
            )),
        );
    }

    if !report.errors().is_empty() {
        println!("\n== errors ==");
        for error in report.errors() {
            println!(
                "{}\t{}\t{}",
                terminal_safe(&error.path.display()),
                error.stage.label(),
                terminal_safe(&error.message),
            );
        }
    }

    let unresolved = variable_bindings - resolved_bindings;
    println!("\n== non-resolution breakdown (of {unresolved} unresolved bindings) ==");
    print_reason("reassigned", non_resolution.reassigned(), unresolved);
    print_reason("opaque_scope", non_resolution.opaque_scope(), unresolved);
    print_reason("special", non_resolution.special(), unresolved);
    print_reason(
        "no_initial_form",
        non_resolution.no_initial_form(),
        unresolved,
    );
    print_reason(
        "initial_form_not_constant",
        non_resolution.initial_form_not_constant(),
        unresolved,
    );
    print_reason(
        "initial_form_not_propagatable",
        non_resolution.initial_form_not_propagatable(),
        unresolved,
    );

    // `R4`: the actionable suggestion this report exists to give — which head
    // to register in the transparency table next, ranked by how many opaque
    // scopes it alone is blocking.
    println!(
        "\n== suggested next operators to register (top {top} of {} opaque scopes) ==",
        non_resolution.opaque_scope()
    );
    for (label, count) in non_resolution.ranked_opacity_causes().into_iter().take(top) {
        println!(
            "{}\t{count}\t{}",
            terminal_safe(&label.display()),
            display_percent(percentage(count, non_resolution.opaque_scope()))
        );
    }

    println!(
        "\n== which binder left the most bindings uninitialized (top {top} of {}) ==",
        non_resolution.no_initial_form()
    );
    for (head, count) in non_resolution
        .ranked_uninitialized_binders()
        .into_iter()
        .take(top)
    {
        println!(
            "{}\t{count}\t{}",
            terminal_safe(head.as_deref().unwrap_or("<no binder>")),
            display_percent(percentage(count, non_resolution.no_initial_form()))
        );
    }
}

fn print_reason(label: &str, count: usize, unresolved: usize) {
    println!(
        "{label}\t{count}\t{}",
        display_percent(percentage(count, unresolved))
    );
}

fn display_percent(percent: Option<f64>) -> String {
    percent.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.1}%"))
}
