macro_rules! safe_text {
    ($value:expr) => {
        crate::presentation::cli::terminal_safe(&$value)
    };
}

mod analysis_report;
mod argv;
mod basic_edit;
mod capabilities;
mod command;
mod config;
mod config_bridge;
mod contract;
mod dependency_report;

// Phase 2 facade (section 4.1). args/shared/gate - and shared's io, diff and
// macos_acl submodules - now live in `paredit-core-cli`. `contract` stays here:
// it enumerates three features' capabilities, which makes it composition root
// (section 11.5.1).
use paredit_core_cli::{args, diagnosis, gate, messages, shared};
// Phase 3 facade: the composition root sees each slice's Args type and run fn.
use paredit_feature_change_summary::change_summary::cli as change_summary;
use paredit_feature_code_metrics::cohesion_report::cli as cohesion_report;
use paredit_feature_code_metrics::debt_score_report::cli as debt_score_report;
use paredit_feature_code_metrics::docstring_report::cli as docstring_report;
use paredit_feature_code_metrics::duplication_ratio_report::cli as duplication_ratio_report;
use paredit_feature_code_metrics::hotspot_report::cli as hotspot_report;
use paredit_feature_code_metrics::indentation_report::cli as indentation_report;
use paredit_feature_code_metrics::line_metrics_report::cli as line_metrics_report;
use paredit_feature_code_metrics::todo_report::cli as todo_report;
use paredit_feature_external_check::external_diagnostics_report::cli as external_diagnostics_report;
use paredit_feature_extract::extract_constant::cli as extract_constant;
use paredit_feature_extract::extract_function::cli as extract_function;
use paredit_feature_extract::extract_local_function::cli as extract_local_function;
use paredit_feature_form_transform::replace_forms::cli as replace_forms;
use paredit_feature_form_transform::thread_expression::cli as thread_expression;
use paredit_feature_form_transform::unthread_expression::cli as unthread_expression;
use paredit_feature_form_transform::unwrap_call::cli as unwrap_call;
use paredit_feature_inline::inline_function::cli as inline_function;
use paredit_feature_inline::inline_lambda::cli as inline_lambda;
use paredit_feature_inline::inline_let::cli as inline_let;
use paredit_feature_inline::inline_local_function::cli as inline_local_function;
use paredit_feature_inline::inline_symbol_macro::cli as inline_symbol_macro;
use paredit_feature_lisp_analysis::circular_literal_report::cli as circular_literal_report;
use paredit_feature_lisp_analysis::class_hierarchy_report::cli as class_hierarchy_report;
use paredit_feature_lisp_analysis::format_directive_report::cli as format_directive_report;
use paredit_feature_lisp_analysis::generic_dispatch_report::cli as generic_dispatch_report;
use paredit_feature_lisp_analysis::loop_report::cli as loop_report;
use paredit_feature_lisp_analysis::macro_expansion_report::cli as macro_expansion_report;
use paredit_feature_lisp_analysis::macro_hygiene_report::cli as macro_hygiene_report;
use paredit_feature_lisp_analysis::method_combination_report::cli as method_combination_report;
use paredit_feature_lisp_analysis::package_lock_report::cli as package_lock_report;
use paredit_feature_lisp_analysis::read_conditional_report::cli as read_conditional_report;
use paredit_feature_lisp_analysis::read_time_eval_report::cli as read_time_eval_report;
use paredit_feature_lisp_analysis::readtable_case_report::cli as readtable_case_report;
use paredit_feature_lisp_analysis::restart_report::cli as restart_report;
use paredit_feature_project_inventory::api_diff_report::cli as api_diff_report;
use paredit_feature_project_inventory::api_surface_report::cli as api_surface_report;
use paredit_feature_project_inventory::blame_report::cli as blame_report;
use paredit_feature_project_inventory::external_system_report::cli as external_system_report;
use paredit_feature_project_inventory::keyword_arity_report::cli as keyword_arity_report;
use paredit_feature_project_inventory::license_report::cli as license_report;
use paredit_feature_project_inventory::serial_consistency_report::cli as serial_consistency_report;
use paredit_feature_project_inventory::symbol_index_report::cli as symbol_index_report;
use paredit_feature_project_inventory::test_map_report::cli as test_map_report;
use paredit_feature_project_inventory::unreachable_expression_report::cli as unreachable_expression_report;
use paredit_feature_selector::resolve_report::cli as resolve_report;
use paredit_feature_semantic_report::constant_report::cli as constant_report;
use paredit_feature_semantic_report::effect_report::cli as effect_report;
use paredit_feature_semantic_report::narrowing_report::cli as narrowing_report;
use paredit_feature_semantic_report::type_report::cli as type_report;
use paredit_feature_semantic_report::value_propagation_report::cli as value_propagation_report;
use paredit_feature_similarity::clone_report::cli as clone_report;
use paredit_feature_similarity::duplicate_report::cli as duplicate_report;
use paredit_feature_similarity::similarity_report::cli as similarity_report;
use paredit_feature_structural_diff::structural_diff::cli as structural_diff;
use paredit_feature_structural_diff::structural_patch::cli as structural_patch;
mod dispatch;
mod duplicate_export_report;
mod duplicate_method_report;
mod duplicate_slot_report;
mod emacs_lisp_file_report;
mod lint_report;
mod shadowed_binding_report;
mod symbol_report;
mod unused_parameter_report;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Args, Parser};
use serde_json::json;

