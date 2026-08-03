use super::fix::FixCommand;
use super::{MigrateCommand, SchemaCommand, query_count, query_find, query_replace};
use super::{
    accessor_arity_report, add_ignore_declaration, analysis_report, api_diff_report,
    api_surface_report, append_list_to_cons_report, append_nil_report,
    args::{
        AnalyzeArgs, CanonicalizeArgs, CopyArgs, CursorArgs, EditTargetArgs, FormatArgs, KillArgs,
        NavigateArgs, NewlineArgs, NormalizeQuotesArgs, RaiseArgs, ReindentArgs, RepairArgs,
        ReplaceArgs, TargetArgs, TransposeArgs, UnwrapPrefixArgs, WrapArgs, YankArgs,
    },
    around_method_missing_call_next_method_report, asdf_perform_without_call_next_method_report,
    asdf_self_referential_depends_on_report, asdf_system_missing_version_report,
    atom_swap_with_side_effect_report, binds_constant_report, blame_report,
    block_name_shadows_outer_block_report, butlast_default_count_report, call_cycle_report,
    call_graph_report, call_report, capabilities, car_nthcdr_report, car_reverse_report,
    case_key_eql_pitfall_report, case_nil_key_report, cerror_missing_continue_format_report,
    change_summary, char_case_fold_report, char_op_string_report, circular_literal_report,
    class_cycle_report, class_hierarchy_report, clone_report, code_char_char_code_report,
    coerce_to_t_report, cohesion_report, commented_repl_transcript_report, complexity_report,
    cond_t_clause_report, cond_to_case_candidate_report, config, cons_to_list_report,
    constant_if_test_report, constant_report, constant_when_test_report, context_report,
    convert_cond_to_if, convert_flet_to_labels, convert_if_to_cond, convert_if_to_unless,
    convert_if_to_when, convert_labels_to_flet, convert_let_star_to_let, convert_let_to_let_star,
    convert_sequential_binding, convert_unless_to_if, convert_when_to_if, data_check_report,
    de_morgan_report, dead_boolean_operand_report, debt_score_report,
    deeply_nested_anonymous_lambda_report, defclass_required_slot_no_initform_or_initarg_report,
    defclass_slot_shadowing_report, define_condition_empty_superclass_list_report,
    define_condition_missing_report_for_error_type_report, definition_movement, definition_removal,
    definition_report, defpackage_quoted_report, defpackage_without_in_package_report,
    dependency_report, destructive_literal_report, destructuring_bind_unused_whole_report,
    disabled_test_left_in_report, division_result_precision_loss_report, docstring_report,
    dolist_result_form_references_loop_variable_report,
    dotimes_bound_mutation_has_no_effect_report, dotimes_dolist_index_var_mutated_report,
    double_reverse_report, duplicate_boolean_operand_report, duplicate_case_key_report,
    duplicate_cond_test_report, duplicate_defmethod_signature_report, duplicate_keyword_report,
    duplicate_lambda_list_keyword_report, duplicate_let_binding_report, duplicate_parameter_report,
    duplicate_report, duplicate_setf_place_report, duplicate_test_name_report,
    duplication_ratio_report, dynamic_var_bound_across_thread_boundary_report, effect_report,
    eliminate_empty_binding_form, emacs_lisp_file_report, empty_body_report, empty_let_report,
    empty_test_body_report, epsilon_less_float_loop_bound_report, eq_char_comparison_report,
    eq_number_comparison_report, eql_list_comparison_report, eql_search_literal_report,
    eql_string_comparison_report, equality_arity_report, eval_when_situation_report,
    exhaustive_case_otherwise_report, explicit_nil_return_report, explicit_step_delta_report,
    external_diagnostics_report, external_system_report, extract_constant, extract_function,
    extract_local_function, flatten_progn, flet_single_use_inlinable_report, fold_constants,
    form_report, format_directive_report, format_missing_destination_report,
    format_nested_directive_unbalanced_report, format_newline_report,
    format_percent_ampersand_adjacent_redundancy_report, format_to_string_report,
    format_unknown_directive_report, ftype_values_arity_mismatch_report, funcall_lambda_report,
    function_parameter, future_promise_never_realized_report, generate_accessors,
    generate_defgeneric, generate_defpackage, generate_defsystem, generate_docstring,
    generate_tests, generic_dispatch_report, generic_function_no_methods_report,
    getf_default_nil_report, gethash_default_report, giant_conditional_form_report,
    go_to_undefined_tag_report, handler_bind_handler_returns_bare_value_report,
    handler_case_no_clauses_report, hash_table_iteration_order_assumed_report, hotspot_report,
    identical_if_branch_report, identity_arithmetic_report, if_arity_report, if_not_report,
    if_to_or_report, if_to_unless_report, ignore_errors_wraps_non_error_signal_report,
    impact_report, indentation_report, inline_function, inline_lambda, inline_let,
    inline_literal_constant, inline_local_function, inline_symbol_macro,
    intern_dynamic_package_target_report, introduce_let, introspection_probe_unchecked_report,
    keyword_arity_report, kill_ring_report, lambda_list_keyword_order_report,
    last_default_count_report, leftover_break_call_report, leftover_format_debug_marker_report,
    leftover_inspect_call_report, leftover_print_debug_report, leftover_step_call_report,
    leftover_time_benchmark_call_report, leftover_trace_call_report, let_report,
    license_header_report, license_report, line_metrics_report, lint_report, list_star_nil_report,
    list_star_to_cons_report, literal_place_report, lock_acquired_not_released_report,
    loop_clause_order_violation_report, loop_collect_into_immediately_returned_report,
    loop_for_across_statically_known_list_report, loop_into_accumulator_kind_conflict_report,
    loop_report, loop_unreachable_finally_clause_report, macro_expansion_report,
    macro_hygiene_report, magic_number_report, make_array_default_keyword_report,
    make_hash_table_test_report, make_list_default_element_report, malformed_case_clause_report,
    malformed_cond_clause_report, malformed_iteration_spec_report, malformed_let_binding_report,
    manual_incf_report, manual_push_report, manual_pushnew_report, merge_nested_flet,
    merge_nested_let, merge_nested_let_star, method_combination_report,
    method_qualifier_typo_report, mixed_float_precision_arithmetic_report,
    modify_macro_arity_report, multiple_value_bind_all_ignored_report,
    multiple_value_list_of_values_report, multiple_value_setq_arity_mismatch_report, naming_report,
    narrowing_report, negated_comparison_report, negated_if_report, negated_step_delta_report,
    negated_when_unless_report, nested_boolean_report, nested_char_case_report,
    nested_cond_flattenable_report, nested_cxr_report,
    nested_function_parameter_shadows_enclosing_parameter_report, nested_get_chain_report,
    nested_progn_report, nested_string_case_report, nested_unless_report, nested_when_report,
    nil_comparison_report, nth_constant_index_report, nthcdr_small_index_report,
    nthcdr_zero_report, one_armed_if_report, one_step_arithmetic_report,
    overly_long_parameter_list_report, package, package_boundary_report,
    package_circular_in_package_chain_report, package_conflict_report, package_cycle_report,
    package_level_shadowing_report, package_lock_report, parse_integer_default_radix_report,
    positional_argument_count_exceeds_readability_report,
    print_object_without_print_unreadable_object_report, prog2_to_progn_report,
    quoted_case_key_report, quoted_form_contains_stray_unquote_report, reachability_report,
    read_conditional_report, read_time_eval_report, readtable_case_report,
    recursive_lock_reentry_risk_report, redefinition_report, redundant_apply_report,
    redundant_body_progn_report, redundant_boolean_identity_report, redundant_count_nil_report,
    redundant_divisor_report, redundant_end_nil_report, redundant_eql_test_report,
    redundant_from_end_nil_report, redundant_funcall_report, redundant_identity_key_report,
    redundant_identity_report, redundant_if_nil_report, redundant_into_empty_collection_report,
    redundant_let_star_report, redundant_precision_coercion_report, redundant_prog1_report,
    redundant_progn_report, redundant_quote_report, redundant_start_zero_report,
    redundant_the_report, refactor, refactor_checkpoint, refactor_step, remove_unused_binding,
    remove_unused_control, rename, rename_control, replace_forms, resolve_report,
    restart_case_clause_without_report_report, restart_report, return_from_unmatched_block_report,
    return_outside_implicit_nil_block_report, self_assignment_report, self_comparison_report,
    self_recursive_tail_call_report, semantic_coverage_report, serial_consistency_report,
    set_membership_via_linear_scan_report, setf_arity_report, setq_non_variable_report,
    sharp_quoted_lambda_report, sign_comparison_report,
    signal_on_error_condition_returns_silently_report, signature_report, similarity_report,
    single_arg_comparison_report, single_clause_cond_report, single_operand_arithmetic_report,
    single_operand_boolean_report, single_operand_list_op_report, single_value_bind_report,
    sleep_in_test_report, slot_value_bypasses_accessor_report, source_report, split_let,
    split_let_star, step_zero_report, string_case_fold_report, stringly_typed_dispatch_report,
    struct_cycle_report, structural_diff, structural_patch, subseq_zero_report,
    symbol_function_fset_dynamic_name_report, symbol_index_report, system_conflict_report,
    system_cycle_report, t_comparison_report, tagbody_unreachable_tag_report,
    test_asserts_constant_report, test_map_report, test_without_assertion_report, the_arity_report,
    thread_expression, thread_spawned_without_error_handler_report, todo_report, type_report,
    typecase_nil_key_report, typep_predicate_report, undefined_package_report,
    unreachable_case_clause_report, unreachable_cond_clause_report, unreachable_expression_report,
    unsynchronized_shared_mutation_report, unthread_expression, unused_export_report,
    unused_local_callable_report, unused_nickname_report, unused_package_report,
    unwind_protect_no_cleanup_report, unwrap_call, use_widening_report, value_propagation_report,
    values_list_of_list_report, verbose_negation_report, when_unless_implicit_nil_misused_report,
    with_accessors_empty_binding_list_report, with_open_file_redundant_direction_default_report,
    workspace_report, writability_report, zero_divisor_report,
};
use clap::Subcommand;

