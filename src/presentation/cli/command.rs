use super::{
    args::{
        AnalyzeArgs, EditTargetArgs, FormatArgs, RepairArgs, ReplaceArgs, TargetArgs, WrapArgs,
    },
    call_graph_report, call_report, capabilities, constant_if_test_report, convert_cond_to_if,
    convert_flet_to_labels, convert_if_to_cond, convert_if_to_unless, convert_if_to_when,
    convert_labels_to_flet, convert_let_star_to_let, convert_let_to_let_star,
    convert_sequential_binding, convert_unless_to_if, convert_when_to_if, de_morgan_report,
    dead_boolean_operand_report, definition_movement, definition_removal, definition_report,
    dependency_report, duplicate_boolean_operand_report, duplicate_cond_test_report,
    duplicate_report, eliminate_empty_binding_form, explicit_nil_return_report, extract_constant,
    extract_function, extract_local_function, flatten_progn, form_report, funcall_lambda_report,
    function_parameter, identical_if_branch_report, if_to_or_report, impact_report,
    inline_function, inline_lambda, inline_let, inline_literal_constant, inline_local_function,
    inline_symbol_macro, introduce_let, let_report, merge_nested_flet, merge_nested_let,
    merge_nested_let_star, negated_comparison_report, negated_if_report,
    negated_when_unless_report, nested_boolean_report, nested_progn_report, nested_unless_report,
    nested_when_report, one_armed_if_report, package, redundant_apply_report,
    redundant_body_progn_report, redundant_boolean_identity_report, redundant_eql_test_report,
    redundant_funcall_report, redundant_identity_key_report, redundant_identity_report,
    redundant_if_nil_report, redundant_let_star_report, redundant_progn_report,
    redundant_quote_report, refactor, remove_unused_binding, remove_unused_control, rename,
    rename_control, replace_forms, sharp_quoted_lambda_report, signature_report, similarity_report,
    single_clause_cond_report, single_operand_boolean_report, split_let, split_let_star,
    symbol_report, thread_expression, unreachable_cond_clause_report, unthread_expression,
    unwrap_call, verbose_negation_report, workspace_report,
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
    /// Report a return/return-from with an explicit nil result, the default ((return nil) is (return)).
    ExplicitNilReturn(explicit_nil_return_report::args::ExplicitNilReturnReportArgs),
    /// Report cond forms with the same test expression in more than one clause.
    DuplicateCondTests(duplicate_cond_test_report::args::DuplicateCondTestReportArgs),
    /// Report cond forms with clauses after a t catch-all clause that can never run.
    UnreachableCondClause(unreachable_cond_clause_report::args::UnreachableCondClauseReportArgs),
    /// Report and/or forms that list the same operand more than once.
    DuplicateBooleanOperands(
        duplicate_boolean_operand_report::args::DuplicateBooleanOperandReportArgs,
    ),
    /// Report and/or forms whose non-final constant operand makes later operands dead.
    DeadBooleanOperand(dead_boolean_operand_report::args::DeadBooleanOperandReportArgs),
    /// Report if forms whose then and else branches are structurally identical.
    IdenticalIfBranches(identical_if_branch_report::args::IdenticalIfBranchReportArgs),
    /// Report structurally similar S-expression forms across explicit files.
    Similarity(similarity_report::args::SimilarityReportArgs),
    /// Report local let bindings and inline safety for agent refactor planning.
    Lets(let_report::LetReportArgs),
    /// Report self-evaluating literals (numbers, strings, characters, keywords) that are quoted redundantly.
    RedundantQuote(redundant_quote_report::args::RedundantQuoteReportArgs),
    /// Report progn forms that are redundant (empty, or wrapping a single form).
    RedundantProgn(redundant_progn_report::args::RedundantPrognReportArgs),
    /// Report when/unless forms whose test is a (not X)/(null X) negation (flip the macro instead).
    NegatedWhenUnless(negated_when_unless_report::args::NegatedWhenUnlessReportArgs),
    /// Report negated two-arg numeric comparisons ((not (= a b)) is (/= a b)).
    NegatedComparison(negated_comparison_report::args::NegatedComparisonReportArgs),
    /// Report three-arg if with a negated test ((if (not c) a b) is (if c b a)).
    NegatedIf(negated_if_report::args::NegatedIfReportArgs),
    /// Report an if with a literal t/nil test ((if t a b) is a; (if nil a b) is b).
    ConstantIfTest(constant_if_test_report::args::ConstantIfTestReportArgs),
    /// Report negation written the long way ((- 0 x) and (* x -1) are (- x)).
    VerboseNegation(verbose_negation_report::args::VerboseNegationReportArgs),
    /// Report a same-operator and/or nested in an and/or, which flattens ((or a (or b c)) is (or a b c)).
    NestedBoolean(nested_boolean_report::args::NestedBooleanReportArgs),
    /// Report progn forms with two or more body forms nested directly inside another progn.
    NestedProgn(nested_progn_report::args::NestedPrognReportArgs),
    /// Report an unless whose only body is an unless, mergeable by or ((unless a (unless b c)) is (unless (or a b) c)).
    NestedUnless(nested_unless_report::args::NestedUnlessReportArgs),
    /// Report a when whose only body is a when, mergeable by and ((when a (when b c)) is (when (and a b) c)).
    NestedWhen(nested_when_report::args::NestedWhenReportArgs),
    /// Report one-armed if forms with no else branch ((if test then) is (when test then)).
    OneArmedIf(one_armed_if_report::args::OneArmedIfReportArgs),
    /// Report an if whose test and then are the same atom ((if x x y) is (or x y)).
    IfToOr(if_to_or_report::args::IfToOrReportArgs),
    /// Report (apply #'f (list ...)) forms that are just (f ...) (a direct call).
    RedundantApply(redundant_apply_report::args::RedundantApplyReportArgs),
    /// Report an eql-defaulting call with an explicit :test #'eql ((find x l :test #'eql) is (find x l)).
    RedundantEqlTest(redundant_eql_test_report::args::RedundantEqlTestReportArgs),
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
    /// Report (funcall (lambda ...) ...) forms that apply the lambda directly (((lambda ...) ...)).
    FuncallLambda(funcall_lambda_report::args::FuncallLambdaReportArgs),
    /// Report #'(lambda ...) forms with a redundant #' prefix (#'(lambda ...) is (lambda ...)).
    SharpQuotedLambda(sharp_quoted_lambda_report::args::SharpQuotedLambdaReportArgs),
    /// Report single-operand and/or forms ((and X) and (or X) are just X).
    SingleOperandBoolean(single_operand_boolean_report::args::SingleOperandBooleanReportArgs),
    /// Report a cond with a single non-t clause that has a body ((cond (test body)) is (when test body)).
    SingleClauseCond(single_clause_cond_report::args::SingleClauseCondReportArgs),
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