use args::*;
use command::Command;
pub(crate) use shared::{read_input_dialect_and_tree, terminal_safe, terminal_safe_error_chain};

#[derive(Debug, Parser)]
#[command(
    name = "paredit",
    version,
    about,
    long_about = None,
    after_help = "Canonical namespaces:\n  `paredit inspect ...` reads and reports without writing.\n  `paredit edit ...` transforms one selected form; stdout by default, --write to update the file.\n  `paredit refactor ...` plans, previews, verifies, and applies semantic changes.\n\nAll source-facing commands live in these three namespaces.\n`paredit completions <shell>` prints a shell completion script.\nRun `paredit inspect capabilities --output json` for a machine-readable catalog of every command and flag."
)]
struct Cli {
    /// Write nothing to disk. Suppresses --write on every command, whatever
    /// else the command line says, so a wrapper can append it unconditionally.
    #[arg(long, global = true)]
    dry_run: bool,
    /// Emit JSON Lines progress on stderr, one object per line, for
    /// operations long enough that silence looks like a hang.
    #[arg(long, global = true)]
    progress: bool,
    #[command(subcommand)]
    command: Command,
    #[command(flatten)]
    budget: BudgetArgs,
}

/// The bounds that apply to whichever command runs.
///
/// Declared once on the root and marked `global`, so every subcommand accepts
/// them and `inspect capabilities` lists them everywhere without 275 arg
/// structs repeating the same four flags. They are resolved before dispatch
/// and published process-wide; nothing downstream is handed a budget it could
/// forget to honour.
#[derive(Debug, Args)]
struct BudgetArgs {
    /// Wall-clock budget in milliseconds. Checked between files; unset means no budget.
    #[arg(long, global = true, value_name = "MILLIS")]
    timeout_ms: Option<u64>,
    /// Ceiling on one document read, for example 8MiB. May lower the default, never raise it.
    #[arg(long, global = true, value_name = "SIZE")]
    max_input_bytes: Option<String>,
    /// Ceiling on one file found by a directory scan. May lower the default, never raise it.
    #[arg(long, global = true, value_name = "SIZE")]
    max_file_bytes: Option<String>,
    /// Ceiling on the bytes one scan reads in total. May lower the default, never raise it.
    #[arg(long, global = true, value_name = "SIZE")]
    max_total_bytes: Option<String>,
    /// Ceiling on the files one scan may yield. May lower the default, never raise it.
    #[arg(long, global = true, value_name = "COUNT")]
    max_files: Option<usize>,
    /// Workers for multi-file analysis. 0 uses every core; 1 is fully serial.
    #[arg(long, global = true, value_name = "COUNT", default_value_t = 0)]
    jobs: usize,
}

/// The environment spelling of `--dry-run`, for wrapping a whole session.
const DRY_RUN_VAR: &str = "PAREDIT_DRY_RUN";

#[must_use]
pub fn run() -> ExitCode {
    let invocation: Vec<String> = std::env::args().collect();
    // `bootstrap` first, so a protocol server honours the repository's
    // `paredit.toml` exactly as a one-shot command does.
    let cli = bootstrap();
    if let Err(error) = install_budget(&cli.budget) {
        eprintln!("Error: {}", terminal_safe_error_chain(&error));
        // A rejected limit is a usage error: the command line, not the source,
        // is what has to change.
        return ExitCode::from(2);
    }

    // The protocol servers own their own exit status, and are therefore taken
    // before dispatch. A session normally ends with the client closing the
    // pipe, and routing that through the `Result` path below would report every
    // clean shutdown as an error.
    let command = match cli.command {
        Command::Lsp(args) => return crate::presentation::lsp::lsp(args),
        Command::Mcp(args) => return crate::presentation::mcp::mcp(args),
        Command::Serve(args) => return crate::presentation::serve::serve(args),
        command => command,
    };
    match dispatch::dispatch(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report_failure(&error, &invocation),
    }
}