/// Read-only inventory and analysis commands.
#[derive(Debug, Subcommand)]
#[command(
    after_help = "Examples:\n  paredit inspect check --file src/foo.lisp\n  paredit inspect outline --file src/foo.lisp --output json\n  paredit inspect symbols --symbol old-name --output json src/a.lisp src/b.lisp\n  paredit inspect workspace --output json .\n  paredit inspect capabilities --output json"
)]
pub(super) enum InspectCommand {
    /// Validate that input is a balanced S-expression document.
    Check(analysis_report::args::CheckArgs),
    /// Detect Lisp dialect from --file extension or explicit --dialect.
    Dialect(AnalyzeArgs),
    /// Print parse, dialect, and structural metrics for agent planning.
    Stats(AnalyzeArgs),
    /// Print a complete JSON report for AI coding agent refactor planning.
    AgentReport(analysis_report::args::AgentReportArgs),
    /// Print a machine-readable catalog of every command, flag, default, and enum value.
    Capabilities(capabilities::CapabilitiesArgs),
    /// Describe what changed between two versions of a file, as prose a pull request can use.
    Change(change_summary::ChangeSummaryArgs),
    /// Print top-level forms with paths, spans, and definition hints.
    Outline(AnalyzeArgs),
    /// Report one selected form with local structure for agent refactor planning.
    Form(form_report::args::FormReportArgs),
    /// Report which forms a selector names, with paths, coordinates and stable ids.
    Resolve(resolve_report::args::ResolveReportArgs),
    /// Find exact atom occurrences without touching strings or comments.
    FindSymbol(paredit_feature_project_inventory::SymbolQueryArgs),
    /// Report exact atom occurrences across explicit files for rename planning.
    Symbols(paredit_feature_project_inventory::SymbolReportArgs),
    /// Report list-head call sites across explicit files for arity refactor planning.
    Calls(call_report::args::CallReportArgs),
    /// Compare callable definitions and call-site arity across explicit files.
    Signature(signature_report::args::SignatureReportArgs),
    /// Report internal and optional external call graph edges across explicit files.
    CallGraph(call_graph_report::args::CallGraphArgs),
    /// Report refactoring impact risks for one symbol across explicit files.
    Impact(impact_report::args::ImpactReportArgs),
    /// Discover Lisp sources under roots and report parse/refactor inventory.
    Workspace(workspace_report::args::WorkspaceReportArgs),
    /// Report which files an analysis would select, and which rule dropped the rest.
    Sources(source_report::args::SourceReportArgs),
    /// Report package, system, load, and qualified-symbol dependencies across explicit files.
    Dependencies(dependency_report::args::DependencyReportArgs),
    /// Report Common Lisp package declarations across explicit files.
    Packages(package::types::PackageReportArgs),
    /// Report definition-like top-level forms across explicit files.
    Definitions(definition_report::args::DefinitionReportArgs),
    /// Report definition-like top-level forms with no external exact atom references.
    UnusedDefinitions(definition_report::args::UnusedDefinitionReportArgs),
    /// Report repeated structural S-expression shapes across explicit files.
    Duplicates(duplicate_report::args::DuplicateReportArgs),
    /// Group near-duplicate forms into clone classes, label each Type-1/2/3, and rank them by the lines extracting one would save.
    CloneClasses(clone_report::args::CloneClassReportArgs),
    /// Report duplicated runs of adjacent sibling forms, the sub-form clones no whole-form report can see.
    CloneSequences(clone_report::args::CloneSequenceReportArgs),
    /// Report project forms that duplicate a reference corpus, to find code a dependency already provides.
    CloneExternal(clone_report::args::CloneExternalReportArgs),
    /// Recommend a --threshold from the project's own similarity distribution instead of the built-in default.
    CloneThreshold(clone_report::args::CloneThresholdReportArgs),
    /// Order each clone class by the commit that introduced it, separating the original from the copies.
    CloneGenealogy(clone_report::args::CloneGenealogyReportArgs),
    /// Compare two documents by their parse: which forms were inserted,
    /// deleted, or replaced, ignoring whitespace and comments.
    Diff(structural_diff::args::StructuralDiffArgs),
    /// Report a setf/setq/psetf/psetq that assigns the same variable more than once.
    DuplicateSetfPlaces(duplicate_setf_place_report::args::DuplicateSetfPlaceReportArgs),
    /// Report defclass/define-condition/defstruct forms declaring the same slot name more than once.
    DuplicateSlots(paredit_feature_lisp_analysis::DuplicateSlotReportArgs),
    /// Report Emacs Lisp per-file facts: lexical-binding, provided and required features, autoload cookies.
    ElispFile(emacs_lisp_file_report::args::EmacsLispFileReportArgs),
    /// Report defmethod forms with the same name, qualifier, and specializers declared more than once.
    DuplicateMethods(paredit_feature_lisp_analysis::DuplicateMethodReportArgs),
    /// Report callable definitions whose lambda list names the same parameter more than once.
    DuplicateParameters(duplicate_parameter_report::args::DuplicateParameterReportArgs),
    /// Report lambda lists that repeat a lambda-list keyword (&optional, &rest, &key, ...).
    DuplicateLambdaListKeyword(
        duplicate_lambda_list_keyword_report::args::DuplicateLambdaListKeywordReportArgs,
    ),
    /// Report lambda lists whose keywords are out of the canonical &optional/&rest/&key/&aux order.
    LambdaListKeywordOrder(
        lambda_list_keyword_order_report::args::LambdaListKeywordOrderReportArgs,
    ),
    /// Report case/ecase/ccase forms with the same key in more than one clause.
    DuplicateCaseKeys(duplicate_case_key_report::args::DuplicateCaseKeyReportArgs),
    /// Report case/ecase/ccase clauses with a quoted key ('a matches quote and a, not a).
    QuotedCaseKey(quoted_case_key_report::args::QuotedCaseKeyReportArgs),
    /// Report case/ecase/ccase clauses with a bare nil key, which never matches (use ((nil) ...)).
    CaseNilKey(case_nil_key_report::args::CaseNilKeyReportArgs),
    /// Report typecase/etypecase/ctypecase clauses with a bare nil type, which never matches (use null).
    TypecaseNilKey(typecase_nil_key_report::args::TypecaseNilKeyReportArgs),
    /// Report case/typecase-family clauses that are not a non-empty list (a bare atom or empty clause).
    MalformedCaseClause(malformed_case_clause_report::args::MalformedCaseClauseReportArgs),
    /// Report case/typecase clauses after a t/otherwise catch-all clause that can never run.
    UnreachableCaseClause(unreachable_case_clause_report::args::UnreachableCaseClauseReportArgs),
    /// Report ecase/ccase/etypecase/ctypecase forms with a forbidden t/otherwise clause.
    ExhaustiveCaseOtherwise(
        exhaustive_case_otherwise_report::args::ExhaustiveCaseOtherwiseReportArgs,
    ),
    /// Report an incf/decf with an explicit delta of 1, the default ((incf x 1) is (incf x)).
    ExplicitStepDelta(explicit_step_delta_report::args::ExplicitStepDeltaReportArgs),
    /// Report an incf/decf with a negative literal delta, which flips the operator ((incf x -1) is (decf x)).
    NegatedStepDelta(negated_step_delta_report::args::NegatedStepDeltaReportArgs),
    /// Report a return/return-from with an explicit nil result, the default ((return nil) is (return)).
    ExplicitNilReturn(explicit_nil_return_report::args::ExplicitNilReturnReportArgs),
    /// Report cond forms with the same test expression in more than one clause.
    DuplicateCondTests(duplicate_cond_test_report::args::DuplicateCondTestReportArgs),
    /// Report cond forms with clauses after a t catch-all clause that can never run.
    UnreachableCondClause(unreachable_cond_clause_report::args::UnreachableCondClauseReportArgs),
    /// Report cond clauses that are not a non-empty list (a bare atom or empty clause).
    MalformedCondClause(malformed_cond_clause_report::args::MalformedCondClauseReportArgs),
    /// Report parallel let forms that bind the same variable more than once.
    DuplicateLetBindings(duplicate_let_binding_report::args::DuplicateLetBindingReportArgs),
    /// Report let/let* bindings that are neither a symbol nor a (var value) pair.
    MalformedLetBinding(malformed_let_binding_report::args::MalformedLetBindingReportArgs),
    /// Report a setf/setq that manually increments a variable ((setf x (1+ x)) is (incf x)).
    ManualIncf(manual_incf_report::args::ManualIncfReportArgs),
    /// Report a setf/setq that manually conses onto a variable ((setf x (cons e x)) is (push e x)).
    ManualPush(manual_push_report::args::ManualPushReportArgs),
    /// Report a setf/setq that manually adjoins onto a variable ((setf x (adjoin e x)) is (pushnew e x)).
    ManualPushnew(manual_pushnew_report::args::ManualPushnewReportArgs),
    /// Report let/let*/do/do* bindings whose variable is a constant (nil, t, or a keyword).
    BindsConstant(binds_constant_report::args::BindsConstantReportArgs),
    /// Report dolist/dotimes specs that are not a (var form [result]) list.
    MalformedIterationSpec(malformed_iteration_spec_report::args::MalformedIterationSpecReportArgs),
    /// Report and/or forms that list the same operand more than once.
    DuplicateBooleanOperands(
        duplicate_boolean_operand_report::args::DuplicateBooleanOperandReportArgs,
    ),
    /// Report and/or forms whose non-final constant operand makes later operands dead.
    DeadBooleanOperand(dead_boolean_operand_report::args::DeadBooleanOperandReportArgs),
    /// Report setq/setf/psetq/psetf pairs that assign a place to itself.
    SelfAssignments(self_assignment_report::args::SelfAssignmentReportArgs),
    /// Report setq/setf/psetq/psetf forms with an odd argument count (missing a value).
    SetfArity(setf_arity_report::args::SetfArityReportArgs),
    /// Report setq/psetq places that are not variables (a list, literal, or constant).
    SetqNonVariable(setq_non_variable_report::args::SetqNonVariableReportArgs),
    /// Report incf/decf/push/pop calls with the wrong number of arguments.
    ModifyMacroArity(modify_macro_arity_report::args::ModifyMacroArityReportArgs),
    /// Report comparison calls whose two operands are structurally identical.
    SelfComparison(self_comparison_report::args::SelfComparisonReportArgs),
    /// Report if forms whose then and else branches are structurally identical.
    IdenticalIfBranches(identical_if_branch_report::args::IdenticalIfBranchReportArgs),
    /// Report if forms with the wrong number of arguments (Common Lisp if takes 2 or 3).
    IfArity(if_arity_report::args::IfArityReportArgs),
    /// Report eq/eql/equal/equalp comparisons against t ((eq x t) only matches the symbol T).
    TComparison(t_comparison_report::args::TComparisonReportArgs),
    /// Report the special forms without exactly two arguments (a type and a form).
    TheArity(the_arity_report::args::TheArityReportArgs),
    /// Report eq/eql/equal/equalp calls without exactly two arguments.
    EqualityArity(equality_arity_report::args::EqualityArityReportArgs),
    /// Report nth/elt/gethash/getf/... accessors with the wrong number of arguments.
    AccessorArity(accessor_arity_report::args::AccessorArityReportArgs),
    /// Report eval-when forms with an invalid situation (not :compile-toplevel/:load-toplevel/:execute).
    EvalWhenSituation(eval_when_situation_report::args::EvalWhenSituationReportArgs),
    /// Report eq/eql calls that compare against a string literal (never reliably eql).
    EqlStringComparison(eql_string_comparison_report::args::EqlStringComparisonReportArgs),
    /// Report eq/eql calls that compare against a quoted list literal (never reliably eql).
    EqlListComparison(eql_list_comparison_report::args::EqlListComparisonReportArgs),
    /// Report eq calls that compare against a number literal (eq on numbers is unreliable).
    EqNumberComparison(eq_number_comparison_report::args::EqNumberComparisonReportArgs),
    /// Report eq calls that compare against a character literal (eq on characters is unreliable).
    EqCharComparison(eq_char_comparison_report::args::EqCharComparisonReportArgs),
    /// Report destructive sequence calls (nreverse/sort/...) on a quoted list literal (undefined behavior).
    DestructiveLiteral(destructive_literal_report::args::DestructiveLiteralReportArgs),
    /// Report member/assoc/find/... searching for a string/list literal without :test (default eql won't match).
    EqlSearchLiteral(eql_search_literal_report::args::EqlSearchLiteralReportArgs),
    /// Report character functions (char=/char-code/...) applied to a string literal (type error).
    CharOpString(char_op_string_report::args::CharOpStringReportArgs),
    /// Report a string= of two same-case-folded operands ((string= (string-downcase a) (string-downcase b)) is (string-equal a b)).
    StringCaseFold(string_case_fold_report::args::StringCaseFoldReportArgs),
    /// Report a char= of two same-case-folded operands ((char= (char-downcase a) (char-downcase b)) is (char-equal a b)).
    CharCaseFold(char_case_fold_report::args::CharCaseFoldReportArgs),
    /// Report nested string case ops, where the outer dominates ((string-upcase (string-downcase s)) is (string-upcase s)).
    NestedStringCase(nested_string_case_report::args::NestedStringCaseReportArgs),
    /// Report a (code-char (char-code c)), a round-trip that is just c.
    CodeCharCharCode(code_char_char_code_report::args::CodeCharCharCodeReportArgs),
    /// Report a (last x 1) whose explicit count restates the default of 1 ((last x 1) is (last x)).
    LastDefaultCount(last_default_count_report::args::LastDefaultCountReportArgs),
    /// Report a (butlast x 1)/(nbutlast x 1) whose explicit count restates the default of 1.
    ButlastDefaultCount(butlast_default_count_report::args::ButlastDefaultCountReportArgs),
    /// Report a (make-list n :initial-element nil) whose keyword restates the default of nil.
    MakeListDefaultElement(
        make_list_default_element_report::args::MakeListDefaultElementReportArgs,
    ),
    /// Report a (parse-integer s :radix 10) whose keyword restates the default of 10.
    ParseIntegerDefaultRadix(
        parse_integer_default_radix_report::args::ParseIntegerDefaultRadixReportArgs,
    ),
    /// Report a (getf p k nil) whose explicit default restates getf's default of nil ((getf p k nil) is (getf p k)).
    GetfDefaultNil(getf_default_nil_report::args::GetfDefaultNilReportArgs),
    /// Report a make-array with an explicit :adjustable nil / :fill-pointer nil, restating the default.
    MakeArrayDefaultKeyword(
        make_array_default_keyword_report::args::MakeArrayDefaultKeywordReportArgs,
    ),
    /// Report nested char case ops, where the outer dominates ((char-upcase (char-downcase c)) is (char-upcase c)).
    NestedCharCase(nested_char_case_report::args::NestedCharCaseReportArgs),
    /// Report a (list* a ... nil) with a nil tail, which is a spelled-out (list a ...).
    ListStarNil(list_star_nil_report::args::ListStarNilReportArgs),
    /// Report when/unless/dolist/dotimes forms that have no body (the test/spec runs, then nothing).
    EmptyBody(empty_body_report::args::EmptyBodyReportArgs),
    /// Report a let with an empty binding list, which is just progn ((let () body) is (progn body)).
    EmptyLet(empty_let_report::args::EmptyLetReportArgs),
    /// Report arithmetic forms with a redundant identity operand ((+ x 0), (* x 1), (- x 0), (/ x 1)).
    IdentityArithmetic(identity_arithmetic_report::args::IdentityArithmeticReportArgs),
    /// Report a quotient op with a redundant divisor of 1 ((floor x 1) is (floor x)).
    RedundantDivisor(redundant_divisor_report::args::RedundantDivisorReportArgs),
    /// Run every within-file logic-bug lint at once and report all findings.
    Lint(lint_report::args::LintReportArgs),
    /// Report structurally similar S-expression forms across explicit files.
    Similarity(similarity_report::args::SimilarityReportArgs),
    /// Report local let bindings and inline safety for agent refactor planning.
    Lets(let_report::LetReportArgs),
    /// Report per-definition nesting depth and size metrics for refactor prioritization.
    Complexity(complexity_report::args::ComplexityReportArgs),
    /// Report definition names that deviate from idiomatic kebab-case Lisp naming.
    Naming(naming_report::args::NamingReportArgs),
    /// Report callable definitions unreachable from any entry point in the internal call graph.
    Reachability(reachability_report::args::ReachabilityReportArgs),
    /// Report top-level definitions of the same category and name declared more than once.
    Redefinitions(redefinition_report::args::RedefinitionReportArgs),
    /// Report self-evaluating literals (numbers, strings, characters, keywords) that are quoted redundantly.
    RedundantQuote(redundant_quote_report::args::RedundantQuoteReportArgs),
    /// Report progn forms that are redundant (empty, or wrapping a single form).
    RedundantProgn(redundant_progn_report::args::RedundantPrognReportArgs),
    /// Report a prog1 wrapping a single form, which is just that form ((prog1 x) is x).
    RedundantProg1(redundant_prog1_report::args::RedundantProg1ReportArgs),
    /// Report a function's own name called in tail position of its body, annotated with the target dialect's tail-call guarantee.
    SelfRecursiveTailCall(self_recursive_tail_call_report::args::SelfRecursiveTailCallReportArgs),
    /// Report when/unless forms whose test is a (not X)/(null X) negation (flip the macro instead).
    NegatedWhenUnless(negated_when_unless_report::args::NegatedWhenUnlessReportArgs),
    /// Report negated two-arg numeric comparisons ((not (= a b)) is (/= a b)).
    NegatedComparison(negated_comparison_report::args::NegatedComparisonReportArgs),
    /// Report three-arg if with a negated test ((if (not c) a b) is (if c b a)).
    NegatedIf(negated_if_report::args::NegatedIfReportArgs),
    /// Report an if with a literal t/nil test ((if t a b) is a; (if nil a b) is b).
    ConstantIfTest(constant_if_test_report::args::ConstantIfTestReportArgs),
    /// Report a when/unless with a literal t/nil test ((when t b) is (progn b); (when nil b) is nil).
    ConstantWhenTest(constant_when_test_report::args::ConstantWhenTestReportArgs),
    /// Report a cons onto nil or a list literal ((cons a nil) is (list a); (cons a (list b)) is (list a b)).
    ConsToList(cons_to_list_report::args::ConsToListReportArgs),
    /// Report (reverse (reverse x)), a wasteful obfuscated copy ((reverse (reverse x)) is (copy-seq x)).
    DoubleReverse(double_reverse_report::args::DoubleReverseReportArgs),
    /// Report (append (list x) rest), a one-element append that is just a cons ((append (list x) r) is (cons x r)).
    AppendListToCons(append_list_to_cons_report::args::AppendListToConsReportArgs),
    /// Report a two-argument (list* a b), which is just a cons ((list* a b) is (cons a b)).
    ListStarToCons(list_star_to_cons_report::args::ListStarToConsReportArgs),
    /// Report (values-list (list a b)), which is just (values a b).
    ValuesListOfList(values_list_of_list_report::args::ValuesListOfListReportArgs),
    /// Report (multiple-value-list (values a b)), which is just (list a b).
    MultipleValueListOfValues(
        multiple_value_list_of_values_report::args::MultipleValueListOfValuesReportArgs,
    ),
    /// Report (append x nil), a fresh top-level copy ((append x nil) is (copy-list x)).
    AppendNil(append_nil_report::args::AppendNilReportArgs),
    /// Report negation written the long way ((- 0 x) and (* x -1) are (- x)).
    VerboseNegation(verbose_negation_report::args::VerboseNegationReportArgs),
    /// Report a same-operator and/or nested in an and/or, which flattens ((or a (or b c)) is (or a b c)).
    NestedBoolean(nested_boolean_report::args::NestedBooleanReportArgs),
    /// Report nested car/cdr accessors that combine into one ((car (cdr x)) is (cadr x)).
    NestedCxr(nested_cxr_report::args::NestedCxrReportArgs),
    /// Report an inner let binding or lambda-list parameter that reuses the name of a top-level definition in the same file.
    PackageLevelShadowing(package_level_shadowing_report::args::PackageLevelShadowingReportArgs),
    /// Report nth with a small constant index that has an ordinal accessor ((nth 0 x) is (first x)).
    NthConstantIndex(nth_constant_index_report::args::NthConstantIndexReportArgs),
    /// Report (nthcdr 0 list), which is just list (nthcdr with a zero count returns the list).
    NthcdrZero(nthcdr_zero_report::args::NthcdrZeroReportArgs),
    /// Report (subseq seq 0), a whole-sequence copy ((subseq seq 0) is (copy-seq seq)).
    SubseqZero(subseq_zero_report::args::SubseqZeroReportArgs),
    /// Report (car (nthcdr n x)), which is just (nth n x).
    CarNthcdr(car_nthcdr_report::args::CarNthcdrReportArgs),
    /// Report (car (reverse x)), a wasteful full copy to read the last element ((car (reverse x)) is (car (last x))).
    CarReverse(car_reverse_report::args::CarReverseReportArgs),
    /// Report (nthcdr 1..4 list) with a named cdr accessor ((nthcdr 2 x) is (cddr x)).
    NthcdrSmallIndex(nthcdr_small_index_report::args::NthcdrSmallIndexReportArgs),
    /// Report progn forms with two or more body forms nested directly inside another progn.
    NestedProgn(nested_progn_report::args::NestedPrognReportArgs),
    /// Report an unless whose only body is an unless, mergeable by or ((unless a (unless b c)) is (unless (or a b) c)).
    NestedUnless(nested_unless_report::args::NestedUnlessReportArgs),
    /// Report a when whose only body is a when, mergeable by and ((when a (when b c)) is (when (and a b) c)).
    NestedWhen(nested_when_report::args::NestedWhenReportArgs),
    /// Report eq/eql/equal/equalp comparisons against nil ((eq x nil) is just (null x)).
    NilComparison(nil_comparison_report::args::NilComparisonReportArgs),
    /// Report one-armed if forms with no else branch ((if test then) is (when test then)).
    OneArmedIf(one_armed_if_report::args::OneArmedIfReportArgs),
    /// Report an if whose test and then are the same atom ((if x x y) is (or x y)).
    IfToOr(if_to_or_report::args::IfToOrReportArgs),
    /// Report a three-argument if with then=nil and else=t ((if test nil t) is (not test)).
    IfNot(if_not_report::args::IfNotReportArgs),
    /// Report a three-argument if with then=nil ((if c nil e) is (unless c e)).
    IfToUnless(if_to_unless_report::args::IfToUnlessReportArgs),
    /// Report a two-form prog2, which is just progn ((prog2 a b) is (progn a b)).
    Prog2ToProgn(prog2_to_progn_report::args::Prog2ToPrognReportArgs),
    /// Report a handler-case with no handler clauses, which is just its body ((handler-case x) is x).
    HandlerCaseNoClauses(handler_case_no_clauses_report::args::HandlerCaseNoClausesReportArgs),
    /// Report an unwind-protect with no cleanup forms, which is just its body ((unwind-protect x) is x).
    UnwindProtectNoCleanup(
        unwind_protect_no_cleanup_report::args::UnwindProtectNoCleanupReportArgs,
    ),
    /// Report a +/- of a literal 1 with a shorthand ((+ x 1) is (1+ x); (- x 1) is (1- x)).
    OneStepArithmetic(one_step_arithmetic_report::args::OneStepArithmeticReportArgs),
    /// Report (apply #'f (list ...)) forms that are just (f ...) (a direct call).
    RedundantApply(redundant_apply_report::args::RedundantApplyReportArgs),
    /// Report an eql-defaulting call with an explicit :test #'eql ((find x l :test #'eql) is (find x l)).
    RedundantEqlTest(redundant_eql_test_report::args::RedundantEqlTestReportArgs),
    /// Report a bounded-sequence call with an explicit :start 0, the default ((find x seq :start 0) is (find x seq)).
    RedundantStartZero(redundant_start_zero_report::args::RedundantStartZeroReportArgs),
    /// Report a bounded-sequence call with an explicit :end nil, the default ((find x seq :end nil) is (find x seq)).
    RedundantEndNil(redundant_end_nil_report::args::RedundantEndNilReportArgs),
    /// Report a sequence call with an explicit :from-end nil, the default ((find x seq :from-end nil) is (find x seq)).
    RedundantFromEndNil(redundant_from_end_nil_report::args::RedundantFromEndNilReportArgs),
    /// Report a remove/delete/substitute call with an explicit :count nil, the default ((remove x seq :count nil) is (remove x seq)).
    RedundantCountNil(redundant_count_nil_report::args::RedundantCountNilReportArgs),
    /// Report a make-hash-table with an explicit :test 'eql, the default ((make-hash-table :test 'eql) is (make-hash-table)).
    MakeHashTableTest(make_hash_table_test_report::args::MakeHashTableTestReportArgs),
    /// Report a gethash with an explicit nil default, the default ((gethash k h nil) is (gethash k h)).
    GethashDefault(gethash_default_report::args::GethashDefaultReportArgs),
    /// Report a let/let*/cond/case-family form carrying more bindings or clauses than a threshold.
    GiantConditionalForm(giant_conditional_form_report::args::GiantConditionalFormReportArgs),
    /// Report a typep against a type with a dedicated predicate ((typep x 'string) is (stringp x)).
    TypepPredicate(typep_predicate_report::args::TypepPredicateReportArgs),
    /// Report a coerce to type t, which returns the object unchanged ((coerce x t) is x).
    CoerceToT(coerce_to_t_report::args::CoerceToTReportArgs),
    /// Report a :key-taking call with an explicit :key #'identity or :key nil ((sort xs #'< :key #'identity) is (sort xs #'<)).
    RedundantIdentityKey(redundant_identity_key_report::args::RedundantIdentityKeyReportArgs),
    /// Report multi-form progn forms used as a body of when/unless/let/defun/... (its forms splice in).
    RedundantBodyProgn(redundant_body_progn_report::args::RedundantBodyPrognReportArgs),
    /// Report an and/or with a redundant identity operand (t in and, nil in or).
    RedundantBooleanIdentity(
        redundant_boolean_identity_report::args::RedundantBooleanIdentityReportArgs,
    ),
    /// Report an and/or of all negations, collapsible by De Morgan ((and (not a) (not b)) is (not (or a b))).
    DeMorgan(de_morgan_report::args::DeMorganReportArgs),
    /// Report an (identity x) call, which is just x.
    RedundantIdentity(redundant_identity_report::args::RedundantIdentityReportArgs),
    /// Report three-argument if forms whose else branch is a redundant literal nil.
    RedundantIfNil(redundant_if_nil_report::args::RedundantIfNilReportArgs),
    /// Report a let* with zero or one binding, which is just let (no sequential scope in play).
    RedundantLetStar(redundant_let_star_report::args::RedundantLetStarReportArgs),
    /// Report (funcall #'foo ...) forms that are just (foo ...) (a direct call).
    RedundantFuncall(redundant_funcall_report::args::RedundantFuncallReportArgs),
    /// Report (the t form), a vacuous type declaration that is just form (t matches every object).
    RedundantThe(redundant_the_report::args::RedundantTheReportArgs),
    /// Report (funcall (lambda ...) ...) forms that apply the lambda directly (((lambda ...) ...)).
    FuncallLambda(funcall_lambda_report::args::FuncallLambdaReportArgs),
    /// Report #'(lambda ...) forms with a redundant #' prefix (#'(lambda ...) is (lambda ...)).
    SharpQuotedLambda(sharp_quoted_lambda_report::args::SharpQuotedLambdaReportArgs),
    /// Report single-operand and/or forms ((and X) and (or X) are just X).
    SingleOperandBoolean(single_operand_boolean_report::args::SingleOperandBooleanReportArgs),
    /// Report a single-argument append/nconc/list*, which returns its argument ((append x) is x).
    SingleOperandListOp(single_operand_list_op_report::args::SingleOperandListOpReportArgs),
    /// Report single-operand +/* forms ((+ X) and (* X) are just X).
    SingleOperandArithmetic(
        single_operand_arithmetic_report::args::SingleOperandArithmeticReportArgs,
    ),
    /// Report numeric comparisons (< > <= >= = /=) with a single argument (always true).
    SingleArgComparison(single_arg_comparison_report::args::SingleArgComparisonReportArgs),
    /// Report a cond with a single non-t clause that has a body ((cond (test body)) is (when test body)).
    SingleClauseCond(single_clause_cond_report::args::SingleClauseCondReportArgs),
    /// Report a cond with a single t clause that has a body ((cond (t body)) is (progn body)).
    CondTClause(cond_t_clause_report::args::CondTClauseReportArgs),
    /// Report a multiple-value-bind of one variable, which is just let ((multiple-value-bind (x) f b) is (let ((x f)) b)).
    SingleValueBind(single_value_bind_report::args::SingleValueBindReportArgs),
    /// Report =/</> comparisons against 0 that have a predicate ((= x 0) is (zerop x)).
    SignComparison(sign_comparison_report::args::SignComparisonReportArgs),
    /// Report format calls whose first argument is a string literal (the destination is missing).
    FormatMissingDestination(
        format_missing_destination_report::args::FormatMissingDestinationReportArgs,
    ),
    /// Report (format nil "~A"/"~S" x), which is (princ-to-string x)/(prin1-to-string x).
    FormatToString(format_to_string_report::args::FormatToStringReportArgs),
    /// Report (format t "~%"), which is just (terpri) (write a newline to standard output).
    FormatNewline(format_newline_report::args::FormatNewlineReportArgs),
    /// Report incf/decf/push/pop/pushnew whose place is a self-evaluating literal (cannot be modified).
    LiteralPlace(literal_place_report::args::LiteralPlaceReportArgs),
    /// Report a division-family form with a literal 0 divisor, a guaranteed division-by-zero ((/ x 0)).
    ZeroDivisor(zero_divisor_report::args::ZeroDivisorReportArgs),
    /// Report a call passing the same keyword argument twice ((make-instance 'c :x 1 :x 2)).
    DuplicateKeyword(duplicate_keyword_report::args::DuplicateKeywordReportArgs),
    /// Report a quoted designator in a defpackage clause, which defpackage does not evaluate ((:export 'foo)).
    DefpackageQuoted(defpackage_quoted_report::args::DefpackageQuotedReportArgs),
    /// Report an incf/decf with an explicit step of 0, a no-op ((incf x 0)).
    StepZero(step_zero_report::args::StepZeroReportArgs),
    /// Report declared function parameters with no unshadowed reference in their body.
    UnusedParameters(paredit_feature_function_parameter::UnusedParameterReportArgs),
    /// Report let-family bindings that shadow an enclosing parameter or let binding.
    ShadowedBindings(paredit_feature_binding::ShadowedBindingReportArgs),
    /// Report flet/labels local callables never called anywhere in their visible scope.
    UnusedLocalCallables(unused_local_callable_report::args::UnusedLocalCallableReportArgs),
    /// Report package::symbol references that reach into another package's internal symbols.
    PackageBoundaries(package_boundary_report::args::PackageBoundaryReportArgs),
    /// Report strongly connected cycles of two or more definitions in the internal call graph.
    CallCycles(call_cycle_report::args::CallCycleReportArgs),
    /// Report defpackage :use/:import-from cycles across two or more packages.
    PackageCycles(package_cycle_report::args::PackageCycleReportArgs),
    /// Report distinct defpackage forms that claim the same package name or nickname.
    PackageConflicts(package_conflict_report::args::PackageConflictReportArgs),
    /// Report distinct asdf:defsystem forms that claim the same system name.
    SystemConflicts(system_conflict_report::args::SystemConflictReportArgs),
    /// Report ASDF defsystem :depends-on cycles across two or more systems.
    SystemCycles(system_cycle_report::args::SystemCycleReportArgs),
    /// Report CLOS defclass/define-condition superclass inheritance cycles across two or more classes.
    ClassCycles(class_cycle_report::args::ClassCycleReportArgs),
    /// Report defstruct :include cycles across two or more structs.
    StructCycles(struct_cycle_report::args::StructCycleReportArgs),
    /// Report defpackage declarations never used, imported-from, or reached by a qualified symbol.
    UnusedPackages(unused_package_report::args::UnusedPackageReportArgs),
    /// Report defpackage :export symbols never reached by a qualified symbol reference.
    UnusedExports(unused_export_report::args::UnusedExportReportArgs),
    /// Report defpackage forms that export the same symbol more than once.
    DuplicateExports(paredit_feature_package::DuplicateExportReportArgs),
    /// Report defpackage :nicknames never used as a qualifier anywhere.
    UnusedNicknames(unused_nickname_report::args::UnusedNicknameReportArgs),
    /// Report defpackage :use clauses, which widen the importing package's symbol space more than :import-from.
    UseWidening(use_widening_report::args::UseWideningReportArgs),
    /// Report every exported symbol with the signature its export commits to.
    ApiSurface(api_surface_report::args::ApiSurfaceReportArgs),
    /// Compare the current API against an api-surface snapshot and answer major, minor, or patch.
    ApiDiff(api_diff_report::args::ApiDiffReportArgs),
    /// Pair definitions with the tests that name them, and report both sides that have no counterpart.
    TestMap(test_map_report::args::TestMapReportArgs),
    /// Index every symbol to its definition site and occurrence offsets, for editor and agent caches.
    SymbolIndex(symbol_index_report::args::SymbolIndexReportArgs),
    /// Check call sites against &optional, &rest, and &key lambda lists, including unknown keyword names.
    KeywordArity(keyword_arity_report::args::KeywordArityReportArgs),
    /// Report forms that cannot run because a non-local exit precedes them in the same implicit progn.
    UnreachableExpressions(unreachable_expression_report::args::UnreachableExpressionReportArgs),
    /// Report which ASDF systems this project depends on but does not define.
    ExternalSystems(external_system_report::args::ExternalSystemReportArgs),
    /// Report each defsystem's declared licence, its copyleft strength, and which are superseded.
    Licenses(license_report::args::LicenseReportArgs),
    /// Report files missing a leading license header comment, and headers inconsistent with the fileset's majority.
    LicenseHeaders(license_header_report::args::LicenseHeaderReportArgs),
    /// Report components whose declared dependencies contradict or duplicate their system's :serial t.
    SerialConsistency(serial_consistency_report::args::SerialConsistencyReportArgs),
    /// Report the last author, date, and commit for each definition, so a finding can be routed.
    Blame(blame_report::args::BlameReportArgs),
    /// Report what fraction of a file is structurally repeated, and each repeated shape.
    DuplicationRatio(duplication_ratio_report::args::DuplicationRatioReportArgs),
    /// Report per-definition coupling and the file's internal/external call ratio.
    Cohesion(cohesion_report::args::CohesionReportArgs),
    /// Rank definitions by git change frequency multiplied by complexity.
    Hotspots(hotspot_report::args::HotspotReportArgs),
    /// Report one debt score per file, with the weighted contribution of every input shown.
    DebtScore(debt_score_report::args::DebtScoreReportArgs),
    /// Report body forms indented against the Emacs/SLIME convention, which is a different question from format.
    Indentation(indentation_report::args::IndentationReportArgs),
    /// Report definitions with no docstring, and docstrings naming a parameter the lambda list does not have.
    Docstrings(docstring_report::args::DocstringReportArgs),
    /// Report TODO/FIXME/XXX/HACK/BUG markers with the definition each one sits in.
    Todo(todo_report::args::TodoReportArgs),
    /// Report line length, file length, and lines per definition against configurable thresholds.
    LineMetrics(line_metrics_report::args::LineMetricsReportArgs),
    /// Report what each same-file defmacro expands its own call sites into, and why any call was declined.
    MacroExpansion(macro_expansion_report::args::MacroExpansionReportArgs),
    /// Report macro templates that can capture a caller's variable or evaluate a caller's argument twice.
    MacroHygiene(macro_hygiene_report::args::MacroHygieneReportArgs),
    /// Report each loop's clauses: what it binds, what it accumulates, and whether anything can stop it.
    Loop(loop_report::args::LoopReportArgs),
    /// Report format control strings against the arguments their directives consume.
    FormatDirectives(format_directive_report::args::FormatDirectiveReportArgs),
    /// Report every #+/#- reader conditional, the features it tests, and the code it guards.
    ReadConditionals(read_conditional_report::args::ReadConditionalReportArgs),
    /// Report every #. read-time evaluation, separating inert data from a live call.
    ReadTimeEval(read_time_eval_report::args::ReadTimeEvalReportArgs),
    /// Report #n= and #n# reader labels, and each label with no counterpart.
    CircularLiterals(circular_literal_report::args::CircularLiteralReportArgs),
    /// Report symbols whose identity changes with readtable-case, and the escapes that pin it.
    ReadtableCase(readtable_case_report::args::ReadtableCaseReportArgs),
    /// Report definitions and bindings that collide with a COMMON-LISP symbol.
    PackageLocks(package_lock_report::args::PackageLockReportArgs),
    /// Report defmethod qualifiers, and the :before/:after methods with no primary to run around.
    MethodCombination(method_combination_report::args::MethodCombinationReportArgs),
    /// Report the CLOS inheritance tree, each class's inherited slots, and the slots a subclass shadows.
    ClassHierarchy(class_hierarchy_report::args::ClassHierarchyReportArgs),
    /// Report defgeneric declarations against the defmethod forms that implement them.
    GenericDispatch(generic_dispatch_report::args::GenericDispatchReportArgs),
    /// Report established restarts against invoked ones, and each side with no counterpart.
    Restarts(restart_report::args::RestartReportArgs),
    /// Compile with an external Lisp implementation and report its own diagnostics.
    ExternalDiagnostics(external_diagnostics_report::args::ExternalDiagnosticsReportArgs),
    /// Report the semantic layer's proved types, and declarations that contradict what the value layer proved.
    Types(type_report::args::TypeReportArgs),
    /// Report where a branch's test narrows a binding's type, and which branch the narrowing holds in.
    Narrowing(narrowing_report::args::NarrowingReportArgs),
    /// Report expressions that provably evaluate to a literal, and the file-level defconstant values.
    Constants(constant_report::args::ConstantReportArgs),
    /// Report numeric literals outside the idiomatic allow-list, suggesting defconstant extraction.
    MagicNumbers(magic_number_report::args::MagicNumberReportArgs),
    /// Report which bindings carry a provable constant, and the first condition that blocked the rest.
    ValuePropagation(value_propagation_report::args::ValuePropagationReportArgs),
    /// Report each definition as pure, effectful, or undecidable, propagating effects along same-file calls.
    Effects(effect_report::args::EffectReportArgs),
    /// Report how much of the semantic layer resolves on real source: binding and constant-folding coverage, per dialect, with suggested next operators to register.
    SemanticCoverage(semantic_coverage_report::args::SemanticCoverageReportArgs),
    /// Report in-package forms naming a package no analyzed defpackage declares.
    UndefinedPackages(undefined_package_report::args::UndefinedPackageReportArgs),
    /// Report whether a byte offset is code, a string, a comment, a delimiter, or reader sugar.
    ContextAt(context_report::args::ContextAtArgs),
    /// Report whether a write to --file would succeed, without writing anything.
    Writability(writability_report::args::WritabilityReportArgs),
    /// Report schema-free structural sanity issues in an S-expression data file:
    /// duplicate alist/plist keys, an odd-length plist, and mismatched tuple arity.
    DataCheck(data_check_report::args::DataCheckReportArgs),
    /// Diagnose the kill ring file, and with --repair-reset discard a corrupted one.
    KillRing(kill_ring_report::args::KillRingReportArgs),
    /// Report a bare debug-print call (princ/print/prin1/pprint/message/println/... per dialect) left in committed source.
    LeftoverPrintDebug(leftover_print_debug_report::args::LeftoverPrintDebugReportArgs),
    /// Report trace/untrace used as a statement, left in committed source.
    LeftoverTraceCall(leftover_trace_call_report::args::LeftoverTraceCallReportArgs),
    /// Report a Common Lisp (break ...) left in committed source.
    LeftoverBreakCall(leftover_break_call_report::args::LeftoverBreakCallReportArgs),
    /// Report a Common Lisp (inspect x) or (describe x) left in committed source.
    LeftoverInspectCall(leftover_inspect_call_report::args::LeftoverInspectCallReportArgs),
    /// Report a Common Lisp (time form) wrapper left in committed source.
    LeftoverTimeBenchmarkCall(
        leftover_time_benchmark_call_report::args::LeftoverTimeBenchmarkCallReportArgs,
    ),
    /// Report a Common Lisp (step form) wrapper left in committed source.
    LeftoverStepCall(leftover_step_call_report::args::LeftoverStepCallReportArgs),
    /// Report a comment block shaped like a pasted REPL session.
    CommentedReplTranscript(
        commented_repl_transcript_report::args::CommentedReplTranscriptReportArgs,
    ),
    /// Report a (format t ...) whose control string carries a DEBUG/DBG marker.
    LeftoverFormatDebugMarker(
        leftover_format_debug_marker_report::args::LeftoverFormatDebugMarkerReportArgs,
    ),
    /// Report an :around method whose body never calls call-next-method.
    AroundMethodMissingCallNextMethod(around_method_missing_call_next_method_report::args::AroundMethodMissingCallNextMethodReportArgs),
    /// Report a slot with no :initform and no :initarg that a method in the file reads.
    DefclassRequiredSlotNoInitformOrInitarg(defclass_required_slot_no_initform_or_initarg_report::args::DefclassRequiredSlotNoInitformOrInitargReportArgs),
    /// Report a subclass slot that silently shadows a same-file superclass slot.
    DefclassSlotShadowing(defclass_slot_shadowing_report::args::DefclassSlotShadowingReportArgs),
    /// Report two defmethods with the same name, qualifiers and specializers.
    DuplicateDefmethodSignature(duplicate_defmethod_signature_report::args::DuplicateDefmethodSignatureReportArgs),
    /// Report a defgeneric no defmethod in the file ever specializes.
    GenericFunctionNoMethods(generic_function_no_methods_report::args::GenericFunctionNoMethodsReportArgs),
    /// Report a defmethod qualifier outside :before, :after and :around.
    MethodQualifierTypo(method_qualifier_typo_report::args::MethodQualifierTypoReportArgs),
    /// Report a print-object method that writes to the stream directly.
    PrintObjectWithoutPrintUnreadableObject(print_object_without_print_unreadable_object_report::args::PrintObjectWithoutPrintUnreadableObjectReportArgs),
    /// Report a slot-value read of a slot the file declares an accessor for.
    SlotValueBypassesAccessor(slot_value_bypasses_accessor_report::args::SlotValueBypassesAccessorReportArgs),
    /// Report a cerror whose continue-format-control is missing or nil, defeating continuability.
    CerrorMissingContinueFormat(cerror_missing_continue_format_report::args::CerrorMissingContinueFormatReportArgs),
    /// Report a define-condition with an empty () supertype list, which defaults to condition, not error.
    DefineConditionEmptySuperclassList(define_condition_empty_superclass_list_report::args::DefineConditionEmptySuperclassListReportArgs),
    /// Report an error subtype with no :report option and no same-file superclass supplying one.
    DefineConditionMissingReportForErrorType(define_condition_missing_report_for_error_type_report::args::DefineConditionMissingReportForErrorTypeReportArgs),
    /// Report a handler-bind handler ending in a bare value, which handler-bind discards.
    HandlerBindHandlerReturnsBareValue(handler_bind_handler_returns_bare_value_report::args::HandlerBindHandlerReturnsBareValueReportArgs),
    /// Report an ignore-errors around a signal of a same-file non-error condition it cannot catch.
    IgnoreErrorsWrapsNonErrorSignal(ignore_errors_wraps_non_error_signal_report::args::IgnoreErrorsWrapsNonErrorSignalReportArgs),
    /// Report a restart-case clause with no :report option.
    RestartCaseClauseWithoutReport(restart_case_clause_without_report_report::args::RestartCaseClauseWithoutReportReportArgs),
    /// Report a signal of a same-file error subtype, which returns nil instead of entering the debugger.
    SignalOnErrorConditionReturnsSilently(signal_on_error_condition_returns_silently_report::args::SignalOnErrorConditionReturnsSilentlyReportArgs),
    /// Report a dolist result form reading the loop variable, which the spec binds to nil there.
    DolistResultFormReferencesLoopVariable(dolist_result_form_references_loop_variable_report::args::DolistResultFormReferencesLoopVariableReportArgs),
    /// Report assigning the dotimes count variable inside the body, which cannot change the iteration count.
    DotimesBoundMutationHasNoEffect(dotimes_bound_mutation_has_no_effect_report::args::DotimesBoundMutationHasNoEffectReportArgs),
    /// Report a loop variable clause after a main clause, or a named clause that is not first.
    LoopClauseOrderViolation(loop_clause_order_violation_report::args::LoopClauseOrderViolationReportArgs),
    /// Report a loop for-across clause over a value that is provably a list, not a vector.
    LoopForAcrossStaticallyKnownList(loop_for_across_statically_known_list_report::args::LoopForAcrossStaticallyKnownListReportArgs),
    /// Report two loop accumulation clauses building incompatible kinds into the same into variable.
    LoopIntoAccumulatorKindConflict(loop_into_accumulator_kind_conflict_report::args::LoopIntoAccumulatorKindConflictReportArgs),
    /// Report a loop epilogue form after a finally clause that already returns.
    LoopUnreachableFinallyClause(loop_unreachable_finally_clause_report::args::LoopUnreachableFinallyClauseReportArgs),
    /// Report a test disabled in place rather than removed.
    DisabledTestLeftIn(disabled_test_left_in_report::args::DisabledTestLeftInReportArgs),
    /// Report two test definitions in one file sharing a name.
    DuplicateTestName(duplicate_test_name_report::args::DuplicateTestNameReportArgs),
    /// Report a test definition with an empty body.
    EmptyTestBody(empty_test_body_report::args::EmptyTestBodyReportArgs),
    /// Report a sleep-family call inside a test body.
    SleepInTest(sleep_in_test_report::args::SleepInTestReportArgs),
    /// Report a test assertion on a constant that can never fail.
    TestAssertsConstant(test_asserts_constant_report::args::TestAssertsConstantReportArgs),
    /// Report a test definition whose body contains no assertion form.
    TestWithoutAssertion(test_without_assertion_report::args::TestWithoutAssertionReportArgs),
    /// Report a swap!/alter update function with a side effect, which its retries repeat.
    AtomSwapWithSideEffect(atom_swap_with_side_effect_report::args::AtomSwapWithSideEffectReportArgs),
    /// Report a thread body reading a special its enclosing let rebinds, which does not carry over.
    DynamicVarBoundAcrossThreadBoundary(dynamic_var_bound_across_thread_boundary_report::args::DynamicVarBoundAcrossThreadBoundaryReportArgs),
    /// Report a future or promise bound by let and never mentioned, discarding its value and errors.
    FuturePromiseNeverRealized(future_promise_never_realized_report::args::FuturePromiseNeverRealizedReportArgs),
    /// Report a manually acquired lock with no unwind-protect to release it on a non-local exit.
    LockAcquiredNotReleased(lock_acquired_not_released_report::args::LockAcquiredNotReleasedReportArgs),
    /// Report the same non-recursive lock taken again inside its own scope, a deadlock risk.
    RecursiveLockReentryRisk(recursive_lock_reentry_risk_report::args::RecursiveLockReentryRiskReportArgs),
    /// Report a thread body that inlines work with no handler, so an error in it is lost.
    ThreadSpawnedWithoutErrorHandler(thread_spawned_without_error_handler_report::args::ThreadSpawnedWithoutErrorHandlerReportArgs),
    /// Report a global written inside a thread body with no lock held, which races.
    UnsynchronizedSharedMutation(unsynchronized_shared_mutation_report::args::UnsynchronizedSharedMutationReportArgs),
    /// Report a primary asdf perform method on load-op or compile-op that never calls call-next-method.
    AsdfPerformWithoutCallNextMethod(asdf_perform_without_call_next_method_report::args::AsdfPerformWithoutCallNextMethodReportArgs),
    /// Report a defsystem whose :depends-on names the system itself.
    AsdfSelfReferentialDependsOn(asdf_self_referential_depends_on_report::args::AsdfSelfReferentialDependsOnReportArgs),
    /// Report a released defsystem with no :version option.
    AsdfSystemMissingVersion(asdf_system_missing_version_report::args::AsdfSystemMissingVersionReportArgs),
    /// Report a file that declares a package and defines symbols but never enters it.
    DefpackageWithoutInPackage(defpackage_without_in_package_report::args::DefpackageWithoutInPackageReportArgs),
    /// Report a nested block reusing an outer block's name.
    BlockNameShadowsOuterBlock(block_name_shadows_outer_block_report::args::BlockNameShadowsOuterBlockReportArgs),
    /// Report a dotimes or dolist iteration variable assigned inside the body.
    DotimesDolistIndexVarMutated(dotimes_dolist_index_var_mutated_report::args::DotimesDolistIndexVarMutatedReportArgs),
    /// Report a go targeting a tag no enclosing tagbody establishes.
    GoToUndefinedTag(go_to_undefined_tag_report::args::GoToUndefinedTagReportArgs),
    /// Report a multiple-value-bind none of whose variables the body references.
    MultipleValueBindAllIgnored(multiple_value_bind_all_ignored_report::args::MultipleValueBindAllIgnoredReportArgs),
    /// Report a return-from naming a block that does not lexically enclose it.
    ReturnFromUnmatchedBlock(return_from_unmatched_block_report::args::ReturnFromUnmatchedBlockReportArgs),
    /// Report a return with no enclosing form establishing the implicit nil block.
    ReturnOutsideImplicitNilBlock(return_outside_implicit_nil_block_report::args::ReturnOutsideImplicitNilBlockReportArgs),
    /// Report a tagbody label no go in the form ever targets.
    TagbodyUnreachableTag(tagbody_unreachable_tag_report::args::TagbodyUnreachableTagReportArgs),
    /// Report a case/ecase/ccase clause keyed on a string or float literal, which eql does not match dependably.
    CaseKeyEqlPitfall(case_key_eql_pitfall_report::args::CaseKeyEqlPitfallReportArgs),
    /// Report a cond whose every test compares one variable against a literal (case says it directly).
    CondToCaseCandidate(cond_to_case_candidate_report::args::CondToCaseCandidateReportArgs),
    /// Report a cond whose final t clause holds only another cond, which splices into the outer clause list.
    NestedCondFlattenable(nested_cond_flattenable_report::args::NestedCondFlattenableReportArgs),
    /// Report a when/unless value used as an argument to an operator that requires a number.
    WhenUnlessImplicitNilMisused(when_unless_implicit_nil_misused_report::args::WhenUnlessImplicitNilMisusedReportArgs),
    /// Report three or more anonymous lambdas nested with no intervening named binding.
    DeeplyNestedAnonymousLambda(deeply_nested_anonymous_lambda_report::args::DeeplyNestedAnonymousLambdaReportArgs),
    /// Report an flet/labels or nested-defun parameter reusing an enclosing function's parameter name.
    NestedFunctionParameterShadowsEnclosingParameter(nested_function_parameter_shadows_enclosing_parameter_report::args::NestedParameterShadowReportArgs),
    /// Report a definition declaring more required parameters than a threshold.
    OverlyLongParameterList(overly_long_parameter_list_report::args::OverlyLongParameterListReportArgs),
    /// Report a call passing a long run of unlabelled literal arguments of mixed kinds.
    PositionalArgumentCountExceedsReadability(positional_argument_count_exceeds_readability_report::args::PositionalArgumentCountReportArgs),
    /// Report a cond/if chain dispatching on string equality against an enumeration of literals.
    StringlyTypedDispatch(stringly_typed_dispatch_report::args::StringlyTypedDispatchReportArgs),
    /// Report an intern whose package argument is a computed expression, so the target package is not statically knowable.
    InternDynamicPackageTarget(intern_dynamic_package_target_report::args::InternDynamicPackageTargetReportArgs),
    /// Report a lookup that answers nil when not found, applied by funcall/apply with no opportunity to check it.
    IntrospectionProbeUnchecked(introspection_probe_unchecked_report::args::IntrospectionProbeUncheckedReportArgs),
    /// Report a function definition installed under a name built by intern, which no search can connect to its callers.
    SymbolFunctionFsetDynamicName(symbol_function_fset_dynamic_name_report::args::SymbolFunctionFsetDynamicNameReportArgs),
    /// Report a destructuring-bind that binds a &whole variable and never references it.
    DestructuringBindUnusedWhole(destructuring_bind_unused_whole_report::args::DestructuringBindUnusedWholeReportArgs),
    /// Report a loop whose only collect ... into accumulator is returned unchanged by finally.
    LoopCollectIntoImmediatelyReturned(loop_collect_into_immediately_returned_report::args::LoopCollectIntoImmediatelyReturnedReportArgs),
    /// Report an flet/labels defining one local function whose only use is a tail call that is the whole body.
    FletSingleUseInlinable(flet_single_use_inlinable_report::args::FletSingleUseInlinableReportArgs),
    /// Report a multiple-value-setq whose variable list is a different length from its literal (values ...).
    MultipleValueSetqArityMismatch(multiple_value_setq_arity_mismatch_report::args::MultipleValueSetqArityMismatchReportArgs),
    /// Report an open or with-open-file with an explicit :direction :input, which is already the default.
    WithOpenFileRedundantDirectionDefault(with_open_file_redundant_direction_default_report::args::WithOpenFileRedundantDirectionDefaultReportArgs),
    /// Report a declaimed ftype whose (values ...) return arity exceeds what its defun returns.
    FtypeValuesArityMismatch(ftype_values_arity_mismatch_report::args::FtypeValuesArityMismatchReportArgs),
    /// Report a with-slots/with-accessors with an empty binding list, which is just progn.
    WithAccessorsEmptyBindingList(with_accessors_empty_binding_list_report::args::WithAccessorsEmptyBindingListReportArgs),
    /// Report a quoted form containing a , or ,@ with no enclosing backquote, which Common Lisp refuses to read.
    QuotedFormContainsStrayUnquote(quoted_form_contains_stray_unquote_report::args::QuotedFormContainsStrayUnquoteReportArgs),
    /// Report an element read by position out of a hash table's iteration, whose order is unspecified.
    HashTableIterationOrderAssumed(hash_table_iteration_order_assumed_report::args::HashTableIterationOrderAssumedReportArgs),
    /// Report a member against a long literal list of symbols, which is a set in disguise.
    SetMembershipViaLinearScan(set_membership_via_linear_scan_report::args::SetMembershipViaLinearScanReportArgs),
    /// Report nested gets reading one path, which is get-in.
    NestedGetChain(nested_get_chain_report::args::NestedGetChainReportArgs),
    /// Report an into onto an empty vector or set, which is a direct conversion.
    RedundantIntoEmptyCollection(redundant_into_empty_collection_report::args::RedundantIntoEmptyCollectionReportArgs),
    /// Report a single-float literal beside a double-float literal in one arithmetic form.
    MixedFloatPrecisionArithmetic(mixed_float_precision_arithmetic_report::args::MixedFloatPrecisionArithmeticReportArgs),
    /// Report an Emacs Lisp integer division whose quotient truncates to zero, discarding the value.
    DivisionResultPrecisionLoss(division_result_precision_loss_report::args::DivisionResultPrecisionLossReportArgs),
    /// Report a do loop stepping an inexact float that terminates on = or eql rather than an ordered comparison.
    EpsilonLessFloatLoopBound(epsilon_less_float_loop_bound_report::args::EpsilonLessFloatLoopBoundReportArgs),
    /// Report a float coercion immediately discarded by a truncate/floor/ceiling/round around it.
    RedundantPrecisionCoercion(redundant_precision_coercion_report::args::RedundantPrecisionCoercionReportArgs),
    /// Report a ~ directive in a literal format control string that CLHS 22.3 does not define.
    FormatUnknownDirective(format_unknown_directive_report::args::FormatUnknownDirectiveReportArgs),
    /// Report a ~%~& in a literal format control string, where the ~& is already at the start of a line.
    FormatPercentAmpersandAdjacentRedundancy(format_percent_ampersand_adjacent_redundancy_report::args::FormatPercentAmpersandAdjacentRedundancyReportArgs),
    /// Report an unbalanced ~[ ~{ ~< ~( bracketing construct in a literal format control string.
    FormatNestedDirectiveUnbalanced(format_nested_directive_unbalanced_report::args::FormatNestedDirectiveUnbalancedReportArgs),
    /// Report a top-level in-package that re-enters a package the file had already left.
    PackageCircularInPackageChain(package_circular_in_package_chain_report::args::CircularInPackageChainReportArgs),
}

