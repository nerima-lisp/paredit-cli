#![doc = include_str!("../README.md")]

pub mod butlast_default_count;
pub mod coerce_to_t;
pub mod defpackage_quoted;
pub mod destructuring_bind_unused_whole;
pub mod duplicate_keyword;
pub mod duplicate_lambda_list_keyword;
pub mod duplicate_let_bindings;
pub mod duplicate_parameters;
pub mod duplicate_setf_places;
pub mod empty_let;
pub mod flet_single_use_inlinable;
pub mod ftype_values_arity_mismatch;
pub mod funcall_lambda;
pub mod getf_default_nil;
pub mod gethash_default;
pub mod giant_conditional_form;
pub mod lambda_list_keyword_order;
pub mod loop_collect_into_immediately_returned;
pub mod make_array_default_keyword;
pub mod make_hash_table_test;
pub mod make_list_default_element;
pub mod malformed_let_binding;
pub mod manual_incf;
pub mod manual_push;
pub mod manual_pushnew;
pub mod multiple_value_list_of_values;
pub mod multiple_value_setq_arity_mismatch;
pub mod nested_char_case;
pub mod nested_cxr;
pub mod package_level_shadowing;
pub mod parse_integer_default_radix;
pub mod quoted_form_contains_stray_unquote;
pub mod redundant_apply;
pub mod redundant_funcall;
pub mod redundant_identity;
pub mod redundant_let_star;
pub mod redundant_quote;
pub mod redundant_the;
pub mod self_assignment;
pub mod setf_arity;
pub mod setq_non_variable;
pub mod sharp_quoted_lambda;
pub mod single_value_bind;
pub mod support;

#[cfg(test)]
mod quote_guard_tests;
#[cfg(test)]
mod reader_prefix_fix_tests;
pub mod the_arity;
pub mod typep_predicate;
pub mod values_list_of_list;
pub mod with_accessors_empty_binding_list;
pub mod with_open_file_redundant_direction_default;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// The eight rules added in this batch, driven through the *engine* rather
/// than through their own `build_*_report`.
///
/// The two entry points do not share their quote handling, and neither covers
/// the other. A report walks with [`crate::support::for_each_evaluated_subview`],
/// which never visits data at all; a head-filtered rule is handed matched nodes
/// by the dispatcher *including* the ones inside `'(…)`, and depends on each
/// `check`'s [`crate::support::is_unevaluated_at`] call to decline them. Testing
/// only the report would leave that call — the one thing standing between seven
/// of these rules and a finding on every quoted example in a macro's
/// documentation — unexercised.
///
/// Running the real pass also covers the two declarations a domain test cannot
/// see: each rule's `HeadFilter::Heads` list and its `RuleDialectScope`. A
/// wrong head list passes every `examine()` test while being unreachable from
/// the CLI.
#[cfg(test)]
mod engine_pass_tests;
