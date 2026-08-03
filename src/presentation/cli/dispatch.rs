use super::*;

/// Runs the command the argument layer produced.
///
/// [`CommandResult`] rather than `anyhow::Result<()>`: a command entry point
/// either did its work, could not, or tripped a gate the caller asked for, and
/// those are three different things the exit code has to tell apart. They used
/// to travel as one type-erased `anyhow::Error` that `diagnosis` reflected on
/// with `downcast`.
pub(super) fn dispatch(command: Command) -> CommandResult {
    match command {
        Command::Inspect { command } => match command {
            command::InspectCommand::Check(args) => analysis_report::workflow::check(args)?,
            command::InspectCommand::Dialect(args) => analysis_report::workflow::dialect(args)?,
            command::InspectCommand::Stats(args) => analysis_report::workflow::stats(args)?,
            command::InspectCommand::AgentReport(args) => {
                analysis_report::workflow::agent_report(args)?;
            }
            command::InspectCommand::Capabilities(args) => capabilities::capabilities(args)?,
            command::InspectCommand::Change(args) => change_summary::change_summary(args)?,
            command::InspectCommand::Outline(args) => analysis_report::workflow::outline(args)?,
            command::InspectCommand::Form(args) => form_report::workflow::form_report(args)?,
            command::InspectCommand::Resolve(args) => {
                resolve_report::workflow::resolve_report(args)?;
            }
            command::InspectCommand::FindSymbol(args) => {
                paredit_feature_project_inventory::find_symbol(args)?;
            }
            command::InspectCommand::Symbols(args) => {
                paredit_feature_project_inventory::symbol_report(args)?;
            }
            command::InspectCommand::Calls(args) => call_report::workflow::call_report(args)?,
            command::InspectCommand::Signature(args) => {
                signature_report::workflow::signature_report(args)?;
            }
            command::InspectCommand::CallGraph(args) => {
                call_graph_report::workflow::call_graph(args)?;
            }
            command::InspectCommand::Impact(args) => impact_report::workflow::impact_report(args)?,
            command::InspectCommand::Workspace(args) => {
                workspace_report::workflow::workspace_report(args)?;
            }
            command::InspectCommand::Sources(args) => {
                source_report::workflow::source_report(args)?;
            }
            command::InspectCommand::Dependencies(args) => {
                dependency_report::workflow::dependency_report(args)?;
            }
            command::InspectCommand::Packages(args) => package::report::package_report(args)?,
            command::InspectCommand::Definitions(args) => {
                definition_report::workflow::definition_report(args)?;
            }
            command::InspectCommand::UnusedDefinitions(args) => {
                definition_report::workflow::unused_definition_report(args)?;
            }
            command::InspectCommand::Duplicates(args) => {
                duplicate_report::workflow::duplicate_report(args)?;
            }
            command::InspectCommand::CloneClasses(args) => {
                clone_report::workflow::clone_classes(args)?;
            }
            command::InspectCommand::CloneSequences(args) => {
                clone_report::workflow::clone_sequences(args)?;
            }
            command::InspectCommand::CloneExternal(args) => {
                clone_report::workflow::clone_external(args)?;
            }
            command::InspectCommand::CloneThreshold(args) => {
                clone_report::workflow::clone_threshold(args)?;
            }
            command::InspectCommand::CloneGenealogy(args) => {
                clone_report::workflow::clone_genealogy(args)?;
            }
            command::InspectCommand::Diff(args) => {
                structural_diff::workflow::structural_diff(args)?;
            }
            command::InspectCommand::DuplicateSetfPlaces(args) => {
                duplicate_setf_place_report::workflow::duplicate_setf_place_report(args)?;
            }
            command::InspectCommand::ElispFile(args) => {
                emacs_lisp_file_report::workflow::emacs_lisp_file_report(args)?;
            }
            command::InspectCommand::DuplicateSlots(args) => {
                paredit_feature_lisp_analysis::duplicate_slot_report(args)?;
            }
            command::InspectCommand::DuplicateMethods(args) => {
                paredit_feature_lisp_analysis::duplicate_method_report(args)?;
            }
            command::InspectCommand::DuplicateParameters(args) => {
                duplicate_parameter_report::workflow::duplicate_parameter_report(args)?;
            }
            command::InspectCommand::DuplicateLambdaListKeyword(args) => {
                duplicate_lambda_list_keyword_report::workflow::duplicate_lambda_list_keyword_report(
                    args,
                )?;
            }
            command::InspectCommand::LambdaListKeywordOrder(args) => {
                lambda_list_keyword_order_report::workflow::lambda_list_keyword_order_report(args)?;
            }
            command::InspectCommand::DuplicateCaseKeys(args) => {
                duplicate_case_key_report::workflow::duplicate_case_key_report(args)?;
            }
            command::InspectCommand::QuotedCaseKey(args) => {
                quoted_case_key_report::workflow::quoted_case_key_report(args)?;
            }
            command::InspectCommand::CaseNilKey(args) => {
                case_nil_key_report::workflow::case_nil_key_report(args)?;
            }
            command::InspectCommand::TypecaseNilKey(args) => {
                typecase_nil_key_report::workflow::typecase_nil_key_report(args)?;
            }
            command::InspectCommand::MalformedCaseClause(args) => {
                malformed_case_clause_report::workflow::malformed_case_clause_report(args)?;
            }
            command::InspectCommand::UnreachableCaseClause(args) => {
                unreachable_case_clause_report::workflow::unreachable_case_clause_report(args)?;
            }
            command::InspectCommand::ExhaustiveCaseOtherwise(args) => {
                exhaustive_case_otherwise_report::workflow::exhaustive_case_otherwise_report(args)?;
            }
            command::InspectCommand::ExplicitStepDelta(args) => {
                explicit_step_delta_report::workflow::explicit_step_delta_report(args)?;
            }
            command::InspectCommand::NegatedStepDelta(args) => {
                negated_step_delta_report::workflow::negated_step_delta_report(args)?;
            }
            command::InspectCommand::ExplicitNilReturn(args) => {
                explicit_nil_return_report::workflow::explicit_nil_return_report(args)?;
            }
            command::InspectCommand::DuplicateCondTests(args) => {
                duplicate_cond_test_report::workflow::duplicate_cond_test_report(args)?;
            }
            command::InspectCommand::UnreachableCondClause(args) => {
                unreachable_cond_clause_report::workflow::unreachable_cond_clause_report(args)?;
            }
            command::InspectCommand::MalformedCondClause(args) => {
                malformed_cond_clause_report::workflow::malformed_cond_clause_report(args)?;
            }
            command::InspectCommand::DuplicateLetBindings(args) => {
                duplicate_let_binding_report::workflow::duplicate_let_binding_report(args)?;
            }
            command::InspectCommand::MalformedLetBinding(args) => {
                malformed_let_binding_report::workflow::malformed_let_binding_report(args)?;
            }
            command::InspectCommand::ManualIncf(args) => {
                manual_incf_report::workflow::manual_incf_report(args)?;
            }
            command::InspectCommand::ManualPush(args) => {
                manual_push_report::workflow::manual_push_report(args)?;
            }
            command::InspectCommand::ManualPushnew(args) => {
                manual_pushnew_report::workflow::manual_pushnew_report(args)?;
            }
            command::InspectCommand::BindsConstant(args) => {
                binds_constant_report::workflow::binds_constant_report(args)?;
            }
            command::InspectCommand::MalformedIterationSpec(args) => {
                malformed_iteration_spec_report::workflow::malformed_iteration_spec_report(args)?;
            }
            command::InspectCommand::DuplicateBooleanOperands(args) => {
                duplicate_boolean_operand_report::workflow::duplicate_boolean_operand_report(args)?;
            }
            command::InspectCommand::DeadBooleanOperand(args) => {
                dead_boolean_operand_report::workflow::dead_boolean_operand_report(args)?;
            }
            command::InspectCommand::SelfAssignments(args) => {
                self_assignment_report::workflow::self_assignment_report(args)?;
            }
            command::InspectCommand::SetfArity(args) => {
                setf_arity_report::workflow::setf_arity_report(args)?;
            }
            command::InspectCommand::SetqNonVariable(args) => {
                setq_non_variable_report::workflow::setq_non_variable_report(args)?;
            }
            command::InspectCommand::ModifyMacroArity(args) => {
                modify_macro_arity_report::workflow::modify_macro_arity_report(args)?;
            }
            command::InspectCommand::SelfComparison(args) => {
                self_comparison_report::workflow::self_comparison_report(args)?;
            }
            command::InspectCommand::IdenticalIfBranches(args) => {
                identical_if_branch_report::workflow::identical_if_branch_report(args)?;
            }
            command::InspectCommand::IfArity(args) => {
                if_arity_report::workflow::if_arity_report(args)?;
            }
            command::InspectCommand::TComparison(args) => {
                t_comparison_report::workflow::t_comparison_report(args)?;
            }
            command::InspectCommand::TheArity(args) => {
                the_arity_report::workflow::the_arity_report(args)?;
            }
            command::InspectCommand::EqualityArity(args) => {
                equality_arity_report::workflow::equality_arity_report(args)?;
            }
            command::InspectCommand::AccessorArity(args) => {
                accessor_arity_report::workflow::accessor_arity_report(args)?;
            }
            command::InspectCommand::EvalWhenSituation(args) => {
                eval_when_situation_report::workflow::eval_when_situation_report(args)?;
            }
            command::InspectCommand::EqlStringComparison(args) => {
                eql_string_comparison_report::workflow::eql_string_comparison_report(args)?;
            }
            command::InspectCommand::EqNumberComparison(args) => {
                eq_number_comparison_report::workflow::eq_number_comparison_report(args)?;
            }
            command::InspectCommand::EqCharComparison(args) => {
                eq_char_comparison_report::workflow::eq_char_comparison_report(args)?;
            }
            command::InspectCommand::DestructiveLiteral(args) => {
                destructive_literal_report::workflow::destructive_literal_report(args)?;
            }
            command::InspectCommand::EqlSearchLiteral(args) => {
                eql_search_literal_report::workflow::eql_search_literal_report(args)?;
            }
            command::InspectCommand::CharOpString(args) => {
                char_op_string_report::workflow::char_op_string_report(args)?;
            }
            command::InspectCommand::StringCaseFold(args) => {
                string_case_fold_report::workflow::string_case_fold_report(args)?;
            }
            command::InspectCommand::CharCaseFold(args) => {
                char_case_fold_report::workflow::char_case_fold_report(args)?;
            }
            command::InspectCommand::NestedStringCase(args) => {
                nested_string_case_report::workflow::nested_string_case_report(args)?;
            }
            command::InspectCommand::CodeCharCharCode(args) => {
                code_char_char_code_report::workflow::code_char_char_code_report(args)?;
            }
            command::InspectCommand::LastDefaultCount(args) => {
                last_default_count_report::workflow::last_default_count_report(args)?;
            }
            command::InspectCommand::ButlastDefaultCount(args) => {
                butlast_default_count_report::workflow::butlast_default_count_report(args)?;
            }
            command::InspectCommand::MakeListDefaultElement(args) => {
                make_list_default_element_report::workflow::make_list_default_element_report(args)?;
            }
            command::InspectCommand::ParseIntegerDefaultRadix(args) => {
                parse_integer_default_radix_report::workflow::parse_integer_default_radix_report(
                    args,
                )?;
            }
            command::InspectCommand::GetfDefaultNil(args) => {
                getf_default_nil_report::workflow::getf_default_nil_report(args)?;
            }
            command::InspectCommand::MakeArrayDefaultKeyword(args) => {
                make_array_default_keyword_report::workflow::make_array_default_keyword_report(
                    args,
                )?;
            }
            command::InspectCommand::NestedCharCase(args) => {
                nested_char_case_report::workflow::nested_char_case_report(args)?;
            }
            command::InspectCommand::ListStarNil(args) => {
                list_star_nil_report::workflow::list_star_nil_report(args)?;
            }
            command::InspectCommand::EmptyBody(args) => {
                empty_body_report::workflow::empty_body_report(args)?;
            }
            command::InspectCommand::EmptyLet(args) => {
                empty_let_report::workflow::empty_let_report(args)?;
            }
            command::InspectCommand::IdentityArithmetic(args) => {
                identity_arithmetic_report::workflow::identity_arithmetic_report(args)?;
            }
            command::InspectCommand::RedundantDivisor(args) => {
                redundant_divisor_report::workflow::redundant_divisor_report(args)?;
            }
            command::InspectCommand::EqlListComparison(args) => {
                eql_list_comparison_report::workflow::eql_list_comparison_report(args)?;
            }
            command::InspectCommand::Lint(args) => lint_report::workflow::lint_report(args)?,
            command::InspectCommand::Similarity(args) => {
                similarity_report::workflow::similarity_report(args)?;
            }
            command::InspectCommand::Lets(args) => let_report::let_report(args)?,
            command::InspectCommand::Complexity(args) => {
                complexity_report::workflow::complexity_report(args)?;
            }
            command::InspectCommand::Naming(args) => naming_report::workflow::naming_report(args)?,
            command::InspectCommand::Reachability(args) => {
                reachability_report::workflow::reachability_report(args)?;
            }
            command::InspectCommand::Redefinitions(args) => {
                redefinition_report::workflow::redefinition_report(args)?;
            }
            command::InspectCommand::RedundantQuote(args) => {
                redundant_quote_report::workflow::redundant_quote_report(args)?;
            }
            command::InspectCommand::RedundantProgn(args) => {
                redundant_progn_report::workflow::redundant_progn_report(args)?;
            }
            command::InspectCommand::RedundantProg1(args) => {
                redundant_prog1_report::workflow::redundant_prog1_report(args)?;
            }
            command::InspectCommand::SelfRecursiveTailCall(args) => {
                self_recursive_tail_call_report::workflow::self_recursive_tail_call_report(args)?;
            }
            command::InspectCommand::NegatedWhenUnless(args) => {
                negated_when_unless_report::workflow::negated_when_unless_report(args)?;
            }
            command::InspectCommand::NegatedComparison(args) => {
                negated_comparison_report::workflow::negated_comparison_report(args)?;
            }
            command::InspectCommand::NegatedIf(args) => {
                negated_if_report::workflow::negated_if_report(args)?;
            }
            command::InspectCommand::IfToOr(args) => {
                if_to_or_report::workflow::if_to_or_report(args)?;
            }
            command::InspectCommand::IfNot(args) => {
                if_not_report::workflow::if_not_report(args)?;
            }
            command::InspectCommand::IfToUnless(args) => {
                if_to_unless_report::workflow::if_to_unless_report(args)?;
            }
            command::InspectCommand::Prog2ToProgn(args) => {
                prog2_to_progn_report::workflow::prog2_to_progn_report(args)?;
            }
            command::InspectCommand::HandlerCaseNoClauses(args) => {
                handler_case_no_clauses_report::workflow::handler_case_no_clauses_report(args)?;
            }
            command::InspectCommand::UnwindProtectNoCleanup(args) => {
                unwind_protect_no_cleanup_report::workflow::unwind_protect_no_cleanup_report(args)?;
            }
            command::InspectCommand::OneStepArithmetic(args) => {
                one_step_arithmetic_report::workflow::one_step_arithmetic_report(args)?;
            }
            command::InspectCommand::ConstantIfTest(args) => {
                constant_if_test_report::workflow::constant_if_test_report(args)?;
            }
            command::InspectCommand::ConstantWhenTest(args) => {
                constant_when_test_report::workflow::constant_when_test_report(args)?;
            }
            command::InspectCommand::ConsToList(args) => {
                cons_to_list_report::workflow::cons_to_list_report(args)?;
            }
            command::InspectCommand::DoubleReverse(args) => {
                double_reverse_report::workflow::double_reverse_report(args)?;
            }
            command::InspectCommand::AppendListToCons(args) => {
                append_list_to_cons_report::workflow::append_list_to_cons_report(args)?;
            }
            command::InspectCommand::ListStarToCons(args) => {
                list_star_to_cons_report::workflow::list_star_to_cons_report(args)?;
            }
            command::InspectCommand::ValuesListOfList(args) => {
                values_list_of_list_report::workflow::values_list_of_list_report(args)?;
            }
            command::InspectCommand::MultipleValueListOfValues(args) => {
                multiple_value_list_of_values_report::workflow::multiple_value_list_of_values_report(
                    args,
                )?;
            }
            command::InspectCommand::AppendNil(args) => {
                append_nil_report::workflow::append_nil_report(args)?;
            }
            command::InspectCommand::VerboseNegation(args) => {
                verbose_negation_report::workflow::verbose_negation_report(args)?;
            }
            command::InspectCommand::NestedBoolean(args) => {
                nested_boolean_report::workflow::nested_boolean_report(args)?;
            }
            command::InspectCommand::NestedCxr(args) => {
                nested_cxr_report::workflow::nested_cxr_report(args)?;
            }
            command::InspectCommand::PackageLevelShadowing(args) => {
                package_level_shadowing_report::workflow::package_level_shadowing_report(args)?;
            }
            command::InspectCommand::NthcdrZero(args) => {
                nthcdr_zero_report::workflow::nthcdr_zero_report(args)?;
            }
            command::InspectCommand::SubseqZero(args) => {
                subseq_zero_report::workflow::subseq_zero_report(args)?;
            }
            command::InspectCommand::CarNthcdr(args) => {
                car_nthcdr_report::workflow::car_nthcdr_report(args)?;
            }
            command::InspectCommand::CarReverse(args) => {
                car_reverse_report::workflow::car_reverse_report(args)?;
            }
            command::InspectCommand::NthcdrSmallIndex(args) => {
                nthcdr_small_index_report::workflow::nthcdr_small_index_report(args)?;
            }
            command::InspectCommand::NthConstantIndex(args) => {
                nth_constant_index_report::workflow::nth_constant_index_report(args)?;
            }
            command::InspectCommand::NestedProgn(args) => {
                nested_progn_report::workflow::nested_progn_report(args)?;
            }
            command::InspectCommand::NestedUnless(args) => {
                nested_unless_report::workflow::nested_unless_report(args)?;
            }
            command::InspectCommand::NestedWhen(args) => {
                nested_when_report::workflow::nested_when_report(args)?;
            }
            command::InspectCommand::NilComparison(args) => {
                nil_comparison_report::workflow::nil_comparison_report(args)?;
            }
            command::InspectCommand::OneArmedIf(args) => {
                one_armed_if_report::workflow::one_armed_if_report(args)?;
            }
            command::InspectCommand::RedundantApply(args) => {
                redundant_apply_report::workflow::redundant_apply_report(args)?;
            }
            command::InspectCommand::RedundantEqlTest(args) => {
                redundant_eql_test_report::workflow::redundant_eql_test_report(args)?;
            }
            command::InspectCommand::RedundantStartZero(args) => {
                redundant_start_zero_report::workflow::redundant_start_zero_report(args)?;
            }
            command::InspectCommand::RedundantEndNil(args) => {
                redundant_end_nil_report::workflow::redundant_end_nil_report(args)?;
            }
            command::InspectCommand::RedundantFromEndNil(args) => {
                redundant_from_end_nil_report::workflow::redundant_from_end_nil_report(args)?;
            }
            command::InspectCommand::RedundantCountNil(args) => {
                redundant_count_nil_report::workflow::redundant_count_nil_report(args)?;
            }
            command::InspectCommand::MakeHashTableTest(args) => {
                make_hash_table_test_report::workflow::make_hash_table_test_report(args)?;
            }
            command::InspectCommand::GethashDefault(args) => {
                gethash_default_report::workflow::gethash_default_report(args)?;
            }
            command::InspectCommand::GiantConditionalForm(args) => {
                giant_conditional_form_report::workflow::giant_conditional_form_report(args)?;
            }
            command::InspectCommand::TypepPredicate(args) => {
                typep_predicate_report::workflow::typep_predicate_report(args)?;
            }
            command::InspectCommand::CoerceToT(args) => {
                coerce_to_t_report::workflow::coerce_to_t_report(args)?;
            }
            command::InspectCommand::RedundantIdentityKey(args) => {
                redundant_identity_key_report::workflow::redundant_identity_key_report(args)?;
            }
            command::InspectCommand::RedundantBodyProgn(args) => {
                redundant_body_progn_report::workflow::redundant_body_progn_report(args)?;
            }
            command::InspectCommand::RedundantBooleanIdentity(args) => {
                redundant_boolean_identity_report::workflow::redundant_boolean_identity_report(
                    args,
                )?;
            }
            command::InspectCommand::DeMorgan(args) => {
                de_morgan_report::workflow::de_morgan_report(args)?;
            }
            command::InspectCommand::RedundantIdentity(args) => {
                redundant_identity_report::workflow::redundant_identity_report(args)?;
            }
            command::InspectCommand::RedundantIfNil(args) => {
                redundant_if_nil_report::workflow::redundant_if_nil_report(args)?;
            }
            command::InspectCommand::RedundantLetStar(args) => {
                redundant_let_star_report::workflow::redundant_let_star_report(args)?;
            }
            command::InspectCommand::RedundantFuncall(args) => {
                redundant_funcall_report::workflow::redundant_funcall_report(args)?;
            }
            command::InspectCommand::RedundantThe(args) => {
                redundant_the_report::workflow::redundant_the_report(args)?;
            }
            command::InspectCommand::FuncallLambda(args) => {
                funcall_lambda_report::workflow::funcall_lambda_report(args)?;
            }
            command::InspectCommand::SharpQuotedLambda(args) => {
                sharp_quoted_lambda_report::workflow::sharp_quoted_lambda_report(args)?;
            }
            command::InspectCommand::SingleOperandBoolean(args) => {
                single_operand_boolean_report::workflow::single_operand_boolean_report(args)?;
            }
            command::InspectCommand::SingleOperandListOp(args) => {
                single_operand_list_op_report::workflow::single_operand_list_op_report(args)?;
            }
            command::InspectCommand::SingleOperandArithmetic(args) => {
                single_operand_arithmetic_report::workflow::single_operand_arithmetic_report(args)?;
            }
            command::InspectCommand::SingleArgComparison(args) => {
                single_arg_comparison_report::workflow::single_arg_comparison_report(args)?;
            }
            command::InspectCommand::SingleClauseCond(args) => {
                single_clause_cond_report::workflow::single_clause_cond_report(args)?;
            }
            command::InspectCommand::CondTClause(args) => {
                cond_t_clause_report::workflow::cond_t_clause_report(args)?;
            }
            command::InspectCommand::SingleValueBind(args) => {
                single_value_bind_report::workflow::single_value_bind_report(args)?;
            }
            command::InspectCommand::SignComparison(args) => {
                sign_comparison_report::workflow::sign_comparison_report(args)?;
            }
            command::InspectCommand::FormatMissingDestination(args) => {
                format_missing_destination_report::workflow::format_missing_destination_report(
                    args,
                )?;
            }
            command::InspectCommand::FormatToString(args) => {
                format_to_string_report::workflow::format_to_string_report(args)?;
            }
            command::InspectCommand::FormatNewline(args) => {
                format_newline_report::workflow::format_newline_report(args)?;
            }
            command::InspectCommand::LiteralPlace(args) => {
                literal_place_report::workflow::literal_place_report(args)?;
            }
            command::InspectCommand::ZeroDivisor(args) => {
                zero_divisor_report::workflow::zero_divisor_report(args)?;
            }
            command::InspectCommand::DuplicateKeyword(args) => {
                duplicate_keyword_report::workflow::duplicate_keyword_report(args)?;
            }
            command::InspectCommand::DefpackageQuoted(args) => {
                defpackage_quoted_report::workflow::defpackage_quoted_report(args)?;
            }
            command::InspectCommand::StepZero(args) => {
                step_zero_report::workflow::step_zero_report(args)?;
            }
            command::InspectCommand::UnusedParameters(args) => {
                paredit_feature_function_parameter::unused_parameter_report(args)?;
            }
            command::InspectCommand::ShadowedBindings(args) => {
                paredit_feature_binding::shadowed_binding_report(args)?;
            }
            command::InspectCommand::UnusedLocalCallables(args) => {
                unused_local_callable_report::workflow::unused_local_callable_report(args)?;
            }
            command::InspectCommand::PackageBoundaries(args) => {
                package_boundary_report::workflow::package_boundary_report(args)?;
            }
            command::InspectCommand::CallCycles(args) => {
                call_cycle_report::workflow::call_cycle_report(args)?;
            }
            command::InspectCommand::PackageCycles(args) => {
                package_cycle_report::workflow::package_cycle_report(args)?;
            }
            command::InspectCommand::PackageConflicts(args) => {
                package_conflict_report::workflow::package_conflict_report(args)?;
            }
            command::InspectCommand::SystemConflicts(args) => {
                system_conflict_report::workflow::system_conflict_report(args)?;
            }
            command::InspectCommand::SystemCycles(args) => {
                system_cycle_report::workflow::system_cycle_report(args)?;
            }
            command::InspectCommand::ClassCycles(args) => {
                class_cycle_report::workflow::class_cycle_report(args)?;
            }
            command::InspectCommand::StructCycles(args) => {
                struct_cycle_report::workflow::struct_cycle_report(args)?;
            }
            command::InspectCommand::UnusedPackages(args) => {
                unused_package_report::workflow::unused_package_report(args)?;
            }
            command::InspectCommand::UnusedExports(args) => {
                unused_export_report::workflow::unused_export_report(args)?;
            }
            command::InspectCommand::DuplicateExports(args) => {
                paredit_feature_package::duplicate_export_report(args)?;
            }
            command::InspectCommand::UnusedNicknames(args) => {
                unused_nickname_report::workflow::unused_nickname_report(args)?;
            }
            command::InspectCommand::UseWidening(args) => {
                use_widening_report::workflow::use_widening_report(args)?;
            }
            command::InspectCommand::UndefinedPackages(args) => {
                undefined_package_report::workflow::undefined_package_report(args)?;
            }
            command::InspectCommand::ApiSurface(args) => {
                api_surface_report::workflow::api_surface_report(args)?;
            }
            command::InspectCommand::ApiDiff(args) => {
                api_diff_report::workflow::api_diff_report(args)?;
            }
            command::InspectCommand::TestMap(args) => {
                test_map_report::workflow::test_map_report(args)?;
            }
            command::InspectCommand::SymbolIndex(args) => {
                symbol_index_report::workflow::symbol_index_report(args)?;
            }
            command::InspectCommand::KeywordArity(args) => {
                keyword_arity_report::workflow::keyword_arity_report(args)?;
            }
            command::InspectCommand::UnreachableExpressions(args) => {
                unreachable_expression_report::workflow::unreachable_expression_report(args)?;
            }
            command::InspectCommand::ExternalSystems(args) => {
                external_system_report::workflow::external_system_report(args)?;
            }
            command::InspectCommand::Licenses(args) => {
                license_report::workflow::license_report(args)?;
            }
            command::InspectCommand::LicenseHeaders(args) => {
                license_header_report::workflow::license_header_report(args)?;
            }
            command::InspectCommand::SerialConsistency(args) => {
                serial_consistency_report::workflow::serial_consistency_report(args)?;
            }
            command::InspectCommand::Blame(args) => {
                blame_report::workflow::blame_report(args)?;
            }
            command::InspectCommand::DuplicationRatio(args) => {
                duplication_ratio_report::workflow::duplication_ratio_report(args)?;
            }
            command::InspectCommand::Cohesion(args) => {
                cohesion_report::workflow::cohesion_report(args)?;
            }
            command::InspectCommand::Hotspots(args) => {
                hotspot_report::workflow::hotspot_report(args)?;
            }
            command::InspectCommand::DebtScore(args) => {
                debt_score_report::workflow::debt_score_report(args)?;
            }
            command::InspectCommand::Indentation(args) => {
                indentation_report::workflow::indentation_report(args)?;
            }
            command::InspectCommand::Docstrings(args) => {
                docstring_report::workflow::docstring_report(args)?;
            }
            command::InspectCommand::Todo(args) => {
                todo_report::workflow::todo_report(args)?;
            }
            command::InspectCommand::LineMetrics(args) => {
                line_metrics_report::workflow::line_metrics_report(args)?;
            }
            command::InspectCommand::MacroExpansion(args) => {
                macro_expansion_report::workflow::macro_expansion_report(args)?;
            }
            command::InspectCommand::MacroHygiene(args) => {
                macro_hygiene_report::workflow::macro_hygiene_report(args)?;
            }
            command::InspectCommand::Loop(args) => {
                loop_report::workflow::loop_report(args)?;
            }
            command::InspectCommand::FormatDirectives(args) => {
                format_directive_report::workflow::format_directive_report(args)?;
            }
            command::InspectCommand::ReadConditionals(args) => {
                read_conditional_report::workflow::read_conditional_report(args)?;
            }
            command::InspectCommand::ReadTimeEval(args) => {
                read_time_eval_report::workflow::read_time_eval_report(args)?;
            }
            command::InspectCommand::CircularLiterals(args) => {
                circular_literal_report::workflow::circular_literal_report(args)?;
            }
            command::InspectCommand::ReadtableCase(args) => {
                readtable_case_report::workflow::readtable_case_report(args)?;
            }
            command::InspectCommand::PackageLocks(args) => {
                package_lock_report::workflow::package_lock_report(args)?;
            }
            command::InspectCommand::MethodCombination(args) => {
                method_combination_report::workflow::method_combination_report(args)?;
            }
            command::InspectCommand::ClassHierarchy(args) => {
                class_hierarchy_report::workflow::class_hierarchy_report(args)?;
            }
            command::InspectCommand::GenericDispatch(args) => {
                generic_dispatch_report::workflow::generic_dispatch_report(args)?;
            }
            command::InspectCommand::Restarts(args) => {
                restart_report::workflow::restart_report(args)?;
            }
            command::InspectCommand::ExternalDiagnostics(args) => {
                external_diagnostics_report::workflow::external_diagnostics_report(args)?;
            }
            command::InspectCommand::Types(args) => type_report::workflow::type_report(args)?,
            command::InspectCommand::Narrowing(args) => {
                narrowing_report::workflow::narrowing_report(args)?;
            }
            command::InspectCommand::Constants(args) => {
                constant_report::workflow::constant_report(args)?;
            }
            command::InspectCommand::MagicNumbers(args) => {
                magic_number_report::workflow::magic_number_report(args)?;
            }
            command::InspectCommand::ValuePropagation(args) => {
                value_propagation_report::workflow::value_propagation_report(args)?;
            }
            command::InspectCommand::Effects(args) => {
                effect_report::workflow::effect_report(args)?;
            }
            command::InspectCommand::SemanticCoverage(args) => {
                semantic_coverage_report::workflow::semantic_coverage_report(args)?;
            }
            command::InspectCommand::ContextAt(args) => {
                context_report::workflow::context_at_report(args)?;
            }
            command::InspectCommand::Writability(args) => {
                writability_report::workflow::writability(args)?;
            }
            command::InspectCommand::DataCheck(args) => {
                data_check_report::workflow::data_check_report(args)?;
            }
            command::InspectCommand::KillRing(args) => {
                kill_ring_report::workflow::kill_ring(args)?;
            }
            command::InspectCommand::LeftoverPrintDebug(args) => {
                leftover_print_debug_report::workflow::leftover_print_debug_report(args)?;
            }
            command::InspectCommand::LeftoverTraceCall(args) => {
                leftover_trace_call_report::workflow::leftover_trace_call_report(args)?;
            }
            command::InspectCommand::LeftoverBreakCall(args) => {
                leftover_break_call_report::workflow::leftover_break_call_report(args)?;
            }
            command::InspectCommand::LeftoverInspectCall(args) => {
                leftover_inspect_call_report::workflow::leftover_inspect_call_report(args)?;
            }
            command::InspectCommand::LeftoverTimeBenchmarkCall(args) => {
                leftover_time_benchmark_call_report::workflow::leftover_time_benchmark_call_report(
                    args,
                )?;
            }
            command::InspectCommand::LeftoverStepCall(args) => {
                leftover_step_call_report::workflow::leftover_step_call_report(args)?;
            }
            command::InspectCommand::CommentedReplTranscript(args) => {
                commented_repl_transcript_report::workflow::commented_repl_transcript_report(args)?;
            }
            command::InspectCommand::LeftoverFormatDebugMarker(args) => {
                leftover_format_debug_marker_report::workflow::leftover_format_debug_marker_report(
                    args,
                )?;
            }
            command::InspectCommand::AroundMethodMissingCallNextMethod(args) => {
                around_method_missing_call_next_method_report::workflow::around_method_missing_call_next_method_report(args)?;
            }
            command::InspectCommand::DefclassRequiredSlotNoInitformOrInitarg(args) => {
                defclass_required_slot_no_initform_or_initarg_report::workflow::defclass_required_slot_no_initform_or_initarg_report(args)?;
            }
            command::InspectCommand::DefclassSlotShadowing(args) => {
                defclass_slot_shadowing_report::workflow::defclass_slot_shadowing_report(args)?;
            }
            command::InspectCommand::DuplicateDefmethodSignature(args) => {
                duplicate_defmethod_signature_report::workflow::duplicate_defmethod_signature_report(args)?;
            }
            command::InspectCommand::GenericFunctionNoMethods(args) => {
                generic_function_no_methods_report::workflow::generic_function_no_methods_report(
                    args,
                )?;
            }
            command::InspectCommand::MethodQualifierTypo(args) => {
                method_qualifier_typo_report::workflow::method_qualifier_typo_report(args)?;
            }
            command::InspectCommand::PrintObjectWithoutPrintUnreadableObject(args) => {
                print_object_without_print_unreadable_object_report::workflow::print_object_without_print_unreadable_object_report(args)?;
            }
            command::InspectCommand::SlotValueBypassesAccessor(args) => {
                slot_value_bypasses_accessor_report::workflow::slot_value_bypasses_accessor_report(
                    args,
                )?;
            }
            command::InspectCommand::CerrorMissingContinueFormat(args) => {
                cerror_missing_continue_format_report::workflow::cerror_missing_continue_format_report(args)?;
            }
            command::InspectCommand::DefineConditionEmptySuperclassList(args) => {
                define_condition_empty_superclass_list_report::workflow::define_condition_empty_superclass_list_report(args)?;
            }
            command::InspectCommand::DefineConditionMissingReportForErrorType(args) => {
                define_condition_missing_report_for_error_type_report::workflow::define_condition_missing_report_for_error_type_report(args)?;
            }
            command::InspectCommand::HandlerBindHandlerReturnsBareValue(args) => {
                handler_bind_handler_returns_bare_value_report::workflow::handler_bind_handler_returns_bare_value_report(args)?;
            }
            command::InspectCommand::IgnoreErrorsWrapsNonErrorSignal(args) => {
                ignore_errors_wraps_non_error_signal_report::workflow::ignore_errors_wraps_non_error_signal_report(args)?;
            }
            command::InspectCommand::RestartCaseClauseWithoutReport(args) => {
                restart_case_clause_without_report_report::workflow::restart_case_clause_without_report_report(args)?;
            }
            command::InspectCommand::SignalOnErrorConditionReturnsSilently(args) => {
                signal_on_error_condition_returns_silently_report::workflow::signal_on_error_condition_returns_silently_report(args)?;
            }
            command::InspectCommand::DolistResultFormReferencesLoopVariable(args) => {
                dolist_result_form_references_loop_variable_report::workflow::dolist_result_form_references_loop_variable_report(args)?;
            }
            command::InspectCommand::DotimesBoundMutationHasNoEffect(args) => {
                dotimes_bound_mutation_has_no_effect_report::workflow::dotimes_bound_mutation_has_no_effect_report(args)?;
            }
            command::InspectCommand::LoopClauseOrderViolation(args) => {
                loop_clause_order_violation_report::workflow::loop_clause_order_violation_report(
                    args,
                )?;
            }
            command::InspectCommand::LoopForAcrossStaticallyKnownList(args) => {
                loop_for_across_statically_known_list_report::workflow::loop_for_across_statically_known_list_report(args)?;
            }
            command::InspectCommand::LoopIntoAccumulatorKindConflict(args) => {
                loop_into_accumulator_kind_conflict_report::workflow::loop_into_accumulator_kind_conflict_report(args)?;
            }
            command::InspectCommand::LoopUnreachableFinallyClause(args) => {
                loop_unreachable_finally_clause_report::workflow::loop_unreachable_finally_clause_report(args)?;
            }
            command::InspectCommand::DisabledTestLeftIn(args) => {
                disabled_test_left_in_report::workflow::disabled_test_left_in_report(args)?;
            }
            command::InspectCommand::DuplicateTestName(args) => {
                duplicate_test_name_report::workflow::duplicate_test_name_report(args)?;
            }
            command::InspectCommand::EmptyTestBody(args) => {
                empty_test_body_report::workflow::empty_test_body_report(args)?;
            }
            command::InspectCommand::SleepInTest(args) => {
                sleep_in_test_report::workflow::sleep_in_test_report(args)?;
            }
            command::InspectCommand::TestAssertsConstant(args) => {
                test_asserts_constant_report::workflow::test_asserts_constant_report(args)?;
            }
            command::InspectCommand::TestWithoutAssertion(args) => {
                test_without_assertion_report::workflow::test_without_assertion_report(args)?;
            }
            command::InspectCommand::AtomSwapWithSideEffect(args) => {
                atom_swap_with_side_effect_report::workflow::atom_swap_with_side_effect_report(
                    args,
                )?;
            }
            command::InspectCommand::DynamicVarBoundAcrossThreadBoundary(args) => {
                dynamic_var_bound_across_thread_boundary_report::workflow::dynamic_var_bound_across_thread_boundary_report(args)?;
            }
            command::InspectCommand::FuturePromiseNeverRealized(args) => {
                future_promise_never_realized_report::workflow::future_promise_never_realized_report(args)?;
            }
            command::InspectCommand::LockAcquiredNotReleased(args) => {
                lock_acquired_not_released_report::workflow::lock_acquired_not_released_report(
                    args,
                )?;
            }
            command::InspectCommand::RecursiveLockReentryRisk(args) => {
                recursive_lock_reentry_risk_report::workflow::recursive_lock_reentry_risk_report(
                    args,
                )?;
            }
            command::InspectCommand::ThreadSpawnedWithoutErrorHandler(args) => {
                thread_spawned_without_error_handler_report::workflow::thread_spawned_without_error_handler_report(args)?;
            }
            command::InspectCommand::UnsynchronizedSharedMutation(args) => {
                unsynchronized_shared_mutation_report::workflow::unsynchronized_shared_mutation_report(args)?;
            }
            command::InspectCommand::AsdfPerformWithoutCallNextMethod(args) => {
                asdf_perform_without_call_next_method_report::workflow::asdf_perform_without_call_next_method_report(args)?;
            }
            command::InspectCommand::AsdfSelfReferentialDependsOn(args) => {
                asdf_self_referential_depends_on_report::workflow::asdf_self_referential_depends_on_report(args)?;
            }
            command::InspectCommand::AsdfSystemMissingVersion(args) => {
                asdf_system_missing_version_report::workflow::asdf_system_missing_version_report(
                    args,
                )?;
            }
            command::InspectCommand::DefpackageWithoutInPackage(args) => {
                defpackage_without_in_package_report::workflow::defpackage_without_in_package_report(args)?;
            }
            command::InspectCommand::BlockNameShadowsOuterBlock(args) => {
                block_name_shadows_outer_block_report::workflow::block_name_shadows_outer_block_report(args)?;
            }
            command::InspectCommand::DotimesDolistIndexVarMutated(args) => {
                dotimes_dolist_index_var_mutated_report::workflow::dotimes_dolist_index_var_mutated_report(args)?;
            }
            command::InspectCommand::GoToUndefinedTag(args) => {
                go_to_undefined_tag_report::workflow::go_to_undefined_tag_report(args)?;
            }
            command::InspectCommand::MultipleValueBindAllIgnored(args) => {
                multiple_value_bind_all_ignored_report::workflow::multiple_value_bind_all_ignored_report(args)?;
            }
            command::InspectCommand::ReturnFromUnmatchedBlock(args) => {
                return_from_unmatched_block_report::workflow::return_from_unmatched_block_report(
                    args,
                )?;
            }
            command::InspectCommand::ReturnOutsideImplicitNilBlock(args) => {
                return_outside_implicit_nil_block_report::workflow::return_outside_implicit_nil_block_report(args)?;
            }
            command::InspectCommand::TagbodyUnreachableTag(args) => {
                tagbody_unreachable_tag_report::workflow::tagbody_unreachable_tag_report(args)?;
            }
            command::InspectCommand::CaseKeyEqlPitfall(args) => {
                case_key_eql_pitfall_report::workflow::case_key_eql_pitfall_report(args)?;
            }
            command::InspectCommand::CondToCaseCandidate(args) => {
                cond_to_case_candidate_report::workflow::cond_to_case_candidate_report(args)?;
            }
            command::InspectCommand::NestedCondFlattenable(args) => {
                nested_cond_flattenable_report::workflow::nested_cond_flattenable_report(args)?;
            }
            command::InspectCommand::WhenUnlessImplicitNilMisused(args) => {
                when_unless_implicit_nil_misused_report::workflow::when_unless_implicit_nil_misused_report(args)?;
            }
            command::InspectCommand::DeeplyNestedAnonymousLambda(args) => {
                deeply_nested_anonymous_lambda_report::workflow::deeply_nested_anonymous_lambda_report(args)?;
            }
            command::InspectCommand::NestedFunctionParameterShadowsEnclosingParameter(args) => {
                nested_function_parameter_shadows_enclosing_parameter_report::workflow::nested_parameter_shadow_report(args)?;
            }
            command::InspectCommand::OverlyLongParameterList(args) => {
                overly_long_parameter_list_report::workflow::overly_long_parameter_list_report(
                    args,
                )?;
            }
            command::InspectCommand::PositionalArgumentCountExceedsReadability(args) => {
                positional_argument_count_exceeds_readability_report::workflow::positional_argument_count_report(args)?;
            }
            command::InspectCommand::StringlyTypedDispatch(args) => {
                stringly_typed_dispatch_report::workflow::stringly_typed_dispatch_report(args)?;
            }
            command::InspectCommand::InternDynamicPackageTarget(args) => {
                intern_dynamic_package_target_report::workflow::intern_dynamic_package_target_report(args)?;
            }
            command::InspectCommand::IntrospectionProbeUnchecked(args) => {
                introspection_probe_unchecked_report::workflow::introspection_probe_unchecked_report(args)?;
            }
            command::InspectCommand::SymbolFunctionFsetDynamicName(args) => {
                symbol_function_fset_dynamic_name_report::workflow::symbol_function_fset_dynamic_name_report(args)?;
            }
            command::InspectCommand::DestructuringBindUnusedWhole(args) => {
                destructuring_bind_unused_whole_report::workflow::destructuring_bind_unused_whole_report(args)?;
            }
            command::InspectCommand::LoopCollectIntoImmediatelyReturned(args) => {
                loop_collect_into_immediately_returned_report::workflow::loop_collect_into_immediately_returned_report(args)?;
            }
            command::InspectCommand::FletSingleUseInlinable(args) => {
                flet_single_use_inlinable_report::workflow::flet_single_use_inlinable_report(args)?;
            }
            command::InspectCommand::MultipleValueSetqArityMismatch(args) => {
                multiple_value_setq_arity_mismatch_report::workflow::multiple_value_setq_arity_mismatch_report(args)?;
            }
            command::InspectCommand::WithOpenFileRedundantDirectionDefault(args) => {
                with_open_file_redundant_direction_default_report::workflow::with_open_file_redundant_direction_default_report(args)?;
            }
            command::InspectCommand::FtypeValuesArityMismatch(args) => {
                ftype_values_arity_mismatch_report::workflow::ftype_values_arity_mismatch_report(
                    args,
                )?;
            }
            command::InspectCommand::WithAccessorsEmptyBindingList(args) => {
                with_accessors_empty_binding_list_report::workflow::with_accessors_empty_binding_list_report(args)?;
            }
            command::InspectCommand::QuotedFormContainsStrayUnquote(args) => {
                quoted_form_contains_stray_unquote_report::workflow::quoted_form_contains_stray_unquote_report(args)?;
            }
            command::InspectCommand::HashTableIterationOrderAssumed(args) => {
                hash_table_iteration_order_assumed_report::workflow::hash_table_iteration_order_assumed_report(args)?;
            }
            command::InspectCommand::SetMembershipViaLinearScan(args) => {
                set_membership_via_linear_scan_report::workflow::set_membership_via_linear_scan_report(args)?;
            }
            command::InspectCommand::NestedGetChain(args) => {
                nested_get_chain_report::workflow::nested_get_chain_report(args)?;
            }
            command::InspectCommand::RedundantIntoEmptyCollection(args) => {
                redundant_into_empty_collection_report::workflow::redundant_into_empty_collection_report(args)?;
            }
            command::InspectCommand::MixedFloatPrecisionArithmetic(args) => {
                mixed_float_precision_arithmetic_report::workflow::mixed_float_precision_arithmetic_report(args)?;
            }
            command::InspectCommand::DivisionResultPrecisionLoss(args) => {
                division_result_precision_loss_report::workflow::division_result_precision_loss_report(args)?;
            }
            command::InspectCommand::EpsilonLessFloatLoopBound(args) => {
                epsilon_less_float_loop_bound_report::workflow::epsilon_less_float_loop_bound_report(args)?;
            }
            command::InspectCommand::RedundantPrecisionCoercion(args) => {
                redundant_precision_coercion_report::workflow::redundant_precision_coercion_report(
                    args,
                )?;
            }
            command::InspectCommand::FormatUnknownDirective(args) => {
                format_unknown_directive_report::workflow::format_unknown_directive_report(args)?;
            }
            command::InspectCommand::FormatPercentAmpersandAdjacentRedundancy(args) => {
                format_percent_ampersand_adjacent_redundancy_report::workflow::format_percent_ampersand_adjacent_redundancy_report(args)?;
            }
            command::InspectCommand::FormatNestedDirectiveUnbalanced(args) => {
                format_nested_directive_unbalanced_report::workflow::format_nested_directive_unbalanced_report(args)?;
            }
            command::InspectCommand::PackageCircularInPackageChain(args) => {
                package_circular_in_package_chain_report::workflow::circular_in_package_chain_report(args)?;
            }
            command::InspectCommand::WithOpenReturnsLazySeq(args) => {
                with_open_returns_lazy_seq_report::workflow::with_open_returns_lazy_seq_report(
                    args,
                )?;
            }
            command::InspectCommand::DefInsideFunctionBody(args) => {
                def_inside_function_body_report::workflow::def_inside_function_body_report(args)?;
            }
            command::InspectCommand::SingleKeyNestedPath(args) => {
                single_key_nested_path_report::workflow::single_key_nested_path_report(args)?;
            }
            command::InspectCommand::ApplyWithLiteralCollection(args) => {
                apply_with_literal_collection_report::workflow::apply_with_literal_collection_report(args)?;
            }
            command::InspectCommand::SchemeBeginSingleForm(args) => {
                scheme_begin_single_form_report::workflow::begin_single_form_report(args)?;
            }
            command::InspectCommand::SchemeLetStarIndependentBindings(args) => {
                scheme_let_star_independent_bindings_report::workflow::let_star_independent_bindings_report(args)?;
            }
            command::InspectCommand::SchemeMemqAssqLiteralKey(args) => {
                scheme_memq_assq_literal_key_report::workflow::memq_assq_literal_key_report(args)?;
            }
            command::InspectCommand::SchemeNamedLetNeverRecurs(args) => {
                scheme_named_let_never_recurs_report::workflow::named_let_never_recurs_report(
                    args,
                )?;
            }
            command::InspectCommand::EvalWhenExecuteOnly(args) => {
                eval_when_execute_only_report::workflow::eval_when_execute_only_report(args)?;
            }
            command::InspectCommand::EvalWhenBodyNeverRuns(args) => {
                eval_when_body_never_runs_report::workflow::eval_when_body_never_runs_report(args)?;
            }
            command::InspectCommand::DefconstantNonEqlValue(args) => {
                defconstant_non_eql_value_report::workflow::defconstant_non_eql_value_report(args)?;
            }
        },
        Command::Edit { command } => match command {
            command::EditCommand::Format(args) => basic_edit::workflow::format(args)?,
            command::EditCommand::RepairUnclosedLists(args) => {
                basic_edit::workflow::repair_unclosed_lists(args)?;
            }
            command::EditCommand::Canonicalize(args) => {
                basic_edit::workflow::canonicalize(args)?;
            }
            command::EditCommand::Select(args) => basic_edit::workflow::select(args)?,
            command::EditCommand::Replace(args) => basic_edit::workflow::replace(args)?,
            command::EditCommand::Kill(args) => basic_edit::workflow::kill(args)?,
            command::EditCommand::Copy(args) => basic_edit::workflow::copy(args)?,
            command::EditCommand::Duplicate(args) => basic_edit::workflow::duplicate(args)?,
            command::EditCommand::NormalizeQuotes(args) => {
                basic_edit::workflow::normalize_quotes(args)?;
            }
            command::EditCommand::Yank(args) => basic_edit::workflow::yank(args)?,
            command::EditCommand::Wrap(args) => basic_edit::workflow::wrap(args)?,
            command::EditCommand::UnwrapPrefix(args) => {
                basic_edit::workflow::unwrap_prefix(args)?;
            }
            command::EditCommand::Splice(args) => basic_edit::workflow::splice(args)?,
            command::EditCommand::Split(args) => basic_edit::workflow::split(args)?,
            command::EditCommand::Join(args) => basic_edit::workflow::join(args)?,
            command::EditCommand::SpliceKillingBackward(args) => {
                basic_edit::workflow::splice_killing_backward(args)?;
            }
            command::EditCommand::SpliceKillingForward(args) => {
                basic_edit::workflow::splice_killing_forward(args)?;
            }
            command::EditCommand::Convolute(args) => basic_edit::workflow::convolute(args)?,
            command::EditCommand::Raise(args) => basic_edit::workflow::raise(args)?,
            command::EditCommand::TransposeForward(args) => {
                basic_edit::workflow::transpose_forward(args)?;
            }
            command::EditCommand::TransposeBackward(args) => {
                basic_edit::workflow::transpose_backward(args)?;
            }
            command::EditCommand::Transpose(args) => basic_edit::workflow::transpose(args)?,
            command::EditCommand::SlurpForward(args) => basic_edit::workflow::slurp_forward(args)?,
            command::EditCommand::SlurpBackward(args) => {
                basic_edit::workflow::slurp_backward(args)?;
            }
            command::EditCommand::BarfForward(args) => basic_edit::workflow::barf_forward(args)?,
            command::EditCommand::BarfBackward(args) => basic_edit::workflow::barf_backward(args)?,
            command::EditCommand::Navigate(args) => basic_edit::workflow::navigate(args)?,
            command::EditCommand::DeleteForward(args) => {
                basic_edit::workflow::delete_forward(args)?;
            }
            command::EditCommand::DeleteBackward(args) => {
                basic_edit::workflow::delete_backward(args)?;
            }
            command::EditCommand::Newline(args) => basic_edit::workflow::newline(args)?,
            command::EditCommand::ReindentDefun(args) => {
                basic_edit::workflow::reindent_defun(args)?;
            }
            command::EditCommand::SplitString(args) => basic_edit::workflow::split_string(args)?,
            command::EditCommand::EscapeString(args) => {
                basic_edit::workflow::escape_string(args)?;
            }
            command::EditCommand::UnescapeString(args) => {
                basic_edit::workflow::unescape_string(args)?;
            }
        },
        Command::Refactor { command } => match command {
            command::RefactorCommand::Plan(args) => refactor::workflow::refactor_plan(args)?,
            command::RefactorCommand::Verify(args) => refactor::workflow::verify_refactor(args)?,
            command::RefactorCommand::Preview(args) => refactor::workflow::refactor_preview(args)?,
            command::RefactorCommand::Check(args) => refactor::workflow::refactor_check(args)?,
            command::RefactorCommand::Status(args) => refactor::workflow::refactor_status(args)?,
            command::RefactorCommand::Apply(args) => refactor::workflow::refactor_apply(args)?,
            command::RefactorCommand::Undo(args) => refactor::workflow::refactor_undo(args)?,
            command::RefactorCommand::Diff(args) => refactor::workflow::refactor_diff(args)?,
            command::RefactorCommand::Patch(args) => {
                structural_patch::workflow::structural_patch(args)?;
            }
            command::RefactorCommand::Step(args) => {
                refactor_step::workflow::refactor_step(args)?;
            }
            command::RefactorCommand::CreateCheckpoint(args) => {
                refactor_checkpoint::workflow::create_checkpoint(args)?;
            }
            command::RefactorCommand::ListCheckpoints(args) => {
                refactor_checkpoint::workflow::list_checkpoints(args)?;
            }
            command::RefactorCommand::RestoreCheckpoint(args) => {
                refactor_checkpoint::workflow::restore_checkpoint(args)?;
            }
            command::RefactorCommand::DeleteCheckpoint(args) => {
                refactor_checkpoint::workflow::delete_checkpoint(args)?;
            }
            command::RefactorCommand::WorkspacePlan(args) => {
                refactor::workflow::workspace_refactor_plan(args)?;
            }
            command::RefactorCommand::WorkspacePreview(args) => {
                refactor::workflow::workspace_refactor_preview(args)?;
            }
            command::RefactorCommand::WorkspaceExecute(args) => {
                refactor::workflow::workspace_refactor_execute(args)?;
            }
            command::RefactorCommand::RemoveDefinition(args) => {
                definition_removal::remove_definition::remove_definition(args)?;
            }
            command::RefactorCommand::RemoveUnusedDefinitions(args) => {
                definition_removal::remove_unused_definitions::remove_unused_definitions(args)?;
            }
            command::RefactorCommand::AddIgnoreDeclaration(args) => {
                add_ignore_declaration::workflow::add_ignore_declaration(args)?;
            }
            command::RefactorCommand::FoldConstants(args) => {
                fold_constants::workflow::fold_constants(args)?;
            }
            command::RefactorCommand::MoveDefinition(args) => {
                definition_movement::move_definition::move_definition(args)?;
            }
            command::RefactorCommand::SplitFile(args) => {
                definition_movement::split_file::split_file(args)?;
            }
            command::RefactorCommand::SortDefinitions(args) => {
                definition_movement::sort_definitions::sort_definitions(args)?;
            }
            command::RefactorCommand::MoveForm(args) => {
                definition_movement::move_form::move_form(args)?;
            }
            command::RefactorCommand::InsertTopLevel(args) => {
                definition_movement::insert_top_level::insert_top_level(args)?;
            }
            command::RefactorCommand::ReplacementPlan(args) => {
                duplicate_report::workflow::replacement_plan(args)?;
            }
            command::RefactorCommand::ReplaceForms(args) => replace_forms::replace_forms(args)?,
            command::RefactorCommand::AddExport(args) => package::add_export::add_export(args)?,
            command::RefactorCommand::SortPackageExports(args) => {
                package::sort_exports::sort_package_exports(args)?;
            }
            command::RefactorCommand::SortPackageOptions(args) => {
                package::sort_options::sort_package_options(args)?;
            }
            command::RefactorCommand::MergePackageOptions(args) => {
                package::merge_options::merge_package_options(args)?;
            }
            command::RefactorCommand::RenamePackage(args) => package::rename::rename_package(args)?,
            command::RefactorCommand::RenameAt(args) => rename::rename_at::rename_at(args)?,
            command::RefactorCommand::RenameSymbol(args) => {
                rename::rename_symbol::rename_symbol(args)?;
            }
            command::RefactorCommand::RenameInForm(args) => {
                rename::rename_in_form::rename_in_form(args)?;
            }
            command::RefactorCommand::RenameBinding(args) => {
                rename::rename_binding::rename_binding(args)?;
            }
            command::RefactorCommand::RenameBlock(args) => rename_control::rename_block(args)?,
            command::RefactorCommand::RenameTag(args) => rename_control::rename_tag(args)?,
            command::RefactorCommand::RemoveUnusedBlock(args) => {
                remove_unused_control::remove_unused_block(args)?;
            }
            command::RefactorCommand::RemoveUnusedTag(args) => {
                remove_unused_control::remove_unused_tag(args)?;
            }
            command::RefactorCommand::RenameSymbols(args) => {
                rename::rename_symbols::rename_symbols(args)?;
            }
            command::RefactorCommand::RenameFunction(args) => {
                rename::rename_function::rename_function(args)?;
            }
            command::RefactorCommand::RenameMacrolet(args) => {
                rename::rename_macrolet::rename_macrolet(args)?;
            }
            command::RefactorCommand::RenameSymbolMacro(args) => {
                rename::rename_symbol_macro::rename_symbol_macro(args)?;
            }
            command::RefactorCommand::RenameLocalFunction(args) => {
                rename::rename_local_function::rename_local_function(args)?;
            }
            command::RefactorCommand::ReplaceFunctionCalls(args) => {
                rename::replace_function_calls::replace_function_calls(args)?;
            }
            command::RefactorCommand::WrapFunctionCalls(args) => {
                rename::wrap_function_calls::wrap_function_calls(args)?;
            }
            command::RefactorCommand::UnwrapFunctionCalls(args) => {
                rename::unwrap_function_calls::unwrap_function_calls(args)?;
            }
            command::RefactorCommand::UnwrapCall(args) => unwrap_call::unwrap_call(args)?,
            command::RefactorCommand::ThreadExpression(args) => {
                thread_expression::thread_expression(args)?;
            }
            command::RefactorCommand::UnthreadExpression(args) => {
                unthread_expression::unthread_expression(args)?;
            }
            command::RefactorCommand::ExtractFunction(args) => {
                extract_function::extract_function(args)?;
            }
            command::RefactorCommand::ExtractLocalFunction(args) => {
                extract_local_function::extract_local_function(args)?;
            }
            command::RefactorCommand::ExtractConstant(args) => {
                extract_constant::extract_constant(args)?;
            }
            command::RefactorCommand::InlineFunction(args) => {
                inline_function::inline_function(args)?;
            }
            command::RefactorCommand::InlineLambda(args) => inline_lambda::inline_lambda(args)?,
            command::RefactorCommand::InlineLocalFunction(args) => {
                inline_local_function::inline_local_function(args)?;
            }
            command::RefactorCommand::InlineSymbolMacro(args) => {
                inline_symbol_macro::inline_symbol_macro(args)?;
            }
            command::RefactorCommand::InlineLiteralConstant(args) => {
                inline_literal_constant::inline_literal_constant(args)?;
            }
            command::RefactorCommand::AddFunctionParameter(args) => {
                function_parameter::add::add_function_parameter(args)?;
            }
            command::RefactorCommand::MoveFunctionParameter(args) => {
                function_parameter::move_parameter::move_function_parameter(args)?;
            }
            command::RefactorCommand::SwapFunctionParameters(args) => {
                function_parameter::swap::swap_function_parameters(args)?;
            }
            command::RefactorCommand::ReorderFunctionParameters(args) => {
                function_parameter::reorder::reorder_function_parameters(args)?;
            }
            command::RefactorCommand::RemoveFunctionParameter(args) => {
                function_parameter::remove::remove_function_parameter(args)?;
            }
            command::RefactorCommand::IntroduceLet(args) => introduce_let::introduce_let(args)?,
            command::RefactorCommand::InlineLet(args) => inline_let::inline_let(args)?,
            command::RefactorCommand::ConvertLetToLetStar(args) => {
                convert_let_to_let_star::convert_let_to_let_star(args)?;
            }
            command::RefactorCommand::ConvertLetStarToLet(args) => {
                convert_let_star_to_let::convert_let_star_to_let(args)?;
            }
            command::RefactorCommand::ConvertDoStarToDo(args) => {
                convert_sequential_binding::convert_do_star_to_do(args)?;
            }
            command::RefactorCommand::ConvertProgStarToProg(args) => {
                convert_sequential_binding::convert_prog_star_to_prog(args)?;
            }
            command::RefactorCommand::MergeNestedLetStar(args) => {
                merge_nested_let_star::merge_nested_let_star(args)?;
            }
            command::RefactorCommand::MergeNestedLet(args) => {
                merge_nested_let::merge_nested_let(args)?;
            }
            command::RefactorCommand::MergeNestedFlet(args) => {
                merge_nested_flet::merge_nested_flet(args)?;
            }
            command::RefactorCommand::SplitLetStar(args) => split_let_star::split_let_star(args)?,
            command::RefactorCommand::SplitLet(args) => split_let::split_let(args)?,
            command::RefactorCommand::EliminateEmptyBindingForm(args) => {
                eliminate_empty_binding_form::eliminate_empty_binding_form(args)?;
            }
            command::RefactorCommand::FlattenProgn(args) => flatten_progn::flatten_progn(args)?,
            command::RefactorCommand::ConvertIfToCond(args) => {
                convert_if_to_cond::convert_if_to_cond(args)?;
            }
            command::RefactorCommand::ConvertCondToIf(args) => {
                convert_cond_to_if::convert_cond_to_if(args)?;
            }
            command::RefactorCommand::ConvertWhenToIf(args) => {
                convert_when_to_if::convert_when_to_if(args)?;
            }
            command::RefactorCommand::ConvertUnlessToIf(args) => {
                convert_unless_to_if::convert_unless_to_if(args)?;
            }
            command::RefactorCommand::ConvertIfToWhen(args) => {
                convert_if_to_when::convert_if_to_when(args)?;
            }
            command::RefactorCommand::ConvertIfToUnless(args) => {
                convert_if_to_unless::convert_if_to_unless(args)?;
            }
            command::RefactorCommand::ConvertLabelsToFlet(args) => {
                convert_labels_to_flet::convert_labels_to_flet(args)?;
            }
            command::RefactorCommand::ConvertFletToLabels(args) => {
                convert_flet_to_labels::convert_flet_to_labels(args)?;
            }
            command::RefactorCommand::RemoveUnusedBinding(args) => {
                remove_unused_binding::remove_unused_binding(args)?;
            }
        },
        Command::Query { command } => match command {
            command::QueryCommand::Find(args) => query_find::workflow::query_find(args)?,
            command::QueryCommand::Count(args) => query_count::workflow::query_count(args)?,
            command::QueryCommand::Replace(args) => {
                query_replace::workflow::query_replace(args)?;
            }
        },
        Command::Fix { command } => fix::fix(command)?,
        Command::Migrate { command } => migrate(command)?,
        Command::Schema { command } => schema(command)?,
        Command::Config { command } => match command {
            command::ConfigCommand::Check(args) => config::workflow::check(args)?,
            command::ConfigCommand::Show(args) => config::workflow::show(args)?,
            command::ConfigCommand::Schema(args) => config::workflow::schema_report(args)?,
            command::ConfigCommand::Init(args) => config::workflow::init(args)?,
        },
        Command::Generate { command } => match command {
            command::GenerateCommand::Defpackage(args) => {
                generate_defpackage::generate_defpackage(args)?;
            }
            command::GenerateCommand::Defsystem(args) => {
                generate_defsystem::generate_defsystem(args)?;
            }
            command::GenerateCommand::Tests(args) => generate_tests::generate_tests(args)?,
            command::GenerateCommand::Accessors(args) => {
                generate_accessors::generate_accessors(args)?;
            }
            command::GenerateCommand::Defgeneric(args) => {
                generate_defgeneric::generate_defgeneric(args)?;
            }
            command::GenerateCommand::Docstring(args) => {
                generate_docstring::generate_docstring(args)?;
            }
        },
        // Handled in `run`, before dispatch, because they own their exit
        // status: a closed pipe is how a protocol session normally ends, and
        // `tui`'s own `q` quit is not this process failing either.
        Command::Lsp(_) | Command::Mcp(_) | Command::Serve(_) | Command::Tui(_) => {
            unreachable!("the protocol servers and tui are dispatched from run")
        }
        Command::Completions { shell } => {
            use clap::CommandFactory;
            let mut root = super::Cli::command();
            clap_complete::generate(shell, &mut root, "paredit", &mut std::io::stdout());
        }
    }
    Ok(())
}
