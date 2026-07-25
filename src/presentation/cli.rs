macro_rules! safe_text {
    ($value:expr) => {
        crate::presentation::cli::terminal_safe(&$value)
    };
}

mod accessor_arity_report;
mod analysis_report;
mod args;
mod basic_edit;
mod binds_constant_report;
mod call_cycle_report;
mod call_graph_report;
mod call_report;
mod capabilities;
mod case_nil_key_report;
mod char_op_string_report;
mod class_cycle_report;
mod command;
mod complexity_report;
mod conditional_conversion;
mod cons_to_list_report;
mod constant_if_test_report;
mod contract;
mod convert_cond_to_if;
mod convert_flet_to_labels;
mod convert_if_to_cond;
mod convert_if_to_unless;
mod convert_if_to_when;
mod convert_labels_to_flet;
mod convert_let_star_to_let;
mod convert_let_to_let_star;
mod convert_sequential_binding;
mod convert_unless_to_if;
mod convert_when_to_if;
mod de_morgan_report;
mod dead_boolean_operand_report;
mod definition_movement;
mod definition_removal;
mod definition_report;
mod dependency_report;
mod destructive_literal_report;
mod dispatch;
mod duplicate_boolean_operand_report;
mod duplicate_case_key_report;
mod duplicate_cond_test_report;
mod duplicate_export_report;
mod duplicate_lambda_list_keyword_report;
mod duplicate_let_binding_report;
mod duplicate_method_report;
mod duplicate_parameter_report;
mod duplicate_report;
mod duplicate_setf_place_report;
mod duplicate_slot_report;
mod eliminate_empty_binding_form;
mod empty_body_report;
mod empty_let_report;
mod eq_char_comparison_report;
mod eq_number_comparison_report;
mod eql_list_comparison_report;
mod eql_search_literal_report;
mod eql_string_comparison_report;
mod equality_arity_report;
mod eval_when_situation_report;
mod exhaustive_case_otherwise_report;
mod explicit_nil_return_report;
mod explicit_step_delta_report;
mod extract_constant;
mod extract_function;
mod extract_local_function;
mod flatten_progn;
mod form_report;
mod format_missing_destination_report;
mod funcall_lambda_report;
mod function_parameter;
mod gate;
mod identical_if_branch_report;
mod identity_arithmetic_report;
mod if_arity_report;
mod if_to_or_report;
mod impact_report;
mod inline_function;
mod inline_lambda;
mod inline_let;
mod inline_literal_constant;
mod inline_local_function;
mod inline_symbol_macro;
mod introduce_let;
mod lambda_list_keyword_order_report;
mod let_report;
mod lint_report;
mod literal_place_report;
mod malformed_case_clause_report;
mod malformed_cond_clause_report;
mod malformed_iteration_spec_report;
mod malformed_let_binding_report;
mod manual_incf_report;
mod manual_push_report;
mod manual_pushnew_report;
mod merge_nested_flet;
mod merge_nested_let;
mod merge_nested_let_star;
mod modify_macro_arity_report;
mod naming_report;
mod negated_comparison_report;
mod negated_if_report;
mod negated_step_delta_report;
mod negated_when_unless_report;
mod nested_boolean_report;
mod nested_cxr_report;
mod nested_progn_report;
mod nested_unless_report;
mod nested_when_report;
mod nil_comparison_report;
mod nth_constant_index_report;
mod one_armed_if_report;
mod one_step_arithmetic_report;
mod package;
mod package_boundary_report;
mod package_conflict_report;
mod package_cycle_report;
mod quoted_case_key_report;
mod reachability_report;
mod redefinition_report;
mod redundant_apply_report;
mod redundant_body_progn_report;
mod redundant_boolean_identity_report;
mod redundant_eql_test_report;
mod redundant_funcall_report;
mod redundant_identity_key_report;
mod redundant_identity_report;
mod redundant_if_nil_report;
mod redundant_let_star_report;
mod redundant_progn_report;
mod redundant_quote_report;
mod refactor;
mod remove_unused_binding;
mod remove_unused_control;
mod rename;
mod rename_control;
mod replace_forms;
mod self_assignment_report;
mod self_comparison_report;
mod setf_arity_report;
mod setq_non_variable_report;
mod shadowed_binding_report;
mod shared;
mod sharp_quoted_lambda_report;
mod sign_comparison_report;
mod signature_report;
mod similarity_report;
mod single_arg_comparison_report;
mod single_clause_cond_report;
mod single_operand_arithmetic_report;
mod single_operand_boolean_report;
mod single_value_bind_report;
mod split_let;
mod split_let_star;
mod struct_cycle_report;
mod symbol_report;
mod system_conflict_report;
mod system_cycle_report;
mod t_comparison_report;
mod the_arity_report;
mod thread_expression;
mod undefined_package_report;
mod unreachable_case_clause_report;
mod unreachable_cond_clause_report;
mod unthread_expression;
mod unused_export_report;
mod unused_local_callable_report;
mod unused_nickname_report;
mod unused_package_report;
mod unused_parameter_report;
mod unwrap_call;
mod verbose_negation_report;
mod workspace_report;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path as FsPath, PathBuf};
use std::process::ExitCode;