/// Whether this run may not write.
fn is_dry_run(argv: &[String]) -> bool {
    argv.iter().any(|token| token == "--dry-run")
        || std::env::var(DRY_RUN_VAR).is_ok_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
}

/// Removes `--write` when the run is a dry run.
///
/// `--write` is the one flag every mutating command in this tool spells the
/// same way, which is what makes a single uniform `--dry-run` possible at all:
/// removing it is exactly "write nothing", on all ~80 mutating commands, with
/// no argument struct having to know the flag exists.
///
/// Done here rather than inside each command so a harness can append
/// `--dry-run` to a command line it did not construct and be certain. A
/// per-command opt-in would be a guarantee with 80 places to have missed.
fn suppress_writes_when_dry(argv: Vec<String>) -> Vec<String> {
    if !is_dry_run(&argv) {
        return argv;
    }

    let mut suppressed = false;
    let mut kept = Vec::with_capacity(argv.len());
    let mut past_separator = false;
    for token in argv {
        // After `--` everything is a file name, and one that happens to be
        // called `--write` is still a file name.
        if token == "--" {
            past_separator = true;
        }
        if !past_separator && token == "--write" {
            suppressed = true;
            continue;
        }
        kept.push(token);
    }

    // Never silent: the caller asked for a write and is not getting one.
    if suppressed {
        eprintln!(
            "Note: {}",
            messages::say(messages::Message::DryRunSuppressedWrite)
        );
    }
    kept
}

/// Prints a failure with its stable code and whatever would get past it, in
/// the format this invocation was already going to be read in.
fn report_failure(error: &anyhow::Error, invocation: &[String]) -> ExitCode {
    use clap::CommandFactory;

    let root = Cli::command();
    let context = diagnosis::Context {
        file: argv::long_flag_value(invocation, "file"),
        command_path: argv::resolve_leaf(invocation, &root).map(|(_, path)| path),
    };
    let diagnosis = diagnosis::diagnose(error, &context);

    // JSON on stderr when the report itself would have been JSON. An agent
    // that asked for a machine-readable answer should not have to parse
    // English to find out why it did not get one.
    if argv::effective_output_format(invocation, &root).as_deref() == Some("json") {
        let envelope = json!({
            "schema_version": 1,
            "status": "error",
            "command": context.command_path.as_deref(),
            "error": diagnosis.to_json(),
        });
        // Not wrapped in `terminal_safe`: the values inside were escaped when
        // the envelope was built, and escaping the serialized document would
        // turn its own newlines into `\u{a}` and stop it being JSON.
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| envelope.to_string())
        );
    } else {
        eprintln!(
            "{} [{}]: {}",
            messages::say(messages::Message::ErrorPrefix),
            diagnosis.code,
            terminal_safe_error_chain(error)
        );
        // Best-effort: the file may have moved or changed since the failure,
        // in which case there is nothing to point a caret at and this simply
        // prints no excerpt rather than a wrong one.
        if let Some(caret) = caret_for_failure(&diagnosis, context.file.as_deref()) {
            eprintln!("{caret}");
        }
        for repair in &diagnosis.repairs {
            match &repair.command {
                Some(command) => {
                    eprintln!(
                        "  {}: {} — {}",
                        messages::say(messages::Message::RepairPrefix),
                        repair.detail(),
                        terminal_safe(command)
                    );
                }
                None => eprintln!(
                    "  {}: {}",
                    messages::say(messages::Message::RepairPrefix),
                    repair.detail()
                ),
            }
        }
    }

    ExitCode::from(diagnosis.code.exit_code())
}

/// The caret excerpt for a failure, when it named a byte position and a file
/// to read it from.
///
/// Re-reads the file rather than reusing whatever the command already had in
/// memory: by the time a failure is reported, that buffer is gone, and a
/// second read is the same trade the rest of the repair suggestions already
/// make (`RepairRereadFile` and friends). A read that fails - the file moved,
/// permissions changed - is silently swallowed: the error already printed is
/// the operative one, and a second, unrelated I/O failure here would only
/// obscure it.
fn caret_for_failure(diagnosis: &diagnosis::Diagnosis, file: Option<&str>) -> Option<String> {
    let offset = diagnosis.offset?;
    let source = std::fs::read_to_string(file?).ok()?;
    diagnosis::render_caret(&source, offset)
}

