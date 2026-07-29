use assert_cmd::Command;
use predicates::prelude::*;
use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, FileFailurePersistence};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "cli/accessor_arity_report.rs"]
mod accessor_arity_report;
#[path = "cli/action_contract.rs"]
mod action_contract;
#[path = "cli/agent_report_budget.rs"]
mod agent_report_budget;
#[path = "cli/analysis_report.rs"]
mod analysis_report;
#[path = "cli/append_list_to_cons_report.rs"]
mod append_list_to_cons_report;
#[path = "cli/append_nil_report.rs"]
mod append_nil_report;
#[path = "cli/architecture_contract.rs"]
mod architecture_contract;
#[path = "cli/basic_edit_write.rs"]
mod basic_edit_write;
#[path = "cli/binds_constant_report.rs"]
mod binds_constant_report;
#[path = "cli/butlast_default_count_report.rs"]
mod butlast_default_count_report;
#[path = "cli/call_cycle_report.rs"]
mod call_cycle_report;
#[path = "cli/call_graph_report/mod.rs"]
mod call_graph_report;
#[path = "cli/call_report.rs"]
mod call_report;
#[path = "cli/capabilities_contract.rs"]
mod capabilities_contract;
#[path = "cli/car_nthcdr_report.rs"]
mod car_nthcdr_report;
#[path = "cli/car_reverse_report.rs"]
mod car_reverse_report;
#[path = "cli/case_nil_key_report.rs"]
mod case_nil_key_report;
#[path = "cli/char_case_fold_report.rs"]
mod char_case_fold_report;
#[path = "cli/char_op_string_report.rs"]
mod char_op_string_report;
#[path = "cli/class_cycle_report.rs"]
mod class_cycle_report;
#[path = "cli/code_char_char_code_report.rs"]
mod code_char_char_code_report;
#[path = "cli/code_metrics_report.rs"]
mod code_metrics_report;
#[path = "cli/coerce_to_t_report.rs"]
mod coerce_to_t_report;
#[path = "cli/compatibility_contract.rs"]
mod compatibility_contract;
#[path = "cli/completions_contract.rs"]
mod completions_contract;
#[path = "cli/complexity_report.rs"]
mod complexity_report;
#[path = "cli/cond_t_clause_report.rs"]
mod cond_t_clause_report;
#[path = "cli/conditional_conversion.rs"]
mod conditional_conversion;
#[path = "cli/config_command.rs"]
mod config_command;
#[path = "cli/cons_to_list_report.rs"]
mod cons_to_list_report;
#[path = "cli/constant_if_test_report.rs"]
mod constant_if_test_report;
#[path = "cli/constant_when_test_report.rs"]
mod constant_when_test_report;
#[path = "cli/convert_cond_to_if.rs"]
mod convert_cond_to_if;
#[path = "cli/convert_flet_to_labels.rs"]
mod convert_flet_to_labels;
#[path = "cli/convert_if_to_cond.rs"]
mod convert_if_to_cond;
#[path = "cli/convert_labels_to_flet.rs"]
mod convert_labels_to_flet;
#[path = "cli/convert_let_star_to_let.rs"]
mod convert_let_star_to_let;
#[path = "cli/convert_let_to_let_star.rs"]
mod convert_let_to_let_star;
#[path = "cli/convert_sequential_binding.rs"]
mod convert_sequential_binding;
#[path = "cli/crate_metadata_contract.rs"]
mod crate_metadata_contract;
#[path = "cli/de_morgan_report.rs"]
mod de_morgan_report;
#[path = "cli/dead_boolean_operand_report.rs"]
mod dead_boolean_operand_report;
#[path = "cli/definition_movement.rs"]
mod definition_movement;
#[path = "cli/definition_removal.rs"]
mod definition_removal;
#[path = "cli/definition_report.rs"]
mod definition_report;
#[path = "cli/defpackage_quoted_report.rs"]
mod defpackage_quoted_report;
#[path = "cli/dependency_report.rs"]
mod dependency_report;
#[path = "cli/destructive_literal_report.rs"]
mod destructive_literal_report;
#[path = "cli/dialect_contract.rs"]
mod dialect_contract;
#[path = "cli/double_reverse_report.rs"]
mod double_reverse_report;
#[path = "cli/duplicate_boolean_operand_report.rs"]
mod duplicate_boolean_operand_report;
#[path = "cli/duplicate_case_key_report.rs"]
mod duplicate_case_key_report;
#[path = "cli/duplicate_cond_test_report.rs"]
mod duplicate_cond_test_report;
#[path = "cli/duplicate_export_report.rs"]
mod duplicate_export_report;
#[path = "cli/duplicate_keyword_report.rs"]
mod duplicate_keyword_report;
#[path = "cli/duplicate_lambda_list_keyword_report.rs"]
mod duplicate_lambda_list_keyword_report;
#[path = "cli/duplicate_let_binding_report.rs"]
mod duplicate_let_binding_report;
#[path = "cli/duplicate_method_report.rs"]
mod duplicate_method_report;
#[path = "cli/duplicate_parameter_report.rs"]
mod duplicate_parameter_report;
#[path = "cli/duplicate_report.rs"]
mod duplicate_report;
#[path = "cli/duplicate_setf_place_report.rs"]
mod duplicate_setf_place_report;
#[path = "cli/duplicate_slot_report.rs"]
mod duplicate_slot_report;
#[path = "cli/edit_transpose.rs"]
mod edit_transpose;
#[path = "cli/eliminate_empty_binding_form.rs"]
mod eliminate_empty_binding_form;
#[path = "cli/emacs_lisp_file_report.rs"]
mod emacs_lisp_file_report;
#[path = "cli/empty_body_report.rs"]
mod empty_body_report;
#[path = "cli/empty_let_report.rs"]
mod empty_let_report;
#[path = "cli/eq_char_comparison_report.rs"]
mod eq_char_comparison_report;
#[path = "cli/eq_number_comparison_report.rs"]
mod eq_number_comparison_report;
#[path = "cli/eql_list_comparison_report.rs"]
mod eql_list_comparison_report;
#[path = "cli/eql_search_literal_report.rs"]
mod eql_search_literal_report;
#[path = "cli/eql_string_comparison_report.rs"]
mod eql_string_comparison_report;
#[path = "cli/equality_arity_report.rs"]
mod equality_arity_report;
#[path = "cli/error_diagnosis.rs"]
mod error_diagnosis;
#[path = "cli/eval_when_situation_report.rs"]
mod eval_when_situation_report;
#[path = "cli/exhaustive_case_otherwise_report.rs"]
mod exhaustive_case_otherwise_report;
#[path = "cli/explicit_nil_return_report.rs"]
mod explicit_nil_return_report;
#[path = "cli/explicit_step_delta_report.rs"]
mod explicit_step_delta_report;
#[path = "cli/extract_constant/mod.rs"]
mod extract_constant;
#[path = "cli/extract_function/mod.rs"]
mod extract_function;
#[path = "cli/extract_local_function/mod.rs"]
mod extract_local_function;
#[path = "cli/flatten_progn.rs"]
mod flatten_progn;
#[path = "cli/form_report.rs"]
mod form_report;
#[path = "cli/format/mod.rs"]
mod format;
#[path = "cli/format_missing_destination_report.rs"]
mod format_missing_destination_report;
#[path = "cli/format_newline_report.rs"]
mod format_newline_report;
#[path = "cli/format_to_string_report.rs"]
mod format_to_string_report;
#[path = "cli/funcall_lambda_report.rs"]
mod funcall_lambda_report;
#[path = "cli/function_parameter/mod.rs"]
mod function_parameter;
#[path = "cli/getf_default_nil_report.rs"]
mod getf_default_nil_report;
#[path = "cli/gethash_default_report.rs"]
mod gethash_default_report;
#[path = "cli/handler_case_no_clauses_report.rs"]
mod handler_case_no_clauses_report;
#[path = "cli/help_contract.rs"]
mod help_contract;
#[path = "cli/identical_if_branch_report.rs"]
mod identical_if_branch_report;
#[path = "cli/identity_arithmetic_report.rs"]
mod identity_arithmetic_report;
#[path = "cli/if_arity_report.rs"]
mod if_arity_report;
#[path = "cli/if_not_report.rs"]
mod if_not_report;
#[path = "cli/if_to_or_report.rs"]
mod if_to_or_report;
#[path = "cli/if_to_unless_report.rs"]
mod if_to_unless_report;
#[path = "cli/impact_report.rs"]
mod impact_report;
#[path = "cli/inline_function/mod.rs"]
mod inline_function;
#[path = "cli/inline_lambda.rs"]
mod inline_lambda;
#[path = "cli/inline_literal_constant.rs"]
mod inline_literal_constant;
#[path = "cli/inline_local_function.rs"]
mod inline_local_function;
#[path = "cli/inline_symbol_macro.rs"]
mod inline_symbol_macro;
#[path = "cli/lambda_list_keyword_order_report.rs"]
mod lambda_list_keyword_order_report;
#[path = "cli/last_default_count_report.rs"]
mod last_default_count_report;
#[path = "cli/let_refactor/mod.rs"]
mod let_refactor;
#[path = "cli/lint_report.rs"]
mod lint_report;
#[path = "cli/lint_report_golden.rs"]
mod lint_report_golden;
#[path = "cli/lisp_analysis_report.rs"]
mod lisp_analysis_report;
#[path = "cli/list_star_nil_report.rs"]
mod list_star_nil_report;
#[path = "cli/list_star_to_cons_report.rs"]
mod list_star_to_cons_report;
#[path = "cli/literal_place_report.rs"]
mod literal_place_report;
#[path = "cli/make_array_default_keyword_report.rs"]
mod make_array_default_keyword_report;
#[path = "cli/make_hash_table_test_report.rs"]
mod make_hash_table_test_report;
#[path = "cli/make_list_default_element_report.rs"]
mod make_list_default_element_report;
#[path = "cli/malformed_case_clause_report.rs"]
mod malformed_case_clause_report;
#[path = "cli/malformed_cond_clause_report.rs"]
mod malformed_cond_clause_report;
#[path = "cli/malformed_iteration_spec_report.rs"]
mod malformed_iteration_spec_report;
#[path = "cli/malformed_let_binding_report.rs"]
mod malformed_let_binding_report;
#[path = "cli/manual_incf_report.rs"]
mod manual_incf_report;
#[path = "cli/manual_push_report.rs"]
mod manual_push_report;
#[path = "cli/manual_pushnew_report.rs"]
mod manual_pushnew_report;
#[path = "cli/merge_nested_flet.rs"]
mod merge_nested_flet;
#[path = "cli/merge_nested_let_star.rs"]
mod merge_nested_let_star;
#[path = "cli/merge_split_let.rs"]
mod merge_split_let;
#[path = "cli/modify_macro_arity_report.rs"]
mod modify_macro_arity_report;
#[path = "cli/multiple_value_list_of_values_report.rs"]
mod multiple_value_list_of_values_report;
#[path = "cli/naming_report.rs"]
mod naming_report;
#[path = "cli/negated_comparison_report.rs"]
mod negated_comparison_report;
#[path = "cli/negated_if_report.rs"]
mod negated_if_report;
#[path = "cli/negated_step_delta_report.rs"]
mod negated_step_delta_report;
#[path = "cli/negated_when_unless_report.rs"]
mod negated_when_unless_report;
#[path = "cli/nested_boolean_report.rs"]
mod nested_boolean_report;
#[path = "cli/nested_char_case_report.rs"]
mod nested_char_case_report;
#[path = "cli/nested_cxr_report.rs"]
mod nested_cxr_report;
#[path = "cli/nested_progn_report.rs"]
mod nested_progn_report;
#[path = "cli/nested_string_case_report.rs"]
mod nested_string_case_report;
#[path = "cli/nested_unless_report.rs"]
mod nested_unless_report;
#[path = "cli/nested_when_report.rs"]
mod nested_when_report;
#[path = "cli/nil_comparison_report.rs"]
mod nil_comparison_report;
#[path = "cli/nth_constant_index_report.rs"]
mod nth_constant_index_report;
#[path = "cli/nthcdr_small_index_report.rs"]
mod nthcdr_small_index_report;
#[path = "cli/nthcdr_zero_report.rs"]
mod nthcdr_zero_report;
#[path = "cli/one_armed_if_report.rs"]
mod one_armed_if_report;
#[path = "cli/one_step_arithmetic_report.rs"]
mod one_step_arithmetic_report;
#[path = "cli/package/mod.rs"]
mod package;
#[path = "cli/package_boundary_report.rs"]
mod package_boundary_report;
#[path = "cli/package_conflict_report.rs"]
mod package_conflict_report;
#[path = "cli/package_cycle_report.rs"]
mod package_cycle_report;
#[path = "cli/parse_integer_default_radix_report.rs"]
mod parse_integer_default_radix_report;
#[path = "cli/plan_steps_contract.rs"]
mod plan_steps_contract;
#[path = "cli/prog2_to_progn_report.rs"]
mod prog2_to_progn_report;
#[path = "cli/project_inventory_report.rs"]
mod project_inventory_report;
#[path = "cli/public_api_docs_contract.rs"]
mod public_api_docs_contract;
#[path = "cli/public_module_docs_contract.rs"]
mod public_module_docs_contract;
#[path = "cli/quoted_case_key_report.rs"]
mod quoted_case_key_report;
#[path = "cli/reachability_report.rs"]
mod reachability_report;
#[path = "cli/readme_api_docs_contract.rs"]
mod readme_api_docs_contract;
#[path = "cli/readme_ci_contract.rs"]
mod readme_ci_contract;
#[path = "cli/readme_contract.rs"]
mod readme_contract;
#[path = "cli/readme_docs_contract.rs"]
mod readme_docs_contract;
#[path = "cli/readme_install_contract.rs"]
mod readme_install_contract;
#[path = "cli/readme_smoke.rs"]
mod readme_smoke;
#[path = "cli/readme_workspace_smoke.rs"]
mod readme_workspace_smoke;
#[path = "cli/redefinition_report.rs"]
mod redefinition_report;
#[path = "cli/redundant_apply_report.rs"]
mod redundant_apply_report;
#[path = "cli/redundant_body_progn_report.rs"]
mod redundant_body_progn_report;
#[path = "cli/redundant_boolean_identity_report.rs"]
mod redundant_boolean_identity_report;
#[path = "cli/redundant_count_nil_report.rs"]
mod redundant_count_nil_report;
#[path = "cli/redundant_divisor_report.rs"]
mod redundant_divisor_report;
#[path = "cli/redundant_end_nil_report.rs"]
mod redundant_end_nil_report;
#[path = "cli/redundant_eql_test_report.rs"]
mod redundant_eql_test_report;
#[path = "cli/redundant_from_end_nil_report.rs"]
mod redundant_from_end_nil_report;
#[path = "cli/redundant_funcall_report.rs"]
mod redundant_funcall_report;
#[path = "cli/redundant_identity_key_report.rs"]
mod redundant_identity_key_report;
#[path = "cli/redundant_identity_report.rs"]
mod redundant_identity_report;
#[path = "cli/redundant_if_nil_report.rs"]
mod redundant_if_nil_report;
#[path = "cli/redundant_let_star_report.rs"]
mod redundant_let_star_report;
#[path = "cli/redundant_prog1_report.rs"]
mod redundant_prog1_report;
#[path = "cli/redundant_progn_report.rs"]
mod redundant_progn_report;
#[path = "cli/redundant_quote_report.rs"]
mod redundant_quote_report;
#[path = "cli/redundant_start_zero_report.rs"]
mod redundant_start_zero_report;
#[path = "cli/redundant_the_report.rs"]
mod redundant_the_report;
#[path = "cli/refactor_entrypoint_contract.rs"]
mod refactor_entrypoint_contract;
#[path = "cli/refactor_manifest/mod.rs"]
mod refactor_manifest;
#[path = "cli/refactor_preview.rs"]
mod refactor_preview;
#[path = "cli/refactor_workspace/mod.rs"]
mod refactor_workspace;
#[path = "cli/remove_unused_binding/mod.rs"]
mod remove_unused_binding;
#[path = "cli/remove_unused_control.rs"]
mod remove_unused_control;
#[path = "cli/rename/mod.rs"]
mod rename;
#[path = "cli/rename_at/mod.rs"]
mod rename_at;
#[path = "cli/rename_control.rs"]
mod rename_control;
#[path = "cli/repair_unclosed_lists.rs"]
mod repair_unclosed_lists;
#[path = "cli/replace_forms.rs"]
mod replace_forms;
#[path = "cli/self_assignment_report.rs"]
mod self_assignment_report;
#[path = "cli/self_comparison_report.rs"]
mod self_comparison_report;
#[path = "cli/semantic_report.rs"]
mod semantic_report;
#[path = "cli/setf_arity_report.rs"]
mod setf_arity_report;
#[path = "cli/setq_non_variable_report.rs"]
mod setq_non_variable_report;
#[path = "cli/shadowed_binding_report.rs"]
mod shadowed_binding_report;
#[path = "cli/sharp_quoted_lambda_report.rs"]
mod sharp_quoted_lambda_report;
#[path = "cli/sign_comparison_report.rs"]
mod sign_comparison_report;
#[path = "cli/signature_report.rs"]
mod signature_report;
#[path = "cli/similarity_report.rs"]
mod similarity_report;
#[path = "cli/single_arg_comparison_report.rs"]
mod single_arg_comparison_report;
#[path = "cli/single_clause_cond_report.rs"]
mod single_clause_cond_report;
#[path = "cli/single_operand_arithmetic_report.rs"]
mod single_operand_arithmetic_report;
#[path = "cli/single_operand_boolean_report.rs"]
mod single_operand_boolean_report;
#[path = "cli/single_operand_list_op_report.rs"]
mod single_operand_list_op_report;
#[path = "cli/single_value_bind_report.rs"]
mod single_value_bind_report;
#[path = "cli/skill_contract.rs"]
mod skill_contract;
#[path = "cli/sort_definitions.rs"]
mod sort_definitions;
#[path = "cli/split_file.rs"]
mod split_file;
#[path = "cli/split_let_star.rs"]
mod split_let_star;
#[path = "cli/step_zero_report.rs"]
mod step_zero_report;
#[path = "cli/string_case_fold_report.rs"]
mod string_case_fold_report;
#[path = "cli/struct_cycle_report.rs"]
mod struct_cycle_report;
#[path = "cli/subseq_zero_report.rs"]
mod subseq_zero_report;
#[path = "cli/symbol_report.rs"]
mod symbol_report;
#[path = "cli/system_conflict_report.rs"]
mod system_conflict_report;
#[path = "cli/system_cycle_report.rs"]
mod system_cycle_report;
#[path = "cli/t_comparison_report.rs"]
mod t_comparison_report;
#[path = "cli/terminal_output_safety.rs"]
mod terminal_output_safety;
#[path = "cli/the_arity_report.rs"]
mod the_arity_report;
#[path = "cli/thread_expression/mod.rs"]
mod thread_expression;
#[path = "cli/typecase_nil_key_report.rs"]
mod typecase_nil_key_report;
#[path = "cli/typep_predicate_report.rs"]
mod typep_predicate_report;
#[path = "cli/undefined_package_report.rs"]
mod undefined_package_report;
#[path = "cli/unreachable_case_clause_report.rs"]
mod unreachable_case_clause_report;
#[path = "cli/unreachable_cond_clause_report.rs"]
mod unreachable_cond_clause_report;
#[path = "cli/unused_export_report.rs"]
mod unused_export_report;
#[path = "cli/unused_local_callable_report.rs"]
mod unused_local_callable_report;
#[path = "cli/unused_nickname_report.rs"]
mod unused_nickname_report;
#[path = "cli/unused_package_report.rs"]
mod unused_package_report;
#[path = "cli/unused_parameter_report.rs"]
mod unused_parameter_report;
#[path = "cli/unwind_protect_no_cleanup_report.rs"]
mod unwind_protect_no_cleanup_report;
#[path = "cli/unwrap_call.rs"]
mod unwrap_call;
#[path = "cli/values_list_of_list_report.rs"]
mod values_list_of_list_report;
#[path = "cli/verbose_negation_report.rs"]
mod verbose_negation_report;
#[path = "cli/workspace_entrypoint_contract.rs"]
mod workspace_entrypoint_contract;
#[path = "cli/workspace_report.rs"]
mod workspace_report;
#[path = "cli/zero_divisor_report.rs"]
mod zero_divisor_report;

