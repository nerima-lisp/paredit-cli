use super::super::super::types::execute::{
    WorkspaceRefactorExecute, WorkspaceRefactorExecuteOutcome,
};
use super::super::write_plan::print_refactor_write_plan;
use crate::refactor::usecase::execute::RefactorExecuteDecision;
use anyhow::Result;
use paredit_core_cli::safe_text;

pub fn print_workspace_refactor_execute_text(execution: &WorkspaceRefactorExecute) -> Result<()> {
    let preview = &execution.preview;
    let write_plan = preview.write_plan();
    let writable_files = preview.writable_paths_for_write_plan(&write_plan);
    let refused_files = preview.refused_paths_for_write_plan(&write_plan);
    println!("command\trefactor workspace-execute");
    println!("mode\t{}", preview.mode.label());
    println!("from\t{}", safe_text!(preview.from));
    println!("to\t{}", safe_text!(preview.to));
    println!("write_requested\t{}", preview.write_requested);
    print_refactor_write_plan(&write_plan, &writable_files, &refused_files);
    print_decision("preflight_decision", &execution.preflight_decision);
    print_decision("execute_decision", &execution.execute_decision);
    print_outcome("outcome", &execution.outcome);
    println!(
        "changed_file_count\t{}",
        preview.summary.changed_file_count()
    );
    for changed_file in preview.summary.changed_files() {
        println!("changed_file\t{}", safe_text!(changed_file));
    }
    println!(
        "unchanged_file_count\t{}",
        preview.summary.unchanged_file_count()
    );
    println!(
        "written_file_count\t{}",
        preview.summary.written_file_count()
    );
    println!("edit_count\t{}", preview.summary.edit_count());
    println!("parse_error_count\t{}", preview.summary.parse_error_count());
    println!("policy_passed\t{}", preview.policy.passed());
    let policy_summary = preview.policy.summary();
    println!("policy_violation_count\t{}", policy_summary.violation_count);
    println!("policy_write_blocked\t{}", policy_summary.write_blocked);
    println!(
        "policy_next_action\t{}",
        safe_text!(policy_summary.next_action)
    );
    if let Some(verification) = &execution.pre_verification {
        println!("pre_verification_passed\t{}", verification.passed);
        for check in &verification.checks {
            println!(
                "pre_check\t{}\t{}\tpassed={}\tcount={}\t{}",
                check.level.label(),
                safe_text!(check.code),
                check.passed,
                check.count,
                safe_text!(check.message)
            );
        }
    }
    if let Some(verification) = &execution.post_verification {
        println!("post_verification_passed\t{}", verification.passed);
        for check in &verification.checks {
            println!(
                "post_check\t{}\t{}\tpassed={}\tcount={}\t{}",
                check.level.label(),
                safe_text!(check.code),
                check.passed,
                check.count,
                safe_text!(check.message)
            );
        }
    }

    Ok(())
}

fn print_decision(label: &str, decision: &RefactorExecuteDecision) {
    let status = decision.status();
    let summary = decision.summary();

    println!("{}\tstatus\t{}", label, status.label());
    println!("{}\treason\t{}", label, safe_text!(status.reason()));
    println!(
        "{}\tnext_action\t{}",
        label,
        safe_text!(status.next_action())
    );
    for step in decision.steps() {
        println!(
            "{}\tstep\t{}\t{}",
            label,
            safe_text!(step.name),
            step.status.label()
        );
    }
    println!(
        "{}\tpassed_step_count\t{}",
        label,
        summary.passed_step_count()
    );
    println!(
        "{}\tfailed_step_count\t{}",
        label,
        summary.failed_step_count()
    );
    println!(
        "{}\tskipped_step_count\t{}",
        label,
        summary.skipped_step_count()
    );
    println!(
        "{}\tscheduled_step_count\t{}",
        label,
        summary.scheduled_step_count()
    );
    println!(
        "{}\twrite_parse_refused\t{}",
        label,
        decision.write_parse_refused()
    );
    println!(
        "{}\trun_pre_verification\t{}",
        label,
        decision.run_pre_verification()
    );
    println!("{}\tapply_preview\t{}", label, decision.apply_preview());
    println!(
        "{}\trun_post_verification\t{}",
        label,
        decision.run_post_verification()
    );
}

fn print_outcome(label: &str, outcome: &WorkspaceRefactorExecuteOutcome) {
    let summary = outcome.summary();

    println!("{}\tstatus\t{}", label, outcome.status().label());
    println!(
        "{}\treason\t{}",
        label,
        safe_text!(outcome.status().reason())
    );
    println!(
        "{}\tnext_action\t{}",
        label,
        safe_text!(outcome.status().next_action())
    );
    for step in outcome.steps() {
        println!(
            "{}\tstep\t{}\t{}",
            label,
            safe_text!(step.name),
            step.status.label()
        );
    }
    println!(
        "{}\tpassed_step_count\t{}",
        label,
        summary.passed_step_count()
    );
    println!(
        "{}\tfailed_step_count\t{}",
        label,
        summary.failed_step_count()
    );
    println!(
        "{}\tskipped_step_count\t{}",
        label,
        summary.skipped_step_count()
    );
    println!(
        "{}\tscheduled_step_count\t{}",
        label,
        summary.scheduled_step_count()
    );
    println!("{}\twrite_applied\t{}", label, outcome.write_applied());
    match outcome.post_verification_passed() {
        Some(passed) => println!("{label}\tpost_verification_passed\t{passed}"),
        None => println!("{label}\tpost_verification_passed\tnull"),
    }
}