/// Prints a warning — the run continues, but not quite as asked — in the
/// format this invocation was already going to be read in.
///
/// Mirrors [`report_failure`]'s JSON/text split for the same reason: a
/// caller that asked for `--output json` should not have to fall back to
/// parsing English stderr to learn a configuration file was ignored. Text
/// mode is unchanged from before this existed - `"Warning: {message}"` on
/// stderr - so this only adds a reading, never removes one.
fn report_warning(message: &str, argv: &[String]) {
    use clap::CommandFactory;

    let root = Cli::command();
    if argv::effective_output_format(argv, &root).as_deref() == Some("json") {
        let envelope = json!({
            "schema_version": 1,
            "status": "warning",
            "command": argv::resolve_leaf(argv, &root).map(|(_, path)| path),
            "warning": { "message": terminal_safe(message).to_string() },
        });
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| envelope.to_string())
        );
    } else {
        eprintln!("Warning: {}", terminal_safe(message));
    }
}

/// Reads the configuration, installs what acts below the argument layer, and
/// parses the command line the configuration has contributed to.
///
/// Ordered so that failure is always survivable. A configuration that cannot
/// be read leaves the command running unconfigured with a warning, rather than
/// refusing to run at all: `paredit config check` is where a broken file
/// should be diagnosed, and a tool that will not start is a bad place to learn
/// that a file two directories up has a typo.
fn bootstrap() -> Cli {
    use clap::CommandFactory;

    let argv = suppress_writes_when_dry(std::env::args().collect());
    let loaded = match config::workflow::load(&config::args::ConfigLocationArgs::default()) {
        Ok(loaded) => loaded,
        Err(error) => {
            report_warning(
                &format!(
                    "{}: {:#}",
                    messages::say(messages::Message::ConfigIgnored),
                    error
                ),
                &argv,
            );
            return Cli::parse_from(argv);
        }
    };

    let mut runtime = config::runtime::resolve(&loaded.settings);
    let raw: Vec<String> = std::env::args().collect();
    runtime.dry_run = is_dry_run(&raw);
    runtime.progress = raw.iter().any(|token| token == "--progress");
    paredit_core_cli::runtime::install(runtime);

    // A configuration with errors contributes nothing. Injecting from a file
    // that `config check` would reject turns a fixable diagnostic into a
    // baffling error about flags the caller never typed.
    if loaded.has_errors() {
        report_warning(messages::say(messages::Message::ConfigHasErrors), &argv);
        return Cli::parse_from(argv);
    }

    let augmented = config_bridge::augment(&argv, &loaded.settings, &Cli::command());
    if augmented.argv == argv {
        return Cli::parse_from(argv);
    }

    match Cli::try_parse_from(&augmented.argv) {
        Ok(cli) => cli,
        // The rewrite must never be the reason a command fails. Re-parsing the
        // original both drops the injection and reports against what the
        // caller actually typed. Naming the dropped keys matters: the command
        // is about to run with settings the caller believes are in force.
        Err(error) if error.use_stderr() => {
            let dropped = augmented
                .injected
                .iter()
                .map(|injection| injection.key)
                .collect::<Vec<_>>()
                .join(", ");
            report_warning(
                &format!(
                    "{}: {dropped}",
                    messages::say(messages::Message::ConfigKeysNotApplied)
                ),
                &argv,
            );
            Cli::parse_from(argv)
        }
        Err(error) => error.exit(),
    }
}

/// Resolves and publishes this run's bounds.
///
/// Flags win over the environment, and the environment exists because a CI
/// container's budget is not something the person typing the command controls.
/// Both are ratchets: either may lower a ceiling, neither may raise one.
fn install_budget(args: &BudgetArgs) -> Result<()> {
    use paredit_core_safety::deadline::{self, Deadline};
    use paredit_core_safety::limits::{self, ResourceLimitOverrides, ResourceLimits};

    let from_environment = limits::overrides_from_environment()?;
    let from_flags = ResourceLimitOverrides::from_text(
        args.max_input_bytes.as_deref(),
        args.max_file_bytes.as_deref(),
        args.max_total_bytes.as_deref(),
        args.max_files,
    )?;

    let resolved = ResourceLimits::default()
        .with_overrides(&from_environment)?
        .with_overrides(&from_flags)?;

    // `install` only fails when something already installed a value, which
    // cannot happen before dispatch in a `main` that calls this once. Ignoring
    // the result keeps a second entry point (a test harness, a future embedded
    // caller) from aborting the run over a benign race.
    let _ = limits::install(resolved);
    let _ = limits::install_jobs(args.jobs);
    let _ = deadline::install(Deadline::from_millis(args.timeout_ms));
    Ok(())
}