/// Single-document structural editing commands. These print rewritten source
/// to stdout by default; mutating commands accept --write to update --file in
/// place with reparse validation and rollback.
#[derive(Debug, Subcommand)]
#[command(
    after_help = "Examples:\n  paredit edit select --file src/foo.lisp --path 0.2\n  paredit edit wrap --file src/foo.lisp --path 0.2 --diff\n  paredit edit wrap --file src/foo.lisp --path 0.2 --write\n  paredit edit replace --file src/foo.lisp --at 120 --with '(new-form)' --write\n  paredit edit navigate --file src/foo.lisp --path 0.2 --direction forward\n\nWithout --write the rewritten document is printed to stdout and the file is untouched.\nUse --diff to print a unified diff instead of the whole rewritten document."
)]
pub(super) enum EditCommand {
    /// Print a canonical, indentation-based rendering.
    Format(FormatArgs),
    /// Append required closing delimiters only when input has unclosed lists.
    RepairUnclosedLists(RepairArgs),
    /// Sort an alist- or plist-shaped data file's keys and flatten its whitespace to a single space between elements.
    Canonicalize(CanonicalizeArgs),
    /// Print the S-expression selected by --path or --at.
    Select(TargetArgs),
    /// Replace the selected S-expression with replacement text.
    Replace(ReplaceArgs),
    /// Remove the selected S-expression, optionally pushing it onto the kill ring.
    Kill(KillArgs),
    /// Print the selected S-expression with the comment block written above it.
    Copy(CopyArgs),
    /// Write a second copy of the selected S-expression immediately after it, without using the kill ring.
    Duplicate(EditTargetArgs),
    /// Rewrite the selected quote between its two spellings: 'x / (quote x), #'f / (function f).
    NormalizeQuotes(NormalizeQuotesArgs),
    /// Paste a kill ring entry beside, or over, the selected S-expression.
    Yank(YankArgs),
    /// Wrap the selected S-expression in a delimiter pair, a string, or a reader prefix.
    Wrap(WrapArgs),
    /// Remove the selected S-expression's reader prefix, outermost first.
    UnwrapPrefix(UnwrapPrefixArgs),
    /// Remove one list pair while keeping its children.
    Splice(EditTargetArgs),
    /// Split the enclosing list in two immediately before the selected expression.
    Split(EditTargetArgs),
    /// Join the selected list with its next sibling list into one list.
    Join(EditTargetArgs),
    /// Splice the enclosing list, killing every sibling before the selection.
    SpliceKillingBackward(EditTargetArgs),
    /// Splice the enclosing list, killing the selection and every sibling after it.
    SpliceKillingForward(EditTargetArgs),
    /// Reverse the nesting of the two lists enclosing the selected list.
    Convolute(EditTargetArgs),
    /// Replace an enclosing list with the selected expression, --levels deep.
    Raise(RaiseArgs),
    /// Exchange the selected expression with its next sibling.
    TransposeForward(EditTargetArgs),
    /// Exchange the selected expression with its previous sibling.
    TransposeBackward(EditTargetArgs),
    /// Exchange the selected expression with any other expression in the same list.
    Transpose(TransposeArgs),
    /// Pull the next sibling into the selected list.
    SlurpForward(EditTargetArgs),
    /// Pull the previous sibling into the selected list.
    SlurpBackward(EditTargetArgs),
    /// Push the last child out of the selected list.
    BarfForward(EditTargetArgs),
    /// Push the first child out of the selected list.
    BarfBackward(EditTargetArgs),
    /// Report the --path one structural move lands on.
    Navigate(NavigateArgs),
    /// Delete the character at --at, refusing anything that unbalances the document.
    DeleteForward(CursorArgs),
    /// Delete the character before --at, refusing anything that unbalances the document.
    DeleteBackward(CursorArgs),
    /// Insert a newline at --at and reindent the definition it lands in.
    Newline(NewlineArgs),
    /// Reindent the selected definition without rewrapping its lines.
    ReindentDefun(ReindentArgs),
    /// Split the string literal containing --at into two adjacent literals.
    SplitString(CursorArgs),
    /// Escape the selected string literal's contents one level.
    EscapeString(EditTargetArgs),
    /// Reverse one level of escaping in the selected string literal.
    UnescapeString(EditTargetArgs),
}

