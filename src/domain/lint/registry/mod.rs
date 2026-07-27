//! The one place a lint rule is registered.
//!
//! `RULES`, `RULE_DOCS`, `FIXABLE_RULES`, and `WARNING_RULES` used to be four
//! hand-maintained parallel arrays that a new rule had to be threaded through
//! in lockstep. They are now derived from this array at compile time (see
//! [`catalog`]), so the only way to add a rule is to add its module and one
//! line here — and the only way for the catalogue to disagree with itself is
//! for the derivation to be wrong, which a `const` assertion catches.

pub mod catalog;

use super::rule::RuleEntry;
use super::rules;

/// How many rules the suite ships. Pinned so that adding or losing a rule is a
/// deliberate, reviewed change rather than a silent drift in the catalogue.
pub const RULE_COUNT: usize = 134;

/// Every rule, in report order: findings are grouped by this order, and the
/// public `RULES`/`RULE_DOCS` arrays preserve it.
pub const REGISTRY: [RuleEntry; RULE_COUNT] = [
    RuleEntry::new(&rules::self_assignment::META, &rules::self_assignment::RULE),
    RuleEntry::new(
        &rules::duplicate_setf_places::META,
        &rules::duplicate_setf_places::RULE,
    ),
    RuleEntry::new(&rules::setf_arity::META, &rules::setf_arity::RULE),
    RuleEntry::new(
        &rules::setq_non_variable::META,
        &rules::setq_non_variable::RULE,
    ),
    RuleEntry::new(&rules::manual_incf::META, &rules::manual_incf::RULE),
    RuleEntry::new(&rules::manual_push::META, &rules::manual_push::RULE),
    RuleEntry::new(&rules::manual_pushnew::META, &rules::manual_pushnew::RULE),
    RuleEntry::new(
        &paredit_feature_lint_numeric::explicit_step_delta::rule::META,
        &paredit_feature_lint_numeric::explicit_step_delta::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::negated_step_delta::rule::META,
        &paredit_feature_lint_numeric::negated_step_delta::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_control_flow::explicit_nil_return::rule::META,
        &paredit_feature_lint_control_flow::explicit_nil_return::rule::RULE,
    ),
    RuleEntry::new(&rules::cons_to_list::META, &rules::cons_to_list::RULE),
    RuleEntry::new(&rules::double_reverse::META, &rules::double_reverse::RULE),
    RuleEntry::new(
        &paredit_feature_lint_numeric::modify_macro_arity::rule::META,
        &paredit_feature_lint_numeric::modify_macro_arity::rule::RULE,
    ),
    RuleEntry::new(
        &rules::duplicate_parameters::META,
        &rules::duplicate_parameters::RULE,
    ),
    RuleEntry::new(
        &rules::duplicate_lambda_list_keyword::META,
        &rules::duplicate_lambda_list_keyword::RULE,
    ),
    RuleEntry::new(
        &rules::lambda_list_keyword_order::META,
        &rules::lambda_list_keyword_order::RULE,
    ),
    RuleEntry::new(&rules::redundant_quote::META, &rules::redundant_quote::RULE),
    RuleEntry::new(
        &paredit_feature_lint_control_flow::redundant_progn::rule::META,
        &paredit_feature_lint_control_flow::redundant_progn::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_control_flow::nested_progn::rule::META,
        &paredit_feature_lint_control_flow::nested_progn::rule::RULE,
    ),
    RuleEntry::new(&rules::nested_when::META, &rules::nested_when::RULE),
    RuleEntry::new(&rules::nested_unless::META, &rules::nested_unless::RULE),
    RuleEntry::new(&rules::nested_boolean::META, &rules::nested_boolean::RULE),
    RuleEntry::new(&rules::nested_cxr::META, &rules::nested_cxr::RULE),
    RuleEntry::new(
        &rules::nth_constant_index::META,
        &rules::nth_constant_index::RULE,
    ),
    RuleEntry::new(&rules::nthcdr_zero::META, &rules::nthcdr_zero::RULE),
    RuleEntry::new(
        &rules::nthcdr_small_index::META,
        &rules::nthcdr_small_index::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_control_flow::redundant_body_progn::rule::META,
        &paredit_feature_lint_control_flow::redundant_body_progn::rule::RULE,
    ),
    RuleEntry::new(&rules::empty_let::META, &rules::empty_let::RULE),
    RuleEntry::new(
        &rules::redundant_if_nil::META,
        &rules::redundant_if_nil::RULE,
    ),
    RuleEntry::new(
        &rules::redundant_let_star::META,
        &rules::redundant_let_star::RULE,
    ),
    RuleEntry::new(
        &rules::redundant_funcall::META,
        &rules::redundant_funcall::RULE,
    ),
    RuleEntry::new(&rules::redundant_the::META, &rules::redundant_the::RULE),
    RuleEntry::new(&rules::funcall_lambda::META, &rules::funcall_lambda::RULE),
    RuleEntry::new(
        &rules::sharp_quoted_lambda::META,
        &rules::sharp_quoted_lambda::RULE,
    ),
    RuleEntry::new(
        &rules::redundant_identity::META,
        &rules::redundant_identity::RULE,
    ),
    RuleEntry::new(&rules::redundant_apply::META, &rules::redundant_apply::RULE),
    RuleEntry::new(
        &rules::redundant_eql_test::META,
        &rules::redundant_eql_test::RULE,
    ),
    RuleEntry::new(
        &rules::redundant_identity_key::META,
        &rules::redundant_identity_key::RULE,
    ),
    RuleEntry::new(
        &rules::negated_when_unless::META,
        &rules::negated_when_unless::RULE,
    ),
    RuleEntry::new(
        &rules::negated_comparison::META,
        &rules::negated_comparison::RULE,
    ),
    RuleEntry::new(&rules::negated_if::META, &rules::negated_if::RULE),
    RuleEntry::new(&rules::if_to_or::META, &rules::if_to_or::RULE),
    RuleEntry::new(&rules::if_not::META, &rules::if_not::RULE),
    RuleEntry::new(
        &paredit_feature_lint_numeric::one_step_arithmetic::rule::META,
        &paredit_feature_lint_numeric::one_step_arithmetic::rule::RULE,
    ),
    RuleEntry::new(&rules::one_armed_if::META, &rules::one_armed_if::RULE),
    RuleEntry::new(
        &paredit_feature_lint_numeric::self_comparison::rule::META,
        &paredit_feature_lint_numeric::self_comparison::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::nil_comparison::rule::META,
        &paredit_feature_lint_numeric::nil_comparison::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::t_comparison::rule::META,
        &paredit_feature_lint_numeric::t_comparison::rule::RULE,
    ),
    RuleEntry::new(
        &rules::identical_if_branches::META,
        &rules::identical_if_branches::RULE,
    ),
    RuleEntry::new(
        &rules::constant_if_test::META,
        &rules::constant_if_test::RULE,
    ),
    RuleEntry::new(
        &rules::constant_when_test::META,
        &rules::constant_when_test::RULE,
    ),
    RuleEntry::new(&rules::if_arity::META, &rules::if_arity::RULE),
    RuleEntry::new(&rules::the_arity::META, &rules::the_arity::RULE),
    RuleEntry::new(
        &paredit_feature_lint_numeric::equality_arity::rule::META,
        &paredit_feature_lint_numeric::equality_arity::rule::RULE,
    ),
    RuleEntry::new(&rules::accessor_arity::META, &rules::accessor_arity::RULE),
    RuleEntry::new(
        &rules::append_list_to_cons::META,
        &rules::append_list_to_cons::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_string_char::format_to_string::rule::META,
        &paredit_feature_lint_string_char::format_to_string::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_string_char::format_newline::rule::META,
        &paredit_feature_lint_string_char::format_newline::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::redundant_divisor::rule::META,
        &paredit_feature_lint_numeric::redundant_divisor::rule::RULE,
    ),
    RuleEntry::new(
        &rules::duplicate_case_keys::META,
        &rules::duplicate_case_keys::RULE,
    ),
    RuleEntry::new(&rules::quoted_case_key::META, &rules::quoted_case_key::RULE),
    RuleEntry::new(&rules::case_nil_key::META, &rules::case_nil_key::RULE),
    RuleEntry::new(
        &rules::typecase_nil_key::META,
        &rules::typecase_nil_key::RULE,
    ),
    RuleEntry::new(
        &rules::malformed_case_clause::META,
        &rules::malformed_case_clause::RULE,
    ),
    RuleEntry::new(
        &rules::unreachable_case_clause::META,
        &rules::unreachable_case_clause::RULE,
    ),
    RuleEntry::new(
        &rules::exhaustive_case_otherwise::META,
        &rules::exhaustive_case_otherwise::RULE,
    ),
    RuleEntry::new(
        &rules::duplicate_cond_tests::META,
        &rules::duplicate_cond_tests::RULE,
    ),
    RuleEntry::new(
        &rules::unreachable_cond_clause::META,
        &rules::unreachable_cond_clause::RULE,
    ),
    RuleEntry::new(
        &rules::malformed_cond_clause::META,
        &rules::malformed_cond_clause::RULE,
    ),
    RuleEntry::new(
        &rules::duplicate_let_bindings::META,
        &rules::duplicate_let_bindings::RULE,
    ),
    RuleEntry::new(
        &rules::malformed_let_binding::META,
        &rules::malformed_let_binding::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_control_flow::binds_constant::rule::META,
        &paredit_feature_lint_control_flow::binds_constant::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_control_flow::malformed_iteration_spec::rule::META,
        &paredit_feature_lint_control_flow::malformed_iteration_spec::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_control_flow::eval_when_situation::rule::META,
        &paredit_feature_lint_control_flow::eval_when_situation::rule::RULE,
    ),
    RuleEntry::new(
        &rules::duplicate_boolean_operands::META,
        &rules::duplicate_boolean_operands::RULE,
    ),
    RuleEntry::new(
        &rules::dead_boolean_operand::META,
        &rules::dead_boolean_operand::RULE,
    ),
    RuleEntry::new(
        &rules::redundant_boolean_identity::META,
        &rules::redundant_boolean_identity::RULE,
    ),
    RuleEntry::new(&rules::de_morgan::META, &rules::de_morgan::RULE),
    RuleEntry::new(
        &rules::single_operand_boolean::META,
        &rules::single_operand_boolean::RULE,
    ),
    RuleEntry::new(
        &rules::single_operand_list_op::META,
        &rules::single_operand_list_op::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::single_operand_arithmetic::rule::META,
        &paredit_feature_lint_numeric::single_operand_arithmetic::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::eql_string_comparison::rule::META,
        &paredit_feature_lint_numeric::eql_string_comparison::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::eql_list_comparison::rule::META,
        &paredit_feature_lint_numeric::eql_list_comparison::rule::RULE,
    ),
    RuleEntry::new(
        &rules::eql_search_literal::META,
        &rules::eql_search_literal::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::eq_number_comparison::rule::META,
        &paredit_feature_lint_numeric::eq_number_comparison::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::eq_char_comparison::rule::META,
        &paredit_feature_lint_numeric::eq_char_comparison::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::single_arg_comparison::rule::META,
        &paredit_feature_lint_numeric::single_arg_comparison::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::sign_comparison::rule::META,
        &paredit_feature_lint_numeric::sign_comparison::rule::RULE,
    ),
    RuleEntry::new(
        &rules::single_clause_cond::META,
        &rules::single_clause_cond::RULE,
    ),
    RuleEntry::new(&rules::cond_t_clause::META, &rules::cond_t_clause::RULE),
    RuleEntry::new(
        &rules::single_value_bind::META,
        &rules::single_value_bind::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_string_char::format_missing_destination::rule::META,
        &paredit_feature_lint_string_char::format_missing_destination::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::literal_place::rule::META,
        &paredit_feature_lint_numeric::literal_place::rule::RULE,
    ),
    RuleEntry::new(
        &rules::destructive_literal::META,
        &rules::destructive_literal::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_string_char::char_op_string::rule::META,
        &paredit_feature_lint_string_char::char_op_string::rule::RULE,
    ),
    RuleEntry::new(&rules::empty_body::META, &rules::empty_body::RULE),
    RuleEntry::new(
        &paredit_feature_lint_numeric::identity_arithmetic::rule::META,
        &paredit_feature_lint_numeric::identity_arithmetic::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::verbose_negation::rule::META,
        &paredit_feature_lint_numeric::verbose_negation::rule::RULE,
    ),
    RuleEntry::new(
        &rules::list_star_to_cons::META,
        &rules::list_star_to_cons::RULE,
    ),
    RuleEntry::new(
        &rules::values_list_of_list::META,
        &rules::values_list_of_list::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_control_flow::redundant_prog1::rule::META,
        &paredit_feature_lint_control_flow::redundant_prog1::rule::RULE,
    ),
    RuleEntry::new(&rules::subseq_zero::META, &rules::subseq_zero::RULE),
    RuleEntry::new(&rules::car_nthcdr::META, &rules::car_nthcdr::RULE),
    RuleEntry::new(&rules::car_reverse::META, &rules::car_reverse::RULE),
    RuleEntry::new(&rules::append_nil::META, &rules::append_nil::RULE),
    RuleEntry::new(
        &rules::multiple_value_list_of_values::META,
        &rules::multiple_value_list_of_values::RULE,
    ),
    RuleEntry::new(&rules::typep_predicate::META, &rules::typep_predicate::RULE),
    RuleEntry::new(&rules::coerce_to_t::META, &rules::coerce_to_t::RULE),
    RuleEntry::new(&rules::gethash_default::META, &rules::gethash_default::RULE),
    RuleEntry::new(
        &rules::make_hash_table_test::META,
        &rules::make_hash_table_test::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::zero_divisor::rule::META,
        &paredit_feature_lint_numeric::zero_divisor::rule::RULE,
    ),
    RuleEntry::new(
        &rules::duplicate_keyword::META,
        &rules::duplicate_keyword::RULE,
    ),
    RuleEntry::new(
        &rules::defpackage_quoted::META,
        &rules::defpackage_quoted::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_numeric::step_zero::rule::META,
        &paredit_feature_lint_numeric::step_zero::rule::RULE,
    ),
    RuleEntry::new(&rules::if_to_unless::META, &rules::if_to_unless::RULE),
    RuleEntry::new(
        &paredit_feature_lint_control_flow::prog2_to_progn::rule::META,
        &paredit_feature_lint_control_flow::prog2_to_progn::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_control_flow::handler_case_no_clauses::rule::META,
        &paredit_feature_lint_control_flow::handler_case_no_clauses::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_control_flow::unwind_protect_no_cleanup::rule::META,
        &paredit_feature_lint_control_flow::unwind_protect_no_cleanup::rule::RULE,
    ),
    RuleEntry::new(
        &rules::redundant_start_zero::META,
        &rules::redundant_start_zero::RULE,
    ),
    RuleEntry::new(
        &rules::redundant_end_nil::META,
        &rules::redundant_end_nil::RULE,
    ),
    RuleEntry::new(
        &rules::redundant_from_end_nil::META,
        &rules::redundant_from_end_nil::RULE,
    ),
    RuleEntry::new(
        &rules::redundant_count_nil::META,
        &rules::redundant_count_nil::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_string_char::string_case_fold::rule::META,
        &paredit_feature_lint_string_char::string_case_fold::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_string_char::char_case_fold::rule::META,
        &paredit_feature_lint_string_char::char_case_fold::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_string_char::nested_string_case::rule::META,
        &paredit_feature_lint_string_char::nested_string_case::rule::RULE,
    ),
    RuleEntry::new(
        &paredit_feature_lint_string_char::code_char_char_code::rule::META,
        &paredit_feature_lint_string_char::code_char_char_code::rule::RULE,
    ),
    RuleEntry::new(
        &rules::last_default_count::META,
        &rules::last_default_count::RULE,
    ),
    RuleEntry::new(
        &rules::butlast_default_count::META,
        &rules::butlast_default_count::RULE,
    ),
    RuleEntry::new(
        &rules::make_list_default_element::META,
        &rules::make_list_default_element::RULE,
    ),
    RuleEntry::new(
        &rules::parse_integer_default_radix::META,
        &rules::parse_integer_default_radix::RULE,
    ),
    RuleEntry::new(
        &rules::getf_default_nil::META,
        &rules::getf_default_nil::RULE,
    ),
    RuleEntry::new(
        &rules::make_array_default_keyword::META,
        &rules::make_array_default_keyword::RULE,
    ),
    RuleEntry::new(
        &rules::nested_char_case::META,
        &rules::nested_char_case::RULE,
    ),
    RuleEntry::new(&rules::list_star_nil::META, &rules::list_star_nil::RULE),
];