fn paredit() -> Command {
    Command::cargo_bin("paredit").expect("binary")
}

fn cli_proptest_config(cases: u32) -> ProptestConfig {
    let mut config = ProptestConfig::with_cases(cases);
    config.failure_persistence = Some(Box::new(FileFailurePersistence::Off));
    config
}

/// Bounds the case count like [`cli_proptest_config`] while keeping proptest's
/// default source-parallel failure persistence.
///
/// [`cli_proptest_config`] pins persistence to [`FileFailurePersistence::Off`],
/// which resolves to no path at all: a `*.proptest-regressions` file sitting
/// next to the test is then never loaded, so recorded shrinks stop being
/// replayed without any warning. Properties that have committed seeds must use
/// this variant instead, so those seeds keep running ahead of novel cases.
fn cli_proptest_config_replaying_recorded_failures(cases: u32) -> ProptestConfig {
    ProptestConfig::with_cases(cases)
}

fn stable_manifest_hash(text: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

fn fresh_temp_dir(name: &str) -> PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let unique = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    // Deliberately excludes the current thread's name: libtest names each
    // test thread after its fully-qualified test path, which can run past
    // the filesystem's per-component name limit for deeply nested modules
    // with long test names. `name` (a short caller-supplied label) plus the
    // pid/timestamp/counter already guarantee a unique, readable directory.
    let dir = std::env::temp_dir().join(format!(
        "paredit-cli-{name}-{}-{timestamp}-{unique}",
        std::process::id(),
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Map of "namespace subcommand" → set of long flags, generated from
/// `paredit inspect capabilities` so contract tests validate against the
/// real CLI surface.
fn capability_map() -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    let output = paredit()
        .args(["inspect", "capabilities"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value =
        serde_json::from_slice(&output).expect("capabilities emits valid JSON");

    let mut map = std::collections::BTreeMap::new();
    for namespace in report["commands"].as_array().expect("commands") {
        let namespace_name = namespace["name"].as_str().expect("namespace name");
        let Some(subcommands) = namespace["commands"].as_array() else {
            map.insert(namespace_name.to_owned(), std::collections::BTreeSet::new());
            continue;
        };
        for command in subcommands {
            let command_name = command["name"].as_str().expect("command name");
            let flags = command["args"]
                .as_array()
                .expect("command args")
                .iter()
                .filter_map(|arg| arg["long"].as_str().map(ToOwned::to_owned))
                .collect::<std::collections::BTreeSet<_>>();
            map.insert(format!("{namespace_name} {command_name}"), flags);
        }
    }
    map
}

/// Validates `paredit <ns> <cmd> --flags...` command strings against the
/// capability map, returning one problem line per unknown command or flag.
fn validate_paredit_command_strings(
    lines: &[String],
    capabilities: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for line in lines {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        let (Some(namespace), Some(subcommand)) = (tokens.get(1), tokens.get(2)) else {
            continue;
        };

        let key = if *namespace == "completions" {
            "completions".to_owned()
        } else {
            format!("{namespace} {subcommand}")
        };
        let Some(known_flags) = capabilities.get(&key) else {
            problems.push(format!("unknown command `{key}` in: {line}"));
            continue;
        };

        for token in &tokens[2..] {
            let Some(flag) = token.strip_prefix("--") else {
                continue;
            };
            let flag = flag.split('=').next().unwrap_or(flag);
            if flag == "help" {
                continue;
            }
            if !known_flags.contains(flag) {
                problems.push(format!("unknown flag `--{flag}` for `{key}` in: {line}"));
            }
        }
    }
    problems
}