#[derive(Debug, Subcommand)]
#[command(
    after_help = "Examples:\n  paredit refactor plan --symbol old-name src/foo.lisp src/bar.lisp\n  paredit refactor preview --from old-name --to new-name src/foo.lisp src/bar.lisp\n  paredit refactor verify --symbol old-name --new-symbol new-name --phase post src/foo.lisp src/bar.lisp"
)]
pub(super) enum RefactorCommand {
    /// Produce an ordered, gated refactoring plan for AI coding agents.
    Plan(refactor::args::RefactorPlanArgs),
    /// Verify pre/post refactoring invariants for AI coding agents and CI gates.
    Verify(refactor::args::VerifyRefactorArgs),
    /// Preview exact refactoring rewrites without modifying files.
    Preview(refactor::args::RefactorPreviewArgs),
    /// Validate a refactor preview manifest without writing files or rendering diffs.
    Check(refactor::args::RefactorCheckArgs),
    /// Summarize a refactor preview manifest into agent-safe next actions.
    Status(refactor::args::RefactorStatusArgs),
    /// Apply a previously generated refactor preview manifest with hash guards.
    Apply(refactor::args::RefactorApplyArgs),
    /// Restore the pre-refactor content recorded by `refactor apply --undo-out`.
    Undo(refactor::args::RefactorUndoArgs),
    /// Render a verified diff from a refactor preview manifest without writing files.
    Diff(refactor::args::RefactorDiffArgs),
    /// Carry the difference between two versions of one file onto a third,
    /// matching each change by structure rather than by position.
    Patch(structural_patch::args::StructuralPatchArgs),
    /// Walk a preview manifest one edit at a time, taking only the steps you
    /// accept.
    Step(refactor_step::args::RefactorStepArgs),
    /// Record the current content of files as a named checkpoint.
    CreateCheckpoint(refactor_checkpoint::args::CreateCheckpointArgs),
    /// List every registered checkpoint.
    ListCheckpoints(refactor_checkpoint::args::ListCheckpointsArgs),
    /// Report whether a checkpoint can be restored, and with `--write`,
    /// confirm it still holds.
    RestoreCheckpoint(refactor_checkpoint::args::RestoreCheckpointArgs),
    /// Remove a checkpoint from the registry.
    DeleteCheckpoint(refactor_checkpoint::args::DeleteCheckpointArgs),
    /// Discover Lisp sources under roots and build a gated refactor plan.
    WorkspacePlan(refactor::args::WorkspaceRefactorPlanArgs),
    /// Discover Lisp sources under roots and preview exact refactoring rewrites.
    WorkspacePreview(refactor::args::WorkspaceRefactorPreviewArgs),
    /// Execute a workspace refactor with preview gates and post-write verification.
    WorkspaceExecute(refactor::args::WorkspaceRefactorExecuteArgs),
    /// Plan or remove a top-level definition from one file.
    RemoveDefinition(definition_removal::args::RemoveDefinitionArgs),
    /// Plan or remove unused top-level definitions across explicit files.
    RemoveUnusedDefinitions(definition_removal::args::RemoveUnusedDefinitionsArgs),
    /// Replace every expression `inspect constants` proves constant with the literal it evaluates to.
    FoldConstants(fold_constants::args::FoldConstantsArgs),
    /// Insert (declare (ignore ...)) for every parameter `inspect unused-parameters` reports as unused.
    AddIgnoreDeclaration(add_ignore_declaration::args::AddIgnoreDeclarationArgs),
    /// Plan or move a top-level definition between files.
    MoveDefinition(definition_movement::args::MoveDefinitionArgs),
    /// Plan or split multiple top-level definitions into another file.
    SplitFile(definition_movement::args::SplitFileArgs),
    /// Plan or sort contiguous top-level definition blocks inside one file.
    SortDefinitions(definition_movement::args::SortDefinitionsArgs),
    /// Plan or move any top-level form between files.
    MoveForm(definition_movement::args::MoveFormArgs),
    /// Insert one complete top-level S-expression into a Lisp source file.
    InsertTopLevel(definition_movement::args::InsertTopLevelArgs),
    /// Convert duplicate groups into reviewed replace-forms batches.
    ReplacementPlan(duplicate_report::args::ReplacementPlanArgs),
    /// Plan or replace multiple reviewed forms in one file.
    ReplaceForms(replace_forms::ReplaceFormsArgs),
    /// Plan or add a symbol to a Common Lisp defpackage :export option.
    AddExport(package::types::AddExportArgs),
    /// Plan or sort Common Lisp defpackage :export symbol designators.
    SortPackageExports(package::types::SortPackageExportsArgs),
    /// Plan or sort Common Lisp defpackage option forms.
    SortPackageOptions(package::types::SortPackageOptionsArgs),
    /// Plan or merge duplicate Common Lisp defpackage option forms.
    MergePackageOptions(package::types::MergePackageOptionsArgs),
    /// Plan or rename Common Lisp package designators and qualified prefixes.
    RenamePackage(package::types::RenamePackageArgs),
    /// Rename whatever symbol occupies a byte offset, dispatching to
    /// whichever namespace and lexical scope actually own it.
    RenameAt(rename::args::RenameAtArgs),
    /// Rename exact atom occurrences without touching strings or comments.
    RenameSymbol(rename::args::RenameSymbolArgs),
    /// Rename exact atom occurrences inside one selected form.
    RenameInForm(rename::args::RenameInFormArgs),
    /// Rename one local binding and only the references in its lexical scope.
    RenameBinding(rename::args::RenameBindingArgs),
    /// Rename a selected Common Lisp block and matching return-from references.
    RenameBlock(rename_control::RenameBlockArgs),
    /// Rename one tag in a selected Common Lisp tagbody and matching go references.
    RenameTag(rename_control::RenameTagArgs),
    /// Remove a selected Common Lisp block with no matching return-from.
    RemoveUnusedBlock(remove_unused_control::RemoveUnusedBlockArgs),
    /// Remove an unreferenced tag from a selected Common Lisp tagbody.
    RemoveUnusedTag(remove_unused_control::RemoveUnusedTagArgs),
    /// Plan or apply an exact atom rename across explicit files.
    RenameSymbols(rename::args::RenameSymbolsArgs),
    /// Plan or apply a Common Lisp callable definition and callable-designator rename across explicit files, including function, macro-function, compiler-macro-function, symbol-function, fdefinition, setf names, and definition forms such as define-method-combination.
    RenameFunction(rename::args::RenameFunctionArgs),
    /// Plan or apply a Common Lisp macrolet/compiler-macrolet binding and call-site rename across explicit files while keeping expander bodies out of scope.
    RenameMacrolet(rename::args::RenameMacroletArgs),
    /// Plan or apply a Common Lisp define-symbol-macro binding and value-reference rename across explicit files while keeping expansion and lexical shadowing boundaries separate.
    RenameSymbolMacro(rename::args::RenameSymbolMacroArgs),
    /// Plan or apply a Common Lisp flet/labels local function binding and call-site rename across explicit files, preserving the difference between non-recursive flet bodies and recursive labels bodies.
    RenameLocalFunction(rename::args::RenameLocalFunctionArgs),
    /// Plan or replace callable call-site heads across explicit files.
    ReplaceFunctionCalls(rename::args::ReplaceFunctionCallsArgs),
    /// Plan or wrap callable call sites in another function or macro call.
    WrapFunctionCalls(rename::args::WrapFunctionCallsArgs),
    /// Plan or remove a unary wrapper around callable call sites.
    UnwrapFunctionCalls(rename::args::UnwrapFunctionCallsArgs),
    /// Replace one selected wrapper call with one selected argument.
    UnwrapCall(unwrap_call::UnwrapCallArgs),
    /// Convert a selected nested call chain into a thread-first or thread-last pipeline.
    ThreadExpression(thread_expression::ThreadExpressionArgs),
    /// Convert a selected thread-first or thread-last pipeline into nested calls.
    UnthreadExpression(unthread_expression::UnthreadExpressionArgs),
    /// Extract the selected expression into a top-level function with inferred parameters.
    ExtractFunction(extract_function::ExtractFunctionArgs),
    /// Extract the selected expression into a local flet or labels function.
    ExtractLocalFunction(extract_local_function::ExtractLocalFunctionArgs),
    /// Extract the selected expression into a top-level constant.
    ExtractConstant(extract_constant::ExtractConstantArgs),
    /// Inline one selected function call using a selected function definition.
    InlineFunction(inline_function::InlineFunctionArgs),
    /// Replace an immediately invoked Common Lisp lambda with a parallel let.
    InlineLambda(inline_lambda::InlineLambdaArgs),
    /// Inline the sole direct call in a single-binding Common Lisp flet form.
    InlineLocalFunction(inline_local_function::InlineLocalFunctionArgs),
    /// Expand one conservative Common Lisp symbol-macrolet binding.
    InlineSymbolMacro(inline_symbol_macro::InlineSymbolMacroArgs),
    /// Inline an immutable self-evaluating Common Lisp defconstant value.
    InlineLiteralConstant(inline_literal_constant::InlineLiteralConstantArgs),
    /// Add a parameter to a selected function and explicit call sites.
    AddFunctionParameter(function_parameter::args::AddFunctionParameterArgs),
    /// Move one positional parameter in a selected function and explicit call sites.
    MoveFunctionParameter(function_parameter::args::MoveFunctionParameterArgs),
    /// Swap two positional parameters in a selected function and explicit call sites.
    SwapFunctionParameters(function_parameter::args::SwapFunctionParametersArgs),
    /// Reorder all positional parameters in a selected function and explicit call sites.
    ReorderFunctionParameters(function_parameter::args::ReorderFunctionParametersArgs),
    /// Remove one positional parameter from a selected function and explicit call sites.
    RemoveFunctionParameter(function_parameter::args::RemoveFunctionParameterArgs),
    /// Replace the selected expression with a local binding in the enclosing list.
    IntroduceLet(introduce_let::IntroduceLetArgs),
    /// Inline a single local let binding into its body.
    InlineLet(inline_let::InlineLetArgs),
    /// Convert a Common Lisp or Emacs Lisp parallel let form into let*.
    ConvertLetToLetStar(convert_let_to_let_star::ConvertLetToLetStarArgs),
    /// Convert an independent Common Lisp let* form into let.
    ConvertLetStarToLet(convert_let_star_to_let::ConvertLetStarToLetArgs),
    /// Convert an independent Common Lisp do* form into do.
    ConvertDoStarToDo(convert_sequential_binding::ConvertDoStarToDoArgs),
    /// Convert an independent Common Lisp prog* form into prog.
    ConvertProgStarToProg(convert_sequential_binding::ConvertProgStarToProgArgs),
    /// Merge a directly nested Common Lisp or Emacs Lisp let* form.
    MergeNestedLetStar(merge_nested_let_star::MergeNestedLetStarArgs),
    /// Merge directly nested independent Common Lisp or Emacs Lisp let forms.
    MergeNestedLet(merge_nested_let::MergeNestedLetArgs),
    /// Merge directly nested Common Lisp flet forms when definition scope is unchanged.
    MergeNestedFlet(merge_nested_flet::MergeNestedFletArgs),
    /// Split a Common Lisp or Emacs Lisp let* at a binding boundary.
    SplitLetStar(split_let_star::SplitLetStarArgs),
    /// Split a Common Lisp or Emacs Lisp let without capturing free references.
    SplitLet(split_let::SplitLetArgs),
    /// Remove an empty Common Lisp or Emacs Lisp let or let* in an expression position.
    EliminateEmptyBindingForm(eliminate_empty_binding_form::EliminateEmptyBindingFormArgs),
    /// Flatten directly nested progn forms in a conservative expression context.
    FlattenProgn(flatten_progn::FlattenPrognArgs),
    /// Convert a Common Lisp or Emacs Lisp if form into cond.
    ConvertIfToCond(convert_if_to_cond::ConvertIfToCondArgs),
    /// Convert a Common Lisp or Emacs Lisp cond form into nested if forms.
    ConvertCondToIf(convert_cond_to_if::ConvertCondToIfArgs),
    /// Convert a Common Lisp or Emacs Lisp when form into if.
    ConvertWhenToIf(convert_when_to_if::ConvertWhenToIfArgs),
    /// Convert a Common Lisp or Emacs Lisp unless form into if.
    ConvertUnlessToIf(convert_unless_to_if::ConvertUnlessToIfArgs),
    /// Convert a Common Lisp or Emacs Lisp if form without a meaningful else into when.
    ConvertIfToWhen(convert_if_to_when::ConvertIfToWhenArgs),
    /// Convert a Common Lisp or Emacs Lisp if form with a nil then branch into unless.
    ConvertIfToUnless(convert_if_to_unless::ConvertIfToUnlessArgs),
    /// Convert a non-recursive Common Lisp labels form into flet.
    ConvertLabelsToFlet(convert_labels_to_flet::ConvertLabelsToFletArgs),
    /// Convert a Common Lisp flet form into labels when no definition reference can be captured.
    ConvertFletToLabels(convert_flet_to_labels::ConvertFletToLabelsArgs),
    /// Plan or remove one unused local let binding.
    RemoveUnusedBinding(remove_unused_binding::RemoveUnusedBindingArgs),
}

