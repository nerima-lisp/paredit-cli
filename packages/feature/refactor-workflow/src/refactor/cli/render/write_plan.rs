use serde_json::{Value, json};

use crate::application::refactor::execute::{RefactorWritePlan, RefactorWriteRefusal};

pub(super) fn print_refactor_write_plan(
    write_plan: &RefactorWritePlan,
    writable_files: &[String],
    refused_files: &[String],
) {
    println!(
        "write_plan_write_requested\t{}",
        write_plan.write_requested()
    );
    println!("write_plan_write_allowed\t{}", write_plan.write_allowed());
    println!(
        "write_plan_writable_file_count\t{}",
        write_plan.writable_indexes().len()
    );
    for path in writable_files {
        println!("write_plan_writable_file\t{}", safe_text!(path));
    }
    println!("write_plan_refused_file_count\t{}", refused_files.len());
    for path in refused_files {
        println!("write_plan_refused_file\t{}", safe_text!(path));
    }
    match write_plan.refusal() {
        Some(refusal) => {
            println!("write_plan_refusal\t{}", refusal.label());
            println!(
                "write_plan_refusal_reason\t{}",
                safe_text!(refusal.reason())
            );
            println!(
                "write_plan_refusal_next_action\t{}",
                safe_text!(refusal.next_action())
            );
            match refusal {
                RefactorWriteRefusal::UnparsableOutputs { count } => {
                    println!("write_plan_unparsable_output_count\t{count}");
                }
            }
        }
        None => println!("write_plan_refusal\tnone"),
    }
}

pub(super) fn refactor_write_plan_json(
    write_plan: &RefactorWritePlan,
    writable_files: &[String],
    refused_files: &[String],
) -> Value {
    json!({
        "write_requested": write_plan.write_requested(),
        "write_allowed": write_plan.write_allowed(),
        "writable_file_count": write_plan.writable_indexes().len(),
        "writable_files": writable_files,
        "refused_file_count": refused_files.len(),
        "refused_files": refused_files,
        "refusal": write_plan.refusal().map(refactor_write_refusal_json),
    })
}

fn refactor_write_refusal_json(refusal: &RefactorWriteRefusal) -> Value {
    let mut value = json!({
        "status": refusal.label(),
        "reason": refusal.reason(),
        "next_action": refusal.next_action(),
    });
    match refusal {
        RefactorWriteRefusal::UnparsableOutputs { count } => {
            value["unparsable_output_count"] = json!(count);
        }
    }
    value
}
