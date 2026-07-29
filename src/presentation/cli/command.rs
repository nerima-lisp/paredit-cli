use super::{
    accessor_arity_report, append_list_to_cons_report, append_nil_report,
    args::{
        AnalyzeArgs, EditTargetArgs, FormatArgs, RepairArgs, ReplaceArgs, TargetArgs, WrapArgs,
    },
    binds_constant_report, butlast_default_count_report, call_cycle_report, call_graph_report,
    call_report, capabilities, car_nthcdr_report, car_reverse_report, case_nil_key_report,
    char_case_fold_report, char_op_string_report, class_cycle_report, code_char_char_code_report,
    coerce_to_t_report, complexity_report, cond_t_clause_report, cons_to_list_report,
    constant_if_test_report, constant_when_test_report, convert_cond_to_if, convert_flet_to_labels,
    convert_if_to_cond, convert_if_to_unless, convert_if_to_when, convert_labels_to_flet,
    convert_let_star_to_let, convert_let_to_let_star, convert_sequential_binding,
    convert_unless_to_if, convert_when_to_if, de_morgan_report, dead_boolean_operand_report,
    definition_movement, definition_removal, definition_report, defpackage_quoted_report,
    dependency_report, destructive_literal_report, double_reverse_report,
    duplicate_boolean_operand_report, duplicate_case_key_report, duplicate_cond_test_report,
    duplicate_export_report, duplicate_keyword_report, duplicate_lambda_list_keyword_report,
    duplicate_let_binding_report, duplicate_method_report, duplicate_parameter_report,
    duplicate_report, duplicate_setf_place_report, duplicate_slot_report,
    eliminate_empty_binding_form, emacs_lisp_file_report, empty_body_report, empty_let_report,
    eq_char_comparison_report, eq_number_comparison_report, eql_list_comparison_report,
    eql_search_literal_report, eql_string_comparison_report, equality_arity_report,
    eval_when_situation_report, exhaustive_case_otherwise_report, explicit_nil_return_report,
    explicit_step_delta_report, extract_constant, extract_function, extract_local_function,
    flatten_progn, form_report, format_missing_destination_report, format_newline_report,
    format_to_string_report, funcall_lambda_report, function_parameter, getf_default_nil_report,
    gethash_default_report, handler_case_no_clauses_report, identical_if_branch_report,
    identity_arithmetic_report, if_arity_report, if_not_report, if_to_or_report,
    if_to_unless_report, impact_report, inline_function, inline_lambda, inline_let,
    inline_literal_constant, inline_local_function, inline_symbol_macro, introduce_let,
    lambda_list_keyword_order_report, last_default_count_report, let_report, lint_report,
    list_star_nil_report, list_star_to_cons_report, literal_place_report,
    make_array_default_keyword_report, make_hash_table_test_report,
    make_list_default_element_report, malformed_case_clause_report, malformed_cond_clause_report,
    malformed_iteration_spec_report, malformed_let_binding_report, manual_incf_report,
    manual_push_report, manual_pushnew_report, merge_nested_flet, merge_nested_let,
    merge_nested_let_star, modify_macro_arity_report, multiple_value_list_of_values_report,
    naming_report, negated_comparison_report, negated_if_report, negated_step_delta_report,
    negated_when_unless_report, nested_boolean_report, nested_char_case_report, nested_cxr_report,
    nested_progn_report, nested_string_case_report, nested_unless_report, nested_when_report,
    nil_comparison_report, nth_constant_index_report, nthcdr_small_index_report,
    nthcdr_zero_report, one_armed_if_report, one_step_arithmetic_report, package,
    package_boundary_report, package_conflict_report, package_cycle_report,
    parse_integer_default_radix_report, prog2_to_progn_report, quoted_case_key_report,
    reachability_report, redefinition_report, redundant_apply_report, redundant_body_progn_report,
    redundant_boolean_identity_report, redundant_count_nil_report, redundant_divisor_report,
    redundant_end_nil_report, redundant_eql_test_report, redundant_from_end_nil_report,
    redundant_funcall_report, redundant_identity_key_report, redundant_identity_report,
    redundant_if_nil_report, redundant_let_star_report, redundant_prog1_report,
    redundant_progn_report, redundant_quote_report, redundant_start_zero_report,
    redundant_the_report, refactor, remove_unused_binding, remove_unused_control, rename,
    rename_control, replace_forms, self_assignment_report, self_comparison_report,
    setf_arity_report, setq_non_variable_report, shadowed_binding_report,
    sharp_quoted_lambda_report, sign_comparison_report, signature_report, similarity_report,
    single_arg_comparison_report, single_clause_cond_report, single_operand_arithmetic_report,
    single_operand_boolean_report, single_operand_list_op_report, single_value_bind_report,
    source_report, split_let, split_let_star, step_zero_report, string_case_fold_report,
    struct_cycle_report, subseq_zero_report, symbol_report, system_conflict_report,
    system_cycle_report, t_comparison_report, the_arity_report, thread_expression,
    typecase_nil_key_report, typep_predicate_report, undefined_package_report,
    unreachable_case_clause_report, unreachable_cond_clause_report, unthread_expression,
    unused_export_report, unused_local_callable_report, unused_nickname_report,
    unused_package_report, unused_parameter_report, unwind_protect_no_cleanup_report, unwrap_call,
    values_list_of_list_report, verbose_negation_report, workspace_report, zero_divisor_report,
};
use clap::Subcommand;