// Facade re-exports for extracted feature packages (section 4.1).
use paredit_feature_binding::convert_flet_to_labels::cli as convert_flet_to_labels;
use paredit_feature_binding::convert_labels_to_flet::cli as convert_labels_to_flet;
use paredit_feature_binding::convert_let_star_to_let::cli as convert_let_star_to_let;
use paredit_feature_binding::convert_let_to_let_star::cli as convert_let_to_let_star;
use paredit_feature_binding::convert_sequential_binding::cli as convert_sequential_binding;
use paredit_feature_binding::eliminate_empty_binding_form::cli as eliminate_empty_binding_form;
use paredit_feature_binding::flatten_progn::cli as flatten_progn;
use paredit_feature_binding::introduce_let::cli as introduce_let;
use paredit_feature_binding::let_report::cli as let_report;
use paredit_feature_binding::merge_nested_flet::cli as merge_nested_flet;
use paredit_feature_binding::merge_nested_let::cli as merge_nested_let;
use paredit_feature_binding::merge_nested_let_star::cli as merge_nested_let_star;
use paredit_feature_binding::split_let::cli as split_let;
use paredit_feature_binding::split_let_star::cli as split_let_star;
use paredit_feature_conditional_conversion::convert_cond_to_if::cli as convert_cond_to_if;
use paredit_feature_conditional_conversion::convert_if_to_cond::cli as convert_if_to_cond;
use paredit_feature_conditional_conversion::convert_if_to_unless::cli as convert_if_to_unless;
use paredit_feature_conditional_conversion::convert_if_to_when::cli as convert_if_to_when;
use paredit_feature_conditional_conversion::convert_unless_to_if::cli as convert_unless_to_if;
use paredit_feature_conditional_conversion::convert_when_to_if::cli as convert_when_to_if;
use paredit_feature_function_parameter::function_parameter::cli as function_parameter;
use paredit_feature_inline::inline_literal_constant::cli as inline_literal_constant;
use paredit_feature_lint_conditional::case_nil_key::cli as case_nil_key_report;
use paredit_feature_lint_conditional::cond_t_clause::cli as cond_t_clause_report;
use paredit_feature_lint_conditional::constant_if_test::cli as constant_if_test_report;
use paredit_feature_lint_conditional::constant_when_test::cli as constant_when_test_report;
use paredit_feature_lint_conditional::de_morgan::cli as de_morgan_report;
use paredit_feature_lint_conditional::dead_boolean_operand::cli as dead_boolean_operand_report;
use paredit_feature_lint_conditional::duplicate_boolean_operands::cli as duplicate_boolean_operand_report;
use paredit_feature_lint_conditional::duplicate_case_keys::cli as duplicate_case_key_report;
use paredit_feature_lint_conditional::duplicate_cond_tests::cli as duplicate_cond_test_report;
use paredit_feature_lint_conditional::empty_body::cli as empty_body_report;
use paredit_feature_lint_conditional::exhaustive_case_otherwise::cli as exhaustive_case_otherwise_report;
use paredit_feature_lint_conditional::identical_if_branches::cli as identical_if_branch_report;
use paredit_feature_lint_conditional::if_arity::cli as if_arity_report;
use paredit_feature_lint_conditional::if_not::cli as if_not_report;
use paredit_feature_lint_conditional::if_to_or::cli as if_to_or_report;
use paredit_feature_lint_conditional::if_to_unless::cli as if_to_unless_report;
use paredit_feature_lint_conditional::malformed_case_clause::cli as malformed_case_clause_report;
use paredit_feature_lint_conditional::malformed_cond_clause::cli as malformed_cond_clause_report;
use paredit_feature_lint_conditional::negated_comparison::cli as negated_comparison_report;
use paredit_feature_lint_conditional::negated_if::cli as negated_if_report;
use paredit_feature_lint_conditional::negated_when_unless::cli as negated_when_unless_report;
use paredit_feature_lint_conditional::nested_boolean::cli as nested_boolean_report;
use paredit_feature_lint_conditional::nested_unless::cli as nested_unless_report;
use paredit_feature_lint_conditional::nested_when::cli as nested_when_report;
use paredit_feature_lint_conditional::one_armed_if::cli as one_armed_if_report;
use paredit_feature_lint_conditional::quoted_case_key::cli as quoted_case_key_report;
use paredit_feature_lint_conditional::redundant_boolean_identity::cli as redundant_boolean_identity_report;
use paredit_feature_lint_conditional::redundant_if_nil::cli as redundant_if_nil_report;
use paredit_feature_lint_conditional::single_clause_cond::cli as single_clause_cond_report;
use paredit_feature_lint_conditional::single_operand_boolean::cli as single_operand_boolean_report;
use paredit_feature_lint_conditional::typecase_nil_key::cli as typecase_nil_key_report;
use paredit_feature_lint_conditional::unreachable_case_clause::cli as unreachable_case_clause_report;
use paredit_feature_lint_conditional::unreachable_cond_clause::cli as unreachable_cond_clause_report;
use paredit_feature_lint_control_flow::binds_constant::cli as binds_constant_report;
use paredit_feature_lint_control_flow::eval_when_situation::cli as eval_when_situation_report;
use paredit_feature_lint_control_flow::explicit_nil_return::cli as explicit_nil_return_report;
use paredit_feature_lint_control_flow::handler_case_no_clauses::cli as handler_case_no_clauses_report;
use paredit_feature_lint_control_flow::malformed_iteration_spec::cli as malformed_iteration_spec_report;
use paredit_feature_lint_control_flow::nested_progn::cli as nested_progn_report;
use paredit_feature_lint_control_flow::prog2_to_progn::cli as prog2_to_progn_report;
use paredit_feature_lint_control_flow::redundant_body_progn::cli as redundant_body_progn_report;
use paredit_feature_lint_control_flow::redundant_prog1::cli as redundant_prog1_report;
use paredit_feature_lint_control_flow::redundant_progn::cli as redundant_progn_report;
use paredit_feature_lint_control_flow::unwind_protect_no_cleanup::cli as unwind_protect_no_cleanup_report;
use paredit_feature_lint_form_shape::butlast_default_count::cli as butlast_default_count_report;
use paredit_feature_lint_form_shape::coerce_to_t::cli as coerce_to_t_report;
use paredit_feature_lint_form_shape::defpackage_quoted::cli as defpackage_quoted_report;
use paredit_feature_lint_form_shape::duplicate_keyword::cli as duplicate_keyword_report;
use paredit_feature_lint_form_shape::duplicate_lambda_list_keyword::cli as duplicate_lambda_list_keyword_report;
use paredit_feature_lint_form_shape::duplicate_let_bindings::cli as duplicate_let_binding_report;
use paredit_feature_lint_form_shape::duplicate_parameters::cli as duplicate_parameter_report;
use paredit_feature_lint_form_shape::duplicate_setf_places::cli as duplicate_setf_place_report;
use paredit_feature_lint_form_shape::empty_let::cli as empty_let_report;
use paredit_feature_lint_form_shape::funcall_lambda::cli as funcall_lambda_report;
use paredit_feature_lint_form_shape::getf_default_nil::cli as getf_default_nil_report;
use paredit_feature_lint_form_shape::gethash_default::cli as gethash_default_report;
use paredit_feature_lint_form_shape::lambda_list_keyword_order::cli as lambda_list_keyword_order_report;
use paredit_feature_lint_form_shape::make_array_default_keyword::cli as make_array_default_keyword_report;
use paredit_feature_lint_form_shape::make_hash_table_test::cli as make_hash_table_test_report;
use paredit_feature_lint_form_shape::make_list_default_element::cli as make_list_default_element_report;
use paredit_feature_lint_form_shape::malformed_let_binding::cli as malformed_let_binding_report;
use paredit_feature_lint_form_shape::manual_incf::cli as manual_incf_report;
use paredit_feature_lint_form_shape::manual_push::cli as manual_push_report;
use paredit_feature_lint_form_shape::manual_pushnew::cli as manual_pushnew_report;
use paredit_feature_lint_form_shape::multiple_value_list_of_values::cli as multiple_value_list_of_values_report;
use paredit_feature_lint_form_shape::nested_char_case::cli as nested_char_case_report;
use paredit_feature_lint_form_shape::nested_cxr::cli as nested_cxr_report;
use paredit_feature_lint_form_shape::parse_integer_default_radix::cli as parse_integer_default_radix_report;
use paredit_feature_lint_form_shape::redundant_apply::cli as redundant_apply_report;
use paredit_feature_lint_form_shape::redundant_funcall::cli as redundant_funcall_report;
use paredit_feature_lint_form_shape::redundant_identity::cli as redundant_identity_report;
use paredit_feature_lint_form_shape::redundant_let_star::cli as redundant_let_star_report;
use paredit_feature_lint_form_shape::redundant_quote::cli as redundant_quote_report;
use paredit_feature_lint_form_shape::redundant_the::cli as redundant_the_report;
use paredit_feature_lint_form_shape::self_assignment::cli as self_assignment_report;
use paredit_feature_lint_form_shape::setf_arity::cli as setf_arity_report;
use paredit_feature_lint_form_shape::setq_non_variable::cli as setq_non_variable_report;
use paredit_feature_lint_form_shape::sharp_quoted_lambda::cli as sharp_quoted_lambda_report;
use paredit_feature_lint_form_shape::single_value_bind::cli as single_value_bind_report;
use paredit_feature_lint_form_shape::the_arity::cli as the_arity_report;
use paredit_feature_lint_form_shape::typep_predicate::cli as typep_predicate_report;
use paredit_feature_lint_form_shape::values_list_of_list::cli as values_list_of_list_report;
use paredit_feature_lint_numeric::eq_char_comparison::cli as eq_char_comparison_report;
use paredit_feature_lint_numeric::eq_number_comparison::cli as eq_number_comparison_report;
use paredit_feature_lint_numeric::eql_list_comparison::cli as eql_list_comparison_report;
use paredit_feature_lint_numeric::eql_string_comparison::cli as eql_string_comparison_report;
use paredit_feature_lint_numeric::equality_arity::cli as equality_arity_report;
use paredit_feature_lint_numeric::explicit_step_delta::cli as explicit_step_delta_report;
use paredit_feature_lint_numeric::identity_arithmetic::cli as identity_arithmetic_report;
use paredit_feature_lint_numeric::literal_place::cli as literal_place_report;
use paredit_feature_lint_numeric::modify_macro_arity::cli as modify_macro_arity_report;
use paredit_feature_lint_numeric::negated_step_delta::cli as negated_step_delta_report;
use paredit_feature_lint_numeric::nil_comparison::cli as nil_comparison_report;
use paredit_feature_lint_numeric::one_step_arithmetic::cli as one_step_arithmetic_report;
use paredit_feature_lint_numeric::redundant_divisor::cli as redundant_divisor_report;
use paredit_feature_lint_numeric::self_comparison::cli as self_comparison_report;
use paredit_feature_lint_numeric::sign_comparison::cli as sign_comparison_report;
use paredit_feature_lint_numeric::single_arg_comparison::cli as single_arg_comparison_report;
use paredit_feature_lint_numeric::single_operand_arithmetic::cli as single_operand_arithmetic_report;
use paredit_feature_lint_numeric::step_zero::cli as step_zero_report;
use paredit_feature_lint_numeric::t_comparison::cli as t_comparison_report;
use paredit_feature_lint_numeric::verbose_negation::cli as verbose_negation_report;
use paredit_feature_lint_numeric::zero_divisor::cli as zero_divisor_report;
use paredit_feature_lint_sequence::accessor_arity::cli as accessor_arity_report;
use paredit_feature_lint_sequence::append_list_to_cons::cli as append_list_to_cons_report;
use paredit_feature_lint_sequence::append_nil::cli as append_nil_report;
use paredit_feature_lint_sequence::car_nthcdr::cli as car_nthcdr_report;
use paredit_feature_lint_sequence::car_reverse::cli as car_reverse_report;
use paredit_feature_lint_sequence::cons_to_list::cli as cons_to_list_report;
use paredit_feature_lint_sequence::destructive_literal::cli as destructive_literal_report;
use paredit_feature_lint_sequence::double_reverse::cli as double_reverse_report;
use paredit_feature_lint_sequence::eql_search_literal::cli as eql_search_literal_report;
use paredit_feature_lint_sequence::last_default_count::cli as last_default_count_report;
use paredit_feature_lint_sequence::list_star_nil::cli as list_star_nil_report;
use paredit_feature_lint_sequence::list_star_to_cons::cli as list_star_to_cons_report;
use paredit_feature_lint_sequence::nth_constant_index::cli as nth_constant_index_report;
use paredit_feature_lint_sequence::nthcdr_small_index::cli as nthcdr_small_index_report;
use paredit_feature_lint_sequence::nthcdr_zero::cli as nthcdr_zero_report;
use paredit_feature_lint_sequence::redundant_count_nil::cli as redundant_count_nil_report;
use paredit_feature_lint_sequence::redundant_end_nil::cli as redundant_end_nil_report;
use paredit_feature_lint_sequence::redundant_eql_test::cli as redundant_eql_test_report;
use paredit_feature_lint_sequence::redundant_from_end_nil::cli as redundant_from_end_nil_report;
use paredit_feature_lint_sequence::redundant_identity_key::cli as redundant_identity_key_report;
use paredit_feature_lint_sequence::redundant_start_zero::cli as redundant_start_zero_report;
use paredit_feature_lint_sequence::single_operand_list_op::cli as single_operand_list_op_report;
use paredit_feature_lint_sequence::subseq_zero::cli as subseq_zero_report;
use paredit_feature_lint_string_char::char_case_fold::cli as char_case_fold_report;
use paredit_feature_lint_string_char::char_op_string::cli as char_op_string_report;
use paredit_feature_lint_string_char::code_char_char_code::cli as code_char_char_code_report;
use paredit_feature_lint_string_char::format_missing_destination::cli as format_missing_destination_report;
use paredit_feature_lint_string_char::format_newline::cli as format_newline_report;
use paredit_feature_lint_string_char::format_to_string::cli as format_to_string_report;
use paredit_feature_lint_string_char::nested_string_case::cli as nested_string_case_report;
use paredit_feature_lint_string_char::string_case_fold::cli as string_case_fold_report;
use paredit_feature_package::package::cli as package;
use paredit_feature_package::package_boundary_report::cli as package_boundary_report;
use paredit_feature_package::package_conflict_report::cli as package_conflict_report;
use paredit_feature_package::system_conflict_report::cli as system_conflict_report;
use paredit_feature_package::unused_export_report::cli as unused_export_report;
use paredit_feature_package::unused_nickname_report::cli as unused_nickname_report;
use paredit_feature_package::unused_package_report::cli as unused_package_report;
use paredit_feature_project_analysis::call_cycle_report::cli as call_cycle_report;
use paredit_feature_project_analysis::call_graph_report::cli as call_graph_report;
use paredit_feature_project_analysis::call_report::cli as call_report;
use paredit_feature_project_analysis::class_cycle_report::cli as class_cycle_report;
use paredit_feature_project_analysis::complexity_report::cli as complexity_report;
use paredit_feature_project_analysis::context_report::cli as context_report;
use paredit_feature_project_analysis::form_report::cli as form_report;
use paredit_feature_project_analysis::impact_report::cli as impact_report;
use paredit_feature_project_analysis::naming_report::cli as naming_report;
use paredit_feature_project_analysis::package_cycle_report::cli as package_cycle_report;
use paredit_feature_project_analysis::reachability_report::cli as reachability_report;
use paredit_feature_project_analysis::redefinition_report::cli as redefinition_report;
use paredit_feature_project_analysis::signature_report::cli as signature_report;
use paredit_feature_project_analysis::source_report::cli as source_report;
use paredit_feature_project_analysis::struct_cycle_report::cli as struct_cycle_report;
use paredit_feature_project_analysis::system_cycle_report::cli as system_cycle_report;
use paredit_feature_project_analysis::undefined_package_report::cli as undefined_package_report;
use paredit_feature_project_analysis::unused_local_callable_report::cli as unused_local_callable_report;
use paredit_feature_project_analysis::workspace_report::cli as workspace_report;
use paredit_feature_refactor_workflow::refactor::cli as refactor;
use paredit_feature_refactor_workflow::refactor_step::cli as refactor_step;
use paredit_feature_remove_unused::definition_movement::cli as definition_movement;
use paredit_feature_remove_unused::definition_removal::cli as definition_removal;
use paredit_feature_remove_unused::definition_report::cli as definition_report;
use paredit_feature_remove_unused::remove_unused_binding::cli as remove_unused_binding;
use paredit_feature_remove_unused::remove_unused_control::cli as remove_unused_control;
use paredit_feature_rename::rename::cli as rename;
use paredit_feature_rename::rename_control::cli as rename_control;

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