use crate::application::refactor::execute::{
    RefactorExecuteGateInputs, RefactorExecuteMode, RefactorExecuteOutputParseResult,
    RefactorExecutePolicyResult, RefactorExecutePreVerificationResult,
    RefactorExecutePreflightInputs, RefactorWriteRefusal, build_refactor_execute_decision,
    build_refactor_execute_preflight_decision,
};
use crate::application::refactor::plan::{
    RefactorOperation as ApplicationRefactorOperation, RefactorPlanGate, RefactorPlanPolicy,
    RefactorPlanPolicyOptions as DomainRefactorPlanPolicyOptions, RefactorPlanRequest,
    RefactorPlanStep, RefactorPlanSummary, RefactorVerificationCheck, RefactorVerificationRequest,
    VerificationPhase as ApplicationVerificationPhase, build_refactor_plan_decision,
    refactor_plan_gates as application_refactor_plan_gates,
    refactor_verification_checks as application_refactor_verification_checks,
};
use crate::application::refactor::preview::{
    RefactorPreviewEdit, RefactorPreviewPolicy,
    RefactorPreviewPolicyOptions as DomainRefactorPreviewPolicyOptions, RefactorPreviewSummary,
    evaluate_refactor_preview_policy, refactor_preview_edits,
};
use crate::application::usecase::impact_report::{
    ImpactReportFile, ImpactRiskLevel as ApplicationImpactRiskLevel, raw_refactor_risks,
    summarize_impact_reports,
};
use crate::domain::definition::DefinitionCategory;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteOffset, ByteSpan, Path, SymbolName, SyntaxTree};
use crate::infrastructure::workspace::{WorkspaceDiscoveryOptions, discover_workspace_files};
use anyhow::{Context, Result};
use clap::{Args, Parser, ValueEnum};
use serde_json::{Value, json};

use args::*;
use command::Command;
pub(crate) use shared::{
    MAX_SOURCE_INPUT_BYTES, apply_byte_span_edits, bounded_preview, matching_symbol_occurrences,
    read_input_and_dialect, read_input_dialect_and_tree, read_text_file_with_limit,
    read_text_with_limit, require_output_file, resolve_target, stable_text_hash, terminal_safe,
    terminal_safe_error_chain, unified_diff, write_artifact_with_rollback,
    write_file_with_rollback, write_files_with_rollback,
};

#[derive(Debug, Parser)]
#[command(
    name = "paredit",
    version,
    about,
    long_about = None,
    after_help = "Canonical namespaces:\n  `paredit inspect ...` reads and reports without writing.\n  `paredit edit ...` transforms one selected form; stdout by default, --write to update the file.\n  `paredit refactor ...` plans, previews, verifies, and applies semantic changes.\n\nAll source-facing commands live in these three namespaces.\n`paredit completions <shell>` prints a shell completion script.\nRun `paredit inspect capabilities --output json` for a machine-readable catalog of every command and flag."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

pub fn run() -> ExitCode {
    let cli = Cli::parse();
    match dispatch::dispatch(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {}", terminal_safe_error_chain(&error));
            if error.downcast_ref::<gate::GateFailure>().is_some() {
                ExitCode::from(gate::GATE_FAILURE_EXIT_CODE as u8)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::terminal_safe_error_chain;

    #[test]
    fn cli_error_diagnostic_escapes_untrusted_controls() {
        let error = anyhow::anyhow!("bad\npath\t\u{1b}[31m\u{202e}").context("open failed");

        assert_eq!(
            format!("Error: {}", terminal_safe_error_chain(&error)),
            "Error: open failed: bad\\u{a}path\\u{9}\\u{1b}[31m\\u{202e}"
        );
    }
}
