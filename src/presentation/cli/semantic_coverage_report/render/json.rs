use paredit_core_cli::CliResult;
use serde_json::json;

use crate::semantic_coverage::{SemanticCoveragePolicy, SemanticCoverageReport};

use super::percentage;

pub(super) fn print_semantic_coverage_report(
    report: &SemanticCoverageReport,
    policy: &SemanticCoveragePolicy,
    top: usize,
) -> CliResult<()> {
    let variable_bindings = report.total_variable_bindings();
    let resolved_bindings = report.total_resolved_bindings();
    let list_expressions = report.total_list_expressions();
    let known_list_expressions = report.total_known_list_expressions();
    let non_resolution = report.total_non_resolution();
    let unresolved = variable_bindings - resolved_bindings;

    let by_dialect = report
        .by_dialect()
        .into_iter()
        .map(|(dialect, totals)| {
            json!({
                "dialect": dialect.label(),
                "file_count": totals.file_count,
                "variable_bindings": totals.variable_bindings,
                "resolved_bindings": totals.resolved_bindings,
                "resolved_percent": percentage(totals.resolved_bindings, totals.variable_bindings),
                "list_expressions": totals.list_expressions,
                "known_list_expressions": totals.known_list_expressions,
                "known_percent": percentage(totals.known_list_expressions, totals.list_expressions),
            })
        })
        .collect::<Vec<_>>();

    let opacity_causes = non_resolution
        .ranked_opacity_causes()
        .into_iter()
        .take(top)
        .map(|(label, count)| {
            json!({
                "label": label.display(),
                "count": count,
                "percent_of_opaque_scopes": percentage(count, non_resolution.opaque_scope()),
            })
        })
        .collect::<Vec<_>>();

    let uninitialized_binders = non_resolution
        .ranked_uninitialized_binders()
        .into_iter()
        .take(top)
        .map(|(binder, count)| {
            json!({
                "binder": binder,
                "count": count,
                "percent_of_no_initial_form": percentage(count, non_resolution.no_initial_form()),
            })
        })
        .collect::<Vec<_>>();

    let payload = json!({
        "schema_version": 1,
        "file_count": report.files().len(),
        "errors": report
            .errors()
            .iter()
            .map(|error| json!({
                "path": error.path.display().to_string(),
                "stage": error.stage.label(),
                "message": error.message,
            }))
            .collect::<Vec<_>>(),
        "totals": {
            "variable_bindings": variable_bindings,
            "resolved_bindings": resolved_bindings,
            "resolved_percent": percentage(resolved_bindings, variable_bindings),
            "list_expressions": list_expressions,
            "known_list_expressions": known_list_expressions,
            "known_percent": percentage(known_list_expressions, list_expressions),
        },
        // `R2`: only Common Lisp resolves anything today. Broken out per
        // dialect so that fact is a number next to every other dialect's
        // figures, not a claim to take on faith.
        "by_dialect": by_dialect,
        "non_resolution": {
            "unresolved_total": unresolved,
            "reassigned": non_resolution.reassigned(),
            "opaque_scope": non_resolution.opaque_scope(),
            "special": non_resolution.special(),
            "no_initial_form": non_resolution.no_initial_form(),
            "initial_form_not_constant": non_resolution.initial_form_not_constant(),
            "initial_form_not_propagatable": non_resolution.initial_form_not_propagatable(),
        },
        // `R4`: the highest-count `opacity_causes` entries name the head an
        // agent would get the most resolution mileage from registering in
        // the transparency table next; `uninitialized_binders` names which
        // binder leaves the most bindings with no value to propagate at all.
        "opacity_causes": opacity_causes,
        "uninitialized_binders": uninitialized_binders,
        "files": report
            .files()
            .iter()
            .map(|file| json!({
                "path": file.path().display().to_string(),
                "dialect": file.dialect().label(),
                "variable_bindings": file.variable_binding_count(),
                "resolved_bindings": file.resolved_binding_count(),
                "list_expressions": file.list_expression_count(),
                "known_list_expressions": file.known_list_expression_count(),
            }))
            .collect::<Vec<_>>(),
        "policy": {
            "fail_under": policy.threshold,
            "resolved_percent": policy.resolved_percent,
            "passed": policy.passed,
            "message": policy.message,
        },
    });

    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}