/// Read-only inventory and analysis commands.
#[derive(Debug, Subcommand)]
#[command(
    after_help = "Examples:\n  paredit inspect check --file src/foo.lisp\n  paredit inspect outline --file src/foo.lisp --output json\n  paredit inspect symbols --symbol old-name --output json src/a.lisp src/b.lisp\n  paredit inspect workspace --output json .\n  paredit inspect capabilities --output json"
)]
pub(super) enum InspectCommand {
    /// Validate that input is a balanced S-expression document.
    Check(AnalyzeArgs),
    /// Detect Lisp dialect from --file extension or explicit --dialect.
    Dialect(AnalyzeArgs),
    /// Print parse, dialect, and structural metrics for agent planning.
    Stats(AnalyzeArgs),
    /// Print a complete JSON report for AI coding agent refactor planning.
    AgentReport(AnalyzeArgs),
    /// Print a machine-readable catalog of every command, flag, default, and enum value.
    Capabilities(capabilities::CapabilitiesArgs),
    /// Print top-level forms with paths, spans, and definition hints.
    Outline(AnalyzeArgs),
    /// Report one selected form with local structure for agent refactor planning.
    Form(form_report::args::FormReportArgs),
    /// Find exact atom occurrences without touching strings or comments.
    FindSymbol(symbol_report::args::SymbolQueryArgs),
    /// Report exact atom occurrences across explicit files for rename planning.
    Symbols(symbol_report::args::SymbolReportArgs),
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
    /// Report a setf/setq/psetf/psetq that assigns the same variable more than once.
    DuplicateSetfPlaces(duplicate_setf_place_report::args::DuplicateSetfPlaceReportArgs),
    /// Report defclass/define-condition/defstruct forms declaring the same slot name more than once.
    DuplicateSlots(duplicate_slot_report::args::DuplicateSlotReportArgs),
    /// Report Emacs Lisp per-file facts: lexical-binding, provided and required features, autoload cookies.
    ElispFile(emacs_lisp_file_report::args::EmacsLispFileReportArgs),
    /// Report defmethod forms with the same name, qualifier, and specializers declared more than once.
    DuplicateMethods(duplicate_method_report::args::DuplicateMethodReportArgs),
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
    UnusedParameters(unused_parameter_report::args::UnusedParameterReportArgs),
    /// Report let-family bindings that shadow an enclosing parameter or let binding.
    ShadowedBindings(shadowed_binding_report::args::ShadowedBindingReportArgs),
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
    DuplicateExports(duplicate_export_report::args::DuplicateExportReportArgs),
    /// Report defpackage :nicknames never used as a qualifier anywhere.
    UnusedNicknames(unused_nickname_report::args::UnusedNicknameReportArgs),
    /// Report in-package forms naming a package no analyzed defpackage declares.
    UndefinedPackages(undefined_package_report::args::UndefinedPackageReportArgs),
}

/// Single-document structural editing commands. These print rewritten source
/// to stdout by default; mutating commands accept --write to update --file in
/// place with reparse validation and rollback.
#[derive(Debug, Subcommand)]
#[command(
    after_help = "Examples:\n  paredit edit select --file src/foo.lisp --path 0.2\n  paredit edit wrap --file src/foo.lisp --path 0.2 --diff\n  paredit edit wrap --file src/foo.lisp --path 0.2 --write\n  paredit edit replace --file src/foo.lisp --at 120 --with '(new-form)' --write\n\nWithout --write the rewritten document is printed to stdout and the file is untouched.\nUse --diff to print a unified diff instead of the whole rewritten document."
)]
pub(super) enum EditCommand {
    /// Print a canonical, indentation-based rendering.
    Format(FormatArgs),
    /// Append required closing delimiters only when input has unclosed lists.
    RepairUnclosedLists(RepairArgs),
    /// Print the S-expression selected by --path or --at.
    Select(TargetArgs),
    /// Replace the selected S-expression with replacement text.
    Replace(ReplaceArgs),
    /// Remove the selected S-expression.
    Kill(EditTargetArgs),
    /// Wrap the selected S-expression in a new list, optionally choosing the delimiter.
    Wrap(WrapArgs),
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
    /// Replace the selected expression's parent list with the selected expression.
    Raise(EditTargetArgs),
    /// Exchange the selected expression with its next sibling.
    TransposeForward(EditTargetArgs),
    /// Exchange the selected expression with its previous sibling.
    TransposeBackward(EditTargetArgs),
    /// Pull the next sibling into the selected list.
    SlurpForward(EditTargetArgs),
    /// Pull the previous sibling into the selected list.
    SlurpBackward(EditTargetArgs),
    /// Push the last child out of the selected list.
    BarfForward(EditTargetArgs),
    /// Push the first child out of the selected list.
    BarfBackward(EditTargetArgs),
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
    /// Render a verified diff from a refactor preview manifest without writing files.
    Diff(refactor::args::RefactorDiffArgs),
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
    /// Print a shell completion script to stdout.
    Completions {
        /// Shell to generate a completion script for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}