/// Generators that produce new Common Lisp source: a `defpackage` from a
/// file's definitions, an ASDF `defsystem` from a directory, test skeletons
/// for untested definitions, CLOS accessors for bare slots, a `defgeneric`
/// for an undeclared method group, and a docstring template for a
/// definition. Common Lisp only.
#[derive(Debug, Subcommand)]
#[command(
    after_help = "Examples:\n  paredit generate defpackage --file src/app.lisp\n  paredit generate defsystem . --write\n  paredit generate tests src/app.lisp --into tests/app-tests.lisp --write\n  paredit generate accessors --file src/point.lisp --name point --write\n  paredit generate defgeneric --file src/app.lisp --write\n  paredit generate docstring --file src/app.lisp --name render --write"
)]
pub(super) enum GenerateCommand {
    /// Generate a `defpackage` form from a file's own definitions and
    /// qualified symbol references.
    Defpackage(generate_defpackage::GenerateDefpackageArgs),
    /// Generate an ASDF `defsystem` form from a directory of Lisp sources.
    Defsystem(generate_defsystem::GenerateDefsystemArgs),
    /// Generate a `deftest` skeleton for every definition with no test.
    Tests(generate_tests::GenerateTestsArgs),
    /// Add `:accessor` to every `defclass` slot that has neither `:accessor`,
    /// `:reader`, nor `:writer`.
    Accessors(generate_accessors::GenerateAccessorsArgs),
    /// Generate a `defgeneric` for a name whose `defmethod` forms have no
    /// declaration.
    Defgeneric(generate_defgeneric::GenerateDefgenericArgs),
    /// Insert a docstring template at the position Common Lisp expects it.
    Docstring(generate_docstring::GenerateDocstringArgs),
}

