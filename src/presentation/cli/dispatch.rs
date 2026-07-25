use super::*;

pub(super) fn dispatch(command: Command) -> Result<()> {
    match command {
        Command::Inspect { command } => match command {
            command::InspectCommand::Check(args) => analysis_report::workflow::check(args)?,
            command::InspectCommand::Dialect(args) => analysis_report::workflow::dialect(args)?,
            command::InspectCommand::Stats(args) => analysis_report::workflow::stats(args)?,
            command::InspectCommand::AgentReport(args) => {
                analysis_report::workflow::agent_report(args)?
            }
            command::InspectCommand::Capabilities(args) => capabilities::capabilities(args)?,
            command::InspectCommand::Outline(args) => analysis_report::workflow::outline(args)?,
            command::InspectCommand::Form(args) => form_report::workflow::form_report(args)?,
            command::InspectCommand::FindSymbol(args) => {
                symbol_report::workflow::find_symbol(args)?
            }
            command::InspectCommand::Symbols(args) => symbol_report::workflow::symbol_report(args)?,
            command::InspectCommand::Calls(args) => call_report::workflow::call_report(args)?,
            command::InspectCommand::Signature(args) => {
                signature_report::workflow::signature_report(args)?
            }
            command::InspectCommand::CallGraph(args) => {
                call_graph_report::workflow::call_graph(args)?
            }
            command::InspectCommand::Impact(args) => impact_report::workflow::impact_report(args)?,
            command::InspectCommand::Workspace(args) => {
                workspace_report::workflow::workspace_report(args)?
            }
            command::InspectCommand::Dependencies(args) => {
                dependency_report::workflow::dependency_report(args)?
            }
            command::InspectCommand::Packages(args) => package::report::package_report(args)?,
            command::InspectCommand::Definitions(args) => {
                definition_report::workflow::definition_report(args)?
            }
            command::InspectCommand::UnusedDefinitions(args) => {
                definition_report::workflow::unused_definition_report(args)?
            }
            command::InspectCommand::Duplicates(args) => {
                duplicate_report::workflow::duplicate_report(args)?
            }
            command::InspectCommand::DuplicateSetfPlaces(args) => {
                duplicate_setf_place_report::workflow::duplicate_setf_place_report(args)?
            }
            command::InspectCommand::DuplicateSlots(args) => {
                duplicate_slot_report::workflow::duplicate_slot_report(args)?
            }
            command::InspectCommand::DuplicateMethods(args) => {
                duplicate_method_report::workflow::duplicate_method_report(args)?
            }
            command::InspectCommand::DuplicateParameters(args) => {
                duplicate_parameter_report::workflow::duplicate_parameter_report(args)?
            }
            command::InspectCommand::DuplicateLambdaListKeyword(args) => {
                duplicate_lambda_list_keyword_report::workflow::duplicate_lambda_list_keyword_report(
                    args,
                )?
            }
            command::InspectCommand::LambdaListKeywordOrder(args) => {
                lambda_list_keyword_order_report::workflow::lambda_list_keyword_order_report(args)?
            }
            command::InspectCommand::DuplicateCaseKeys(args) => {
                duplicate_case_key_report::workflow::duplicate_case_key_report(args)?
            }
            command::InspectCommand::QuotedCaseKey(args) => {
                quoted_case_key_report::workflow::quoted_case_key_report(args)?
            }
            command::InspectCommand::CaseNilKey(args) => {
                case_nil_key_report::workflow::case_nil_key_report(args)?
            }
            command::InspectCommand::TypecaseNilKey(args) => {
                typecase_nil_key_report::workflow::typecase_nil_key_report(args)?
            }
            command::InspectCommand::MalformedCaseClause(args) => {
                malformed_case_clause_report::workflow::malformed_case_clause_report(args)?
            }
            command::InspectCommand::UnreachableCaseClause(args) => {
                unreachable_case_clause_report::workflow::unreachable_case_clause_report(args)?
            }
            command::InspectCommand::ExhaustiveCaseOtherwise(args) => {
                exhaustive_case_otherwise_report::workflow::exhaustive_case_otherwise_report(args)?
            }
            command::InspectCommand::ExplicitStepDelta(args) => {
                explicit_step_delta_report::workflow::explicit_step_delta_report(args)?
            }
            command::InspectCommand::NegatedStepDelta(args) => {
                negated_step_delta_report::workflow::negated_step_delta_report(args)?
            }
            command::InspectCommand::ExplicitNilReturn(args) => {
                explicit_nil_return_report::workflow::explicit_nil_return_report(args)?
            }
            command::InspectCommand::DuplicateCondTests(args) => {
                duplicate_cond_test_report::workflow::duplicate_cond_test_report(args)?
            }
            command::InspectCommand::UnreachableCondClause(args) => {
                unreachable_cond_clause_report::workflow::unreachable_cond_clause_report(args)?
            }
            command::InspectCommand::MalformedCondClause(args) => {
                malformed_cond_clause_report::workflow::malformed_cond_clause_report(args)?
            }
            command::InspectCommand::DuplicateLetBindings(args) => {
                duplicate_let_binding_report::workflow::duplicate_let_binding_report(args)?
            }
            command::InspectCommand::MalformedLetBinding(args) => {
                malformed_let_binding_report::workflow::malformed_let_binding_report(args)?
            }
            command::InspectCommand::ManualIncf(args) => {
                manual_incf_report::workflow::manual_incf_report(args)?
            }
            command::InspectCommand::ManualPush(args) => {
                manual_push_report::workflow::manual_push_report(args)?
            }
            command::InspectCommand::ManualPushnew(args) => {
                manual_pushnew_report::workflow::manual_pushnew_report(args)?
            }
            command::InspectCommand::BindsConstant(args) => {
                binds_constant_report::workflow::binds_constant_report(args)?
            }
            command::InspectCommand::MalformedIterationSpec(args) => {
                malformed_iteration_spec_report::workflow::malformed_iteration_spec_report(args)?
            }
            command::InspectCommand::DuplicateBooleanOperands(args) => {
                duplicate_boolean_operand_report::workflow::duplicate_boolean_operand_report(args)?
            }
            command::InspectCommand::DeadBooleanOperand(args) => {
                dead_boolean_operand_report::workflow::dead_boolean_operand_report(args)?
            }
            command::InspectCommand::SelfAssignments(args) => {
                self_assignment_report::workflow::self_assignment_report(args)?
            }
            command::InspectCommand::SetfArity(args) => {
                setf_arity_report::workflow::setf_arity_report(args)?
            }
            command::InspectCommand::SetqNonVariable(args) => {
                setq_non_variable_report::workflow::setq_non_variable_report(args)?
            }
            command::InspectCommand::ModifyMacroArity(args) => {
                modify_macro_arity_report::workflow::modify_macro_arity_report(args)?
            }
            command::InspectCommand::SelfComparison(args) => {
                self_comparison_report::workflow::self_comparison_report(args)?
            }
            command::InspectCommand::IdenticalIfBranches(args) => {
                identical_if_branch_report::workflow::identical_if_branch_report(args)?
            }
            command::InspectCommand::IfArity(args) => {
                if_arity_report::workflow::if_arity_report(args)?
            }
            command::InspectCommand::TComparison(args) => {
                t_comparison_report::workflow::t_comparison_report(args)?
            }
            command::InspectCommand::TheArity(args) => {
                the_arity_report::workflow::the_arity_report(args)?
            }
            command::InspectCommand::EqualityArity(args) => {
                equality_arity_report::workflow::equality_arity_report(args)?
            }
            command::InspectCommand::AccessorArity(args) => {
                accessor_arity_report::workflow::accessor_arity_report(args)?
            }
            command::InspectCommand::EvalWhenSituation(args) => {
                eval_when_situation_report::workflow::eval_when_situation_report(args)?
            }
            command::InspectCommand::EqlStringComparison(args) => {
                eql_string_comparison_report::workflow::eql_string_comparison_report(args)?
            }
            command::InspectCommand::EqNumberComparison(args) => {
                eq_number_comparison_report::workflow::eq_number_comparison_report(args)?
            }
            command::InspectCommand::EqCharComparison(args) => {
                eq_char_comparison_report::workflow::eq_char_comparison_report(args)?
            }
            command::InspectCommand::DestructiveLiteral(args) => {
                destructive_literal_report::workflow::destructive_literal_report(args)?
            }
            command::InspectCommand::EqlSearchLiteral(args) => {
                eql_search_literal_report::workflow::eql_search_literal_report(args)?
            }
            command::InspectCommand::CharOpString(args) => {
                char_op_string_report::workflow::char_op_string_report(args)?
            }
            command::InspectCommand::EmptyBody(args) => {
                empty_body_report::workflow::empty_body_report(args)?
            }
            command::InspectCommand::EmptyLet(args) => {
                empty_let_report::workflow::empty_let_report(args)?
            }
            command::InspectCommand::IdentityArithmetic(args) => {
                identity_arithmetic_report::workflow::identity_arithmetic_report(args)?
            }
            command::InspectCommand::RedundantDivisor(args) => {
                redundant_divisor_report::workflow::redundant_divisor_report(args)?
            }
            command::InspectCommand::EqlListComparison(args) => {
                eql_list_comparison_report::workflow::eql_list_comparison_report(args)?
            }
            command::InspectCommand::Lint(args) => lint_report::workflow::lint_report(args)?,
            command::InspectCommand::Similarity(args) => {
                similarity_report::workflow::similarity_report(args)?
            }
            command::InspectCommand::Lets(args) => let_report::let_report(args)?,
            command::InspectCommand::Complexity(args) => {
                complexity_report::workflow::complexity_report(args)?
            }
            command::InspectCommand::Naming(args) => naming_report::workflow::naming_report(args)?,
            command::InspectCommand::Reachability(args) => {
                reachability_report::workflow::reachability_report(args)?
            }
            command::InspectCommand::Redefinitions(args) => {
                redefinition_report::workflow::redefinition_report(args)?
            }
            command::InspectCommand::RedundantQuote(args) => {
                redundant_quote_report::workflow::redundant_quote_report(args)?
            }
            command::InspectCommand::RedundantProgn(args) => {
                redundant_progn_report::workflow::redundant_progn_report(args)?
            }
            command::InspectCommand::RedundantProg1(args) => {
                redundant_prog1_report::workflow::redundant_prog1_report(args)?
            }
            command::InspectCommand::NegatedWhenUnless(args) => {
                negated_when_unless_report::workflow::negated_when_unless_report(args)?
            }
            command::InspectCommand::NegatedComparison(args) => {
                negated_comparison_report::workflow::negated_comparison_report(args)?
            }
            command::InspectCommand::NegatedIf(args) => {
                negated_if_report::workflow::negated_if_report(args)?
            }
            command::InspectCommand::IfToOr(args) => {
                if_to_or_report::workflow::if_to_or_report(args)?
            }
            command::InspectCommand::IfNot(args) => {
                if_not_report::workflow::if_not_report(args)?
            }
            command::InspectCommand::OneStepArithmetic(args) => {
                one_step_arithmetic_report::workflow::one_step_arithmetic_report(args)?
            }
            command::InspectCommand::ConstantIfTest(args) => {
                constant_if_test_report::workflow::constant_if_test_report(args)?
            }
            command::InspectCommand::ConstantWhenTest(args) => {
                constant_when_test_report::workflow::constant_when_test_report(args)?
            }
            command::InspectCommand::ConsToList(args) => {
                cons_to_list_report::workflow::cons_to_list_report(args)?
            }
            command::InspectCommand::DoubleReverse(args) => {
                double_reverse_report::workflow::double_reverse_report(args)?
            }
            command::InspectCommand::AppendListToCons(args) => {
                append_list_to_cons_report::workflow::append_list_to_cons_report(args)?
            }
            command::InspectCommand::ListStarToCons(args) => {
                list_star_to_cons_report::workflow::list_star_to_cons_report(args)?
            }
            command::InspectCommand::ValuesListOfList(args) => {
                values_list_of_list_report::workflow::values_list_of_list_report(args)?
            }
            command::InspectCommand::MultipleValueListOfValues(args) => {
                multiple_value_list_of_values_report::workflow::multiple_value_list_of_values_report(
                    args,
                )?
            }
            command::InspectCommand::AppendNil(args) => {
                append_nil_report::workflow::append_nil_report(args)?
            }
            command::InspectCommand::VerboseNegation(args) => {
                verbose_negation_report::workflow::verbose_negation_report(args)?
            }
            command::InspectCommand::NestedBoolean(args) => {
                nested_boolean_report::workflow::nested_boolean_report(args)?
            }
            command::InspectCommand::NestedCxr(args) => {
                nested_cxr_report::workflow::nested_cxr_report(args)?
            }
            command::InspectCommand::NthcdrZero(args) => {
                nthcdr_zero_report::workflow::nthcdr_zero_report(args)?
            }
            command::InspectCommand::SubseqZero(args) => {
                subseq_zero_report::workflow::subseq_zero_report(args)?
            }
            command::InspectCommand::CarNthcdr(args) => {
                car_nthcdr_report::workflow::car_nthcdr_report(args)?
            }
            command::InspectCommand::CarReverse(args) => {
                car_reverse_report::workflow::car_reverse_report(args)?
            }
            command::InspectCommand::NthcdrSmallIndex(args) => {
                nthcdr_small_index_report::workflow::nthcdr_small_index_report(args)?
            }
            command::InspectCommand::NthConstantIndex(args) => {
                nth_constant_index_report::workflow::nth_constant_index_report(args)?
            }
            command::InspectCommand::NestedProgn(args) => {
                nested_progn_report::workflow::nested_progn_report(args)?
            }
            command::InspectCommand::NestedUnless(args) => {
                nested_unless_report::workflow::nested_unless_report(args)?
            }
            command::InspectCommand::NestedWhen(args) => {
                nested_when_report::workflow::nested_when_report(args)?
            }
            command::InspectCommand::NilComparison(args) => {
                nil_comparison_report::workflow::nil_comparison_report(args)?
            }
            command::InspectCommand::OneArmedIf(args) => {
                one_armed_if_report::workflow::one_armed_if_report(args)?
            }
            command::InspectCommand::RedundantApply(args) => {
                redundant_apply_report::workflow::redundant_apply_report(args)?
            }
            command::InspectCommand::RedundantEqlTest(args) => {
                redundant_eql_test_report::workflow::redundant_eql_test_report(args)?
            }
            command::InspectCommand::RedundantIdentityKey(args) => {
                redundant_identity_key_report::workflow::redundant_identity_key_report(args)?
            }
            command::InspectCommand::RedundantBodyProgn(args) => {
                redundant_body_progn_report::workflow::redundant_body_progn_report(args)?
            }
            command::InspectCommand::RedundantBooleanIdentity(args) => {
                redundant_boolean_identity_report::workflow::redundant_boolean_identity_report(
                    args,
                )?
            }
            command::InspectCommand::DeMorgan(args) => {
                de_morgan_report::workflow::de_morgan_report(args)?
            }
            command::InspectCommand::RedundantIdentity(args) => {
                redundant_identity_report::workflow::redundant_identity_report(args)?
            }
            command::InspectCommand::RedundantIfNil(args) => {
                redundant_if_nil_report::workflow::redundant_if_nil_report(args)?
            }
            command::InspectCommand::RedundantLetStar(args) => {
                redundant_let_star_report::workflow::redundant_let_star_report(args)?
            }
            command::InspectCommand::RedundantFuncall(args) => {
                redundant_funcall_report::workflow::redundant_funcall_report(args)?
            }
            command::InspectCommand::RedundantThe(args) => {
                redundant_the_report::workflow::redundant_the_report(args)?
            }
            command::InspectCommand::FuncallLambda(args) => {
                funcall_lambda_report::workflow::funcall_lambda_report(args)?
            }
            command::InspectCommand::SharpQuotedLambda(args) => {
                sharp_quoted_lambda_report::workflow::sharp_quoted_lambda_report(args)?
            }
            command::InspectCommand::SingleOperandBoolean(args) => {
                single_operand_boolean_report::workflow::single_operand_boolean_report(args)?
            }
            command::InspectCommand::SingleOperandListOp(args) => {
                single_operand_list_op_report::workflow::single_operand_list_op_report(args)?
            }
            command::InspectCommand::SingleOperandArithmetic(args) => {
                single_operand_arithmetic_report::workflow::single_operand_arithmetic_report(args)?
            }
            command::InspectCommand::SingleArgComparison(args) => {
                single_arg_comparison_report::workflow::single_arg_comparison_report(args)?
            }
            command::InspectCommand::SingleClauseCond(args) => {
                single_clause_cond_report::workflow::single_clause_cond_report(args)?
            }
            command::InspectCommand::CondTClause(args) => {
                cond_t_clause_report::workflow::cond_t_clause_report(args)?
            }
            command::InspectCommand::SingleValueBind(args) => {
                single_value_bind_report::workflow::single_value_bind_report(args)?
            }
            command::InspectCommand::SignComparison(args) => {
                sign_comparison_report::workflow::sign_comparison_report(args)?
            }
            command::InspectCommand::FormatMissingDestination(args) => {
                format_missing_destination_report::workflow::format_missing_destination_report(args)?
            }
            command::InspectCommand::FormatToString(args) => {
                format_to_string_report::workflow::format_to_string_report(args)?
            }
            command::InspectCommand::FormatNewline(args) => {
                format_newline_report::workflow::format_newline_report(args)?
            }
            command::InspectCommand::LiteralPlace(args) => {
                literal_place_report::workflow::literal_place_report(args)?
            }
            command::InspectCommand::UnusedParameters(args) => {
                unused_parameter_report::workflow::unused_parameter_report(args)?
            }
            command::InspectCommand::ShadowedBindings(args) => {
                shadowed_binding_report::workflow::shadowed_binding_report(args)?
            }
            command::InspectCommand::UnusedLocalCallables(args) => {
                unused_local_callable_report::workflow::unused_local_callable_report(args)?
            }
            command::InspectCommand::PackageBoundaries(args) => {
                package_boundary_report::workflow::package_boundary_report(args)?
            }
            command::InspectCommand::CallCycles(args) => {
                call_cycle_report::workflow::call_cycle_report(args)?
            }
            command::InspectCommand::PackageCycles(args) => {
                package_cycle_report::workflow::package_cycle_report(args)?
            }
            command::InspectCommand::PackageConflicts(args) => {
                package_conflict_report::workflow::package_conflict_report(args)?
            }
            command::InspectCommand::SystemConflicts(args) => {
                system_conflict_report::workflow::system_conflict_report(args)?
            }
            command::InspectCommand::SystemCycles(args) => {
                system_cycle_report::workflow::system_cycle_report(args)?
            }
            command::InspectCommand::ClassCycles(args) => {
                class_cycle_report::workflow::class_cycle_report(args)?
            }
            command::InspectCommand::StructCycles(args) => {
                struct_cycle_report::workflow::struct_cycle_report(args)?
            }
            command::InspectCommand::UnusedPackages(args) => {
                unused_package_report::workflow::unused_package_report(args)?
            }
            command::InspectCommand::UnusedExports(args) => {
                unused_export_report::workflow::unused_export_report(args)?
            }
            command::InspectCommand::DuplicateExports(args) => {
                duplicate_export_report::workflow::duplicate_export_report(args)?
            }
            command::InspectCommand::UnusedNicknames(args) => {
                unused_nickname_report::workflow::unused_nickname_report(args)?
            }
            command::InspectCommand::UndefinedPackages(args) => {
                undefined_package_report::workflow::undefined_package_report(args)?
            }
        },
        Command::Edit { command } => match command {
            command::EditCommand::Format(args) => basic_edit::workflow::format(args)?,
            command::EditCommand::RepairUnclosedLists(args) => {
                basic_edit::workflow::repair_unclosed_lists(args)?
            }
            command::EditCommand::Select(args) => basic_edit::workflow::select(args)?,
            command::EditCommand::Replace(args) => basic_edit::workflow::replace(args)?,
            command::EditCommand::Kill(args) => basic_edit::workflow::kill(args)?,
            command::EditCommand::Wrap(args) => basic_edit::workflow::wrap(args)?,
            command::EditCommand::Splice(args) => basic_edit::workflow::splice(args)?,
            command::EditCommand::Split(args) => basic_edit::workflow::split(args)?,
            command::EditCommand::Join(args) => basic_edit::workflow::join(args)?,
            command::EditCommand::SpliceKillingBackward(args) => {
                basic_edit::workflow::splice_killing_backward(args)?
            }
            command::EditCommand::SpliceKillingForward(args) => {
                basic_edit::workflow::splice_killing_forward(args)?
            }
            command::EditCommand::Convolute(args) => basic_edit::workflow::convolute(args)?,
            command::EditCommand::Raise(args) => basic_edit::workflow::raise(args)?,
            command::EditCommand::TransposeForward(args) => {
                basic_edit::workflow::transpose_forward(args)?
            }
            command::EditCommand::TransposeBackward(args) => {
                basic_edit::workflow::transpose_backward(args)?
            }
            command::EditCommand::SlurpForward(args) => basic_edit::workflow::slurp_forward(args)?,
            command::EditCommand::SlurpBackward(args) => {
                basic_edit::workflow::slurp_backward(args)?
            }
            command::EditCommand::BarfForward(args) => basic_edit::workflow::barf_forward(args)?,
            command::EditCommand::BarfBackward(args) => basic_edit::workflow::barf_backward(args)?,
        },
        Command::Refactor { command } => match command {
            command::RefactorCommand::Plan(args) => refactor::workflow::refactor_plan(args)?,
            command::RefactorCommand::Verify(args) => refactor::workflow::verify_refactor(args)?,
            command::RefactorCommand::Preview(args) => refactor::workflow::refactor_preview(args)?,
            command::RefactorCommand::Check(args) => refactor::workflow::refactor_check(args)?,
            command::RefactorCommand::Status(args) => refactor::workflow::refactor_status(args)?,
            command::RefactorCommand::Apply(args) => refactor::workflow::refactor_apply(args)?,
            command::RefactorCommand::Diff(args) => refactor::workflow::refactor_diff(args)?,
            command::RefactorCommand::WorkspacePlan(args) => {
                refactor::workflow::workspace_refactor_plan(args)?
            }
            command::RefactorCommand::WorkspacePreview(args) => {
                refactor::workflow::workspace_refactor_preview(args)?
            }
            command::RefactorCommand::WorkspaceExecute(args) => {
                refactor::workflow::workspace_refactor_execute(args)?
            }
            command::RefactorCommand::RemoveDefinition(args) => {
                definition_removal::remove_definition::remove_definition(args)?
            }
            command::RefactorCommand::RemoveUnusedDefinitions(args) => {
                definition_removal::remove_unused_definitions::remove_unused_definitions(args)?
            }
            command::RefactorCommand::MoveDefinition(args) => {
                definition_movement::move_definition::move_definition(args)?
            }
            command::RefactorCommand::SplitFile(args) => {
                definition_movement::split_file::split_file(args)?
            }
            command::RefactorCommand::SortDefinitions(args) => {
                definition_movement::sort_definitions::sort_definitions(args)?
            }
            command::RefactorCommand::MoveForm(args) => {
                definition_movement::move_form::move_form(args)?
            }
            command::RefactorCommand::InsertTopLevel(args) => {
                definition_movement::insert_top_level::insert_top_level(args)?
            }
            command::RefactorCommand::ReplacementPlan(args) => {
                duplicate_report::workflow::replacement_plan(args)?
            }
            command::RefactorCommand::ReplaceForms(args) => replace_forms::replace_forms(args)?,
            command::RefactorCommand::AddExport(args) => package::add_export::add_export(args)?,
            command::RefactorCommand::SortPackageExports(args) => {
                package::sort_exports::sort_package_exports(args)?
            }
            command::RefactorCommand::SortPackageOptions(args) => {
                package::sort_options::sort_package_options(args)?
            }
            command::RefactorCommand::MergePackageOptions(args) => {
                package::merge_options::merge_package_options(args)?
            }
            command::RefactorCommand::RenamePackage(args) => package::rename::rename_package(args)?,
            command::RefactorCommand::RenameAt(args) => rename::rename_at::rename_at(args)?,
            command::RefactorCommand::RenameSymbol(args) => {
                rename::rename_symbol::rename_symbol(args)?
            }
            command::RefactorCommand::RenameInForm(args) => {
                rename::rename_in_form::rename_in_form(args)?
            }
            command::RefactorCommand::RenameBinding(args) => {
                rename::rename_binding::rename_binding(args)?
            }
            command::RefactorCommand::RenameBlock(args) => rename_control::rename_block(args)?,
            command::RefactorCommand::RenameTag(args) => rename_control::rename_tag(args)?,
            command::RefactorCommand::RemoveUnusedBlock(args) => {
                remove_unused_control::remove_unused_block(args)?
            }
            command::RefactorCommand::RemoveUnusedTag(args) => {
                remove_unused_control::remove_unused_tag(args)?
            }
            command::RefactorCommand::RenameSymbols(args) => {
                rename::rename_symbols::rename_symbols(args)?
            }
            command::RefactorCommand::RenameFunction(args) => {
                rename::rename_function::rename_function(args)?
            }
            command::RefactorCommand::RenameMacrolet(args) => {
                rename::rename_macrolet::rename_macrolet(args)?
            }
            command::RefactorCommand::RenameSymbolMacro(args) => {
                rename::rename_symbol_macro::rename_symbol_macro(args)?
            }
            command::RefactorCommand::RenameLocalFunction(args) => {
                rename::rename_local_function::rename_local_function(args)?
            }
            command::RefactorCommand::ReplaceFunctionCalls(args) => {
                rename::replace_function_calls::replace_function_calls(args)?
            }
            command::RefactorCommand::WrapFunctionCalls(args) => {
                rename::wrap_function_calls::wrap_function_calls(args)?
            }
            command::RefactorCommand::UnwrapFunctionCalls(args) => {
                rename::unwrap_function_calls::unwrap_function_calls(args)?
            }
            command::RefactorCommand::UnwrapCall(args) => unwrap_call::unwrap_call(args)?,
            command::RefactorCommand::ThreadExpression(args) => {
                thread_expression::thread_expression(args)?
            }
            command::RefactorCommand::UnthreadExpression(args) => {
                unthread_expression::unthread_expression(args)?
            }
            command::RefactorCommand::ExtractFunction(args) => {
                extract_function::extract_function(args)?
            }
            command::RefactorCommand::ExtractLocalFunction(args) => {
                extract_local_function::extract_local_function(args)?
            }
            command::RefactorCommand::ExtractConstant(args) => {
                extract_constant::extract_constant(args)?
            }
            command::RefactorCommand::InlineFunction(args) => {
                inline_function::inline_function(args)?
            }
            command::RefactorCommand::InlineLambda(args) => inline_lambda::inline_lambda(args)?,
            command::RefactorCommand::InlineLocalFunction(args) => {
                inline_local_function::inline_local_function(args)?
            }
            command::RefactorCommand::InlineSymbolMacro(args) => {
                inline_symbol_macro::inline_symbol_macro(args)?
            }
            command::RefactorCommand::InlineLiteralConstant(args) => {
                inline_literal_constant::inline_literal_constant(args)?
            }
            command::RefactorCommand::AddFunctionParameter(args) => {
                function_parameter::add::add_function_parameter(args)?
            }
            command::RefactorCommand::MoveFunctionParameter(args) => {
                function_parameter::move_parameter::move_function_parameter(args)?
            }
            command::RefactorCommand::SwapFunctionParameters(args) => {
                function_parameter::swap::swap_function_parameters(args)?
            }
            command::RefactorCommand::ReorderFunctionParameters(args) => {
                function_parameter::reorder::reorder_function_parameters(args)?
            }
            command::RefactorCommand::RemoveFunctionParameter(args) => {
                function_parameter::remove::remove_function_parameter(args)?
            }
            command::RefactorCommand::IntroduceLet(args) => introduce_let::introduce_let(args)?,
            command::RefactorCommand::InlineLet(args) => inline_let::inline_let(args)?,
            command::RefactorCommand::ConvertLetToLetStar(args) => {
                convert_let_to_let_star::convert_let_to_let_star(args)?
            }
            command::RefactorCommand::ConvertLetStarToLet(args) => {
                convert_let_star_to_let::convert_let_star_to_let(args)?
            }
            command::RefactorCommand::ConvertDoStarToDo(args) => {
                convert_sequential_binding::convert_do_star_to_do(args)?
            }
            command::RefactorCommand::ConvertProgStarToProg(args) => {
                convert_sequential_binding::convert_prog_star_to_prog(args)?
            }
            command::RefactorCommand::MergeNestedLetStar(args) => {
                merge_nested_let_star::merge_nested_let_star(args)?
            }
            command::RefactorCommand::MergeNestedLet(args) => {
                merge_nested_let::merge_nested_let(args)?
            }
            command::RefactorCommand::MergeNestedFlet(args) => {
                merge_nested_flet::merge_nested_flet(args)?
            }
            command::RefactorCommand::SplitLetStar(args) => split_let_star::split_let_star(args)?,
            command::RefactorCommand::SplitLet(args) => split_let::split_let(args)?,
            command::RefactorCommand::EliminateEmptyBindingForm(args) => {
                eliminate_empty_binding_form::eliminate_empty_binding_form(args)?
            }
            command::RefactorCommand::FlattenProgn(args) => flatten_progn::flatten_progn(args)?,
            command::RefactorCommand::ConvertIfToCond(args) => {
                convert_if_to_cond::convert_if_to_cond(args)?
            }
            command::RefactorCommand::ConvertCondToIf(args) => {
                convert_cond_to_if::convert_cond_to_if(args)?
            }
            command::RefactorCommand::ConvertWhenToIf(args) => {
                convert_when_to_if::convert_when_to_if(args)?
            }
            command::RefactorCommand::ConvertUnlessToIf(args) => {
                convert_unless_to_if::convert_unless_to_if(args)?
            }
            command::RefactorCommand::ConvertIfToWhen(args) => {
                convert_if_to_when::convert_if_to_when(args)?
            }
            command::RefactorCommand::ConvertIfToUnless(args) => {
                convert_if_to_unless::convert_if_to_unless(args)?
            }
            command::RefactorCommand::ConvertLabelsToFlet(args) => {
                convert_labels_to_flet::convert_labels_to_flet(args)?
            }
            command::RefactorCommand::ConvertFletToLabels(args) => {
                convert_flet_to_labels::convert_flet_to_labels(args)?
            }
            command::RefactorCommand::RemoveUnusedBinding(args) => {
                remove_unused_binding::remove_unused_binding(args)?
            }
        },
        Command::Completions { shell } => {
            use clap::CommandFactory;
            let mut root = super::Cli::command();
            clap_complete::generate(shell, &mut root, "paredit", &mut std::io::stdout());
        }
    }
    Ok(())
}