/// Configuration introspection. Reads `paredit.toml`; never reads source.
#[derive(Debug, Subcommand)]
#[command(
    after_help = "Examples:\n  paredit config check\n  paredit config show --changed-only --output text\n  paredit config show --key lint.preset\n  paredit config schema --output text\n  paredit config init --dry-run\n\nLayers, lowest precedence first: built-in defaults, the user's file, the\nrepository's file, each nested directory's file, then PAREDIT_* variables."
)]
pub(super) enum ConfigCommand {
    /// Validate every discovered configuration file and exit 3 if any key is unusable.
    Check(config::args::ConfigCheckArgs),
    /// Print the effective configuration with the file and line that set each key.
    Show(config::args::ConfigShowArgs),
    /// Print every recognised key with its type, default, and environment variable.
    Schema(config::args::ConfigSchemaArgs),
    /// Write a documented starter paredit.toml generated from the schema.
    Init(config::args::ConfigInitArgs),
}

/// The `query` namespace: the pattern language as a first-class capability
/// rather than as one of eight ways to select a form.
#[derive(Debug, Subcommand)]
pub(super) enum QueryCommand {
    /// Report every form in the workspace whose shape matches --query.
    Find(query_find::QueryFindArgs),
    /// Count matches per pattern and per file, for several patterns at once.
    Count(query_count::QueryCountArgs),
    /// Rewrite every match with a --rewrite template. Writes only with --write.
    Replace(query_replace::QueryReplaceArgs),
}

#[derive(Debug, Subcommand)]
pub(super) enum Command {
    /// Read-only inventory, validation, and analysis.
    Inspect {
        #[command(subcommand)]
        command: InspectCommand,
    },
    /// Structural edits on one selected form. Prints rewritten source to stdout, or updates --file in place with --write.
    Edit {
        #[command(subcommand)]
        command: EditCommand,
    },
    /// Semantic refactors, including planning, previews, verification, and apply flows.
    Refactor {
        #[command(subcommand)]
        command: RefactorCommand,
    },
    /// Search the workspace by S-expression pattern, count matches, and
    /// rewrite them with a template.
    Query {
        #[command(subcommand)]
        command: QueryCommand,
    },
    /// Apply the lint auto-fixes. The write side of `inspect lint`, under a
    /// name that says it writes.
    Fix {
        #[command(subcommand)]
        command: FixCommand,
    },
    /// Run a named, ordered, dialect-scoped codemod recipe.
    Migrate {
        #[command(subcommand)]
        command: MigrateCommand,
    },
    /// Validate a Lisp data file against a small, dependency-light schema
    /// language of its own, `defschema` — never evaluating either.
    Schema {
        #[command(subcommand)]
        command: SchemaCommand,
    },
    /// Inspect, validate, and scaffold the layered paredit.toml configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Generators that produce new Common Lisp source from what this tool
    /// already knows how to analyze.
    Generate {
        #[command(subcommand)]
        command: GenerateCommand,
    },
    /// Run a Language Server Protocol server over stdio.
    Lsp(crate::presentation::lsp::LspArgs),
    /// Run a Model Context Protocol server over stdio, for AI coding agents.
    Mcp(crate::presentation::mcp::McpArgs),
    /// Run a resident analysis server over HTTP and JSON-RPC, sharing one
    /// parse and lint cache across calls.
    Serve(crate::presentation::serve::ServeArgs),
    /// Interactively browse one file's tree; prints the last selected --path on exit.
    Tui(crate::presentation::tui::TuiArgs),
    /// Print a shell completion script to stdout.
    Completions {
        /// Shell to generate a completion script for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}
