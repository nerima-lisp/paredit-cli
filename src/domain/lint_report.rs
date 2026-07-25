//! Aggregate syntactic-lint pass: runs every within-file Common Lisp
//! logic-bug lint this tool ships and returns their findings as one
//! rule-tagged list, so a single `inspect lint` answers "does this file have
//! any of the known bug shapes?" without invoking seven commands.
//!
//! Each contributing lint keeps its own focused module and command; this
//! module only fans out to their `collect_*` entry points and normalizes the
//! per-lint item into a uniform [`LintFinding`] (`rule`, `path`, `span`,
//! `message`). Adding a lint to the suite is one `push_findings` block here.
//!
//! Scope: Common Lisp only, inherited from every underlying lint — each is a
//! documented no-op on other dialects, so the aggregate is too.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::accessor_arity_report::{
    collect_accessor_arity_violations, expected_arity_phrase as accessor_arity_phrase,
};
use crate::domain::append_list_to_cons_report::collect_append_list_to_cons;
use crate::domain::append_nil_report::collect_append_nils;
use crate::domain::binds_constant_report::collect_binds_constant;
use crate::domain::butlast_default_count_report::collect_butlast_default_counts;
use crate::domain::car_nthcdr_report::collect_car_nthcdrs;
use crate::domain::car_reverse_report::collect_car_reverses;
use crate::domain::case_nil_key_report::collect_case_nil_keys;
use crate::domain::char_case_fold_report::collect_char_case_folds;
use crate::domain::char_op_string_report::collect_char_op_strings;
use crate::domain::code_char_char_code_report::collect_code_char_char_codes;
use crate::domain::coerce_to_t_report::collect_coerce_to_ts;
use crate::domain::cond_t_clause_report::collect_cond_t_clauses;
use crate::domain::cons_to_list_report::collect_cons_to_lists;
use crate::domain::constant_if_test_report::collect_constant_if_tests;
use crate::domain::constant_when_test_report::collect_constant_when_tests;
use crate::domain::de_morgan_report::collect_de_morgans;
use crate::domain::dead_boolean_operand_report::collect_dead_boolean_operands;
use crate::domain::defpackage_quoted_report::collect_defpackage_quoted;
use crate::domain::destructive_literal_report::collect_destructive_literals;
use crate::domain::dialect::Dialect;
use crate::domain::double_reverse_report::collect_double_reverses;
use crate::domain::duplicate_boolean_operand_report::collect_duplicate_boolean_operands;
use crate::domain::duplicate_case_key_report::collect_duplicate_case_keys;
use crate::domain::duplicate_cond_test_report::collect_duplicate_cond_tests;
use crate::domain::duplicate_keyword_report::collect_duplicate_keywords;
use crate::domain::duplicate_lambda_list_keyword_report::collect_duplicate_lambda_list_keywords;
use crate::domain::duplicate_let_binding_report::collect_duplicate_let_bindings;
use crate::domain::duplicate_parameter_report::collect_duplicate_parameters;
use crate::domain::duplicate_setf_place_report::collect_duplicate_setf_places;
use crate::domain::empty_body_report::collect_empty_bodies;
use crate::domain::empty_let_report::collect_empty_lets;
use crate::domain::eq_char_comparison_report::collect_eq_char_comparisons;
use crate::domain::eq_number_comparison_report::collect_eq_number_comparisons;
use crate::domain::eql_list_comparison_report::collect_eql_list_comparisons;
use crate::domain::eql_search_literal_report::collect_eql_search_literals;
use crate::domain::eql_string_comparison_report::collect_eql_string_comparisons;
use crate::domain::equality_arity_report::collect_equality_arity_violations;
use crate::domain::eval_when_situation_report::collect_eval_when_situations;
use crate::domain::exhaustive_case_otherwise_report::collect_exhaustive_case_otherwise;
use crate::domain::explicit_nil_return_report::collect_explicit_nil_returns;
use crate::domain::explicit_step_delta_report::collect_explicit_step_deltas;
use crate::domain::format_missing_destination_report::collect_format_missing_destinations;
use crate::domain::format_newline_report::collect_format_newlines;
use crate::domain::format_to_string_report::collect_format_to_strings;
use crate::domain::funcall_lambda_report::collect_funcall_lambdas;
use crate::domain::getf_default_nil_report::collect_getf_default_nils;
use crate::domain::gethash_default_report::collect_gethash_defaults;
use crate::domain::handler_case_no_clauses_report::collect_handler_case_no_clauses;
use crate::domain::identical_if_branch_report::collect_identical_if_branches;
use crate::domain::identity_arithmetic_report::collect_identity_arithmetic;
use crate::domain::if_arity_report::collect_if_arity_violations;
use crate::domain::if_not_report::collect_if_nots;
use crate::domain::if_to_or_report::collect_if_to_ors;
use crate::domain::if_to_unless_report::collect_if_to_unless;
use crate::domain::lambda_list_keyword_order_report::collect_lambda_list_keyword_order;
use crate::domain::last_default_count_report::collect_last_default_counts;
use crate::domain::list_star_nil_report::collect_list_star_nils;
use crate::domain::list_star_to_cons_report::collect_list_star_to_cons;
use crate::domain::literal_place_report::collect_literal_places;
use crate::domain::make_array_default_keyword_report::collect_make_array_default_keywords;
use crate::domain::make_hash_table_test_report::collect_make_hash_table_tests;
use crate::domain::make_list_default_element_report::collect_make_list_default_elements;
use crate::domain::malformed_case_clause_report::collect_malformed_case_clauses;
use crate::domain::malformed_cond_clause_report::collect_malformed_cond_clauses;
use crate::domain::malformed_iteration_spec_report::collect_malformed_iteration_specs;
use crate::domain::malformed_let_binding_report::collect_malformed_let_bindings;
use crate::domain::manual_incf_report::collect_manual_incfs;
use crate::domain::manual_push_report::collect_manual_pushes;
use crate::domain::manual_pushnew_report::collect_manual_pushnews;
use crate::domain::modify_macro_arity_report::{
    collect_modify_macro_arity_violations, expected_arity_phrase,
};
use crate::domain::multiple_value_list_of_values_report::collect_multiple_value_list_of_values;
use crate::domain::negated_comparison_report::collect_negated_comparisons;
use crate::domain::negated_if_report::collect_negated_ifs;
use crate::domain::negated_step_delta_report::collect_negated_step_deltas;
use crate::domain::negated_when_unless_report::collect_negated_when_unless;
use crate::domain::nested_boolean_report::collect_nested_booleans;
use crate::domain::nested_char_case_report::collect_nested_char_cases;
use crate::domain::nested_cxr_report::collect_nested_cxrs;
use crate::domain::nested_progn_report::collect_nested_progns;
use crate::domain::nested_string_case_report::collect_nested_string_cases;
use crate::domain::nested_unless_report::collect_nested_unlesses;
use crate::domain::nested_when_report::collect_nested_whens;
use crate::domain::nil_comparison_report::collect_nil_comparisons;
use crate::domain::nth_constant_index_report::collect_nth_constant_indexes;
use crate::domain::nthcdr_small_index_report::collect_nthcdr_small_indexes;
use crate::domain::nthcdr_zero_report::collect_nthcdr_zeros;
use crate::domain::one_armed_if_report::collect_one_armed_ifs;
use crate::domain::one_step_arithmetic_report::collect_one_step_arithmetic;
use crate::domain::parse_integer_default_radix_report::collect_parse_integer_default_radixes;
use crate::domain::prog2_to_progn_report::collect_prog2_to_progn;
use crate::domain::quoted_case_key_report::collect_quoted_case_keys;
use crate::domain::redundant_apply_report::collect_redundant_applies;
use crate::domain::redundant_body_progn_report::collect_redundant_body_progns;
use crate::domain::redundant_boolean_identity_report::collect_redundant_boolean_identities;
use crate::domain::redundant_count_nil_report::collect_redundant_count_nils;
use crate::domain::redundant_divisor_report::collect_redundant_divisors;
use crate::domain::redundant_end_nil_report::collect_redundant_end_nils;
use crate::domain::redundant_eql_test_report::collect_redundant_eql_tests;
use crate::domain::redundant_from_end_nil_report::collect_redundant_from_end_nils;
use crate::domain::redundant_funcall_report::collect_redundant_funcalls;
use crate::domain::redundant_identity_key_report::collect_redundant_identity_keys;
use crate::domain::redundant_identity_report::collect_redundant_identities;
use crate::domain::redundant_if_nil_report::collect_redundant_if_nils;
use crate::domain::redundant_let_star_report::collect_redundant_let_stars;
use crate::domain::redundant_prog1_report::collect_redundant_prog1s;
use crate::domain::redundant_progn_report::collect_redundant_progns;
use crate::domain::redundant_quote_report::collect_redundant_quotes;
use crate::domain::redundant_start_zero_report::collect_redundant_start_zeros;
use crate::domain::redundant_the_report::collect_redundant_thes;
use crate::domain::self_assignment_report::collect_self_assignments;
use crate::domain::self_comparison_report::collect_self_comparisons;
use crate::domain::setf_arity_report::collect_setf_arity_violations;
use crate::domain::setq_non_variable_report::collect_setq_non_variables;
use crate::domain::sexpr::{ByteSpan, SyntaxTree};
use crate::domain::sharp_quoted_lambda_report::collect_sharp_quoted_lambdas;
use crate::domain::sign_comparison_report::collect_sign_comparisons;
use crate::domain::single_arg_comparison_report::collect_single_arg_comparisons;
use crate::domain::single_clause_cond_report::collect_single_clause_conds;
use crate::domain::single_operand_arithmetic_report::collect_single_operand_arithmetic;
use crate::domain::single_operand_boolean_report::collect_single_operand_booleans;
use crate::domain::single_operand_list_op_report::collect_single_operand_list_ops;
use crate::domain::single_value_bind_report::collect_single_value_binds;
use crate::domain::step_zero_report::collect_step_zeros;
use crate::domain::string_case_fold_report::collect_string_case_folds;
use crate::domain::subseq_zero_report::collect_subseq_zeros;
use crate::domain::t_comparison_report::collect_t_comparisons;
use crate::domain::the_arity_report::collect_the_arity_violations;
use crate::domain::typecase_nil_key_report::collect_typecase_nil_keys;
use crate::domain::typep_predicate_report::collect_typep_predicates;
use crate::domain::unreachable_case_clause_report::collect_unreachable_case_clauses;
use crate::domain::unreachable_cond_clause_report::collect_unreachable_cond_clauses;
use crate::domain::unwind_protect_no_cleanup_report::collect_unwind_protect_no_cleanup;
use crate::domain::values_list_of_list_report::collect_values_list_of_lists;
use crate::domain::verbose_negation_report::collect_verbose_negations;
use crate::domain::zero_divisor_report::collect_zero_divisors;

/// Stable rule identifiers, matching each lint's own `inspect` command name.
pub const RULES: [&str; 134] = [
    "self-assignment",
    "duplicate-setf-places",
    "setf-arity",
    "setq-non-variable",
    "manual-incf",
    "manual-push",
    "manual-pushnew",
    "explicit-step-delta",
    "negated-step-delta",
    "explicit-nil-return",
    "cons-to-list",
    "double-reverse",
    "modify-macro-arity",
    "duplicate-parameters",
    "duplicate-lambda-list-keyword",
    "lambda-list-keyword-order",
    "redundant-quote",
    "redundant-progn",
    "nested-progn",
    "nested-when",
    "nested-unless",
    "nested-boolean",
    "nested-cxr",
    "nth-constant-index",
    "nthcdr-zero",
    "nthcdr-small-index",
    "redundant-body-progn",
    "empty-let",
    "redundant-if-nil",
    "redundant-let-star",
    "redundant-funcall",
    "redundant-the",
    "funcall-lambda",
    "sharp-quoted-lambda",
    "redundant-identity",
    "redundant-apply",
    "redundant-eql-test",
    "redundant-identity-key",
    "negated-when-unless",
    "negated-comparison",
    "negated-if",
    "if-to-or",
    "if-not",
    "one-step-arithmetic",
    "one-armed-if",
    "self-comparison",
    "nil-comparison",
    "t-comparison",
    "identical-if-branches",
    "constant-if-test",
    "constant-when-test",
    "if-arity",
    "the-arity",
    "equality-arity",
    "accessor-arity",
    "append-list-to-cons",
    "format-to-string",
    "format-newline",
    "redundant-divisor",
    "duplicate-case-keys",
    "quoted-case-key",
    "case-nil-key",
    "typecase-nil-key",
    "malformed-case-clause",
    "unreachable-case-clause",
    "exhaustive-case-otherwise",
    "duplicate-cond-tests",
    "unreachable-cond-clause",
    "malformed-cond-clause",
    "duplicate-let-bindings",
    "malformed-let-binding",
    "binds-constant",
    "malformed-iteration-spec",
    "eval-when-situation",
    "duplicate-boolean-operands",
    "dead-boolean-operand",
    "redundant-boolean-identity",
    "de-morgan",
    "single-operand-boolean",
    "single-operand-list-op",
    "single-operand-arithmetic",
    "eql-string-comparison",
    "eql-list-comparison",
    "eql-search-literal",
    "eq-number-comparison",
    "eq-char-comparison",
    "single-arg-comparison",
    "sign-comparison",
    "single-clause-cond",
    "cond-t-clause",
    "single-value-bind",
    "format-missing-destination",
    "literal-place",
    "destructive-literal",
    "char-op-string",
    "empty-body",
    "identity-arithmetic",
    "verbose-negation",
    "list-star-to-cons",
    "values-list-of-list",
    "redundant-prog1",
    "subseq-zero",
    "car-nthcdr",
    "car-reverse",
    "append-nil",
    "multiple-value-list-of-values",
    "typep-predicate",
    "coerce-to-t",
    "gethash-default",
    "make-hash-table-test",
    "zero-divisor",
    "duplicate-keyword",
    "defpackage-quoted",
    "step-zero",
    "if-to-unless",
    "prog2-to-progn",
    "handler-case-no-clauses",
    "unwind-protect-no-cleanup",
    "redundant-start-zero",
    "redundant-end-nil",
    "redundant-from-end-nil",
    "redundant-count-nil",
    "string-case-fold",
    "char-case-fold",
    "nested-string-case",
    "code-char-char-code",
    "last-default-count",
    "butlast-default-count",
    "make-list-default-element",
    "parse-integer-default-radix",
    "getf-default-nil",
    "make-array-default-keyword",
    "nested-char-case",
    "list-star-nil",
];

/// The set of rule categories, sorted, for `--category` validation and
/// discovery. Every rule in [`RULE_DOCS`] carries exactly one of these.
pub const CATEGORIES: [&str; 5] = ["arity", "dead-code", "duplicate", "malformed", "suspicious"];

/// `(rule name, category, one-line description)` for each rule, in [`RULES`]
/// order. Powers `inspect lint --list-rules`, the `--category` filter, and
/// inline descriptions in the report, so an agent can discover the rule set,
/// its groupings, and its `--rule`/`--exclude`/`--category` names without
/// consulting the documentation. Kept in lockstep with [`RULES`] and
/// [`CATEGORIES`] by `rule_docs_cover_every_rule`.
pub const RULE_DOCS: [(&str, &str, &str); 134] = [
    (
        "self-assignment",
        "suspicious",
        "a setq/setf/psetq/psetf that assigns a place to itself",
    ),
    (
        "duplicate-setf-places",
        "duplicate",
        "a setf/setq/psetf/psetq that assigns the same variable more than once ((setf a 1 a 2))",
    ),
    (
        "setf-arity",
        "arity",
        "a setq/setf/psetq/psetf with an odd argument count",
    ),
    (
        "setq-non-variable",
        "malformed",
        "a setq/psetq place that is not a variable (a list, literal, or constant)",
    ),
    (
        "manual-incf",
        "suspicious",
        "a setf/setq that manually increments a variable ((setf x (1+ x)) is (incf x))",
    ),
    (
        "manual-push",
        "suspicious",
        "a setf/setq that manually conses onto a variable ((setf x (cons e x)) is (push e x))",
    ),
    (
        "manual-pushnew",
        "suspicious",
        "a setf/setq that manually adjoins onto a variable ((setf x (adjoin e x)) is (pushnew e x))",
    ),
    (
        "explicit-step-delta",
        "suspicious",
        "an incf/decf with an explicit delta of 1, the default ((incf x 1) is (incf x))",
    ),
    (
        "negated-step-delta",
        "suspicious",
        "an incf/decf with a negative literal delta, which flips the operator ((incf x -1) is (decf x))",
    ),
    (
        "explicit-nil-return",
        "suspicious",
        "a return/return-from with an explicit nil result, the default ((return nil) is (return))",
    ),
    (
        "cons-to-list",
        "suspicious",
        "a cons onto nil or a list literal ((cons a nil) is (list a); (cons a (list b)) is (list a b))",
    ),
    (
        "double-reverse",
        "suspicious",
        "a (reverse (reverse x)), a wasteful obfuscated copy ((reverse (reverse x)) is (copy-seq x))",
    ),
    (
        "modify-macro-arity",
        "arity",
        "an incf/decf/push/pop call with the wrong number of arguments",
    ),
    (
        "duplicate-parameters",
        "duplicate",
        "a lambda list that names the same parameter more than once",
    ),
    (
        "duplicate-lambda-list-keyword",
        "duplicate",
        "a lambda list that repeats a lambda-list keyword (&optional, &key, ...)",
    ),
    (
        "lambda-list-keyword-order",
        "malformed",
        "lambda-list keywords out of the canonical &optional/&rest/&key/&aux order",
    ),
    (
        "redundant-quote",
        "suspicious",
        "a self-evaluating literal (number/string/char/keyword) quoted redundantly",
    ),
    (
        "redundant-progn",
        "suspicious",
        "a progn that is empty or wraps a single form (progn X is just X)",
    ),
    (
        "nested-progn",
        "suspicious",
        "a multi-form progn nested directly inside another progn (its forms splice in)",
    ),
    (
        "nested-when",
        "suspicious",
        "a when whose only body is a when, mergeable by and ((when a (when b c)) is (when (and a b) c))",
    ),
    (
        "nested-unless",
        "suspicious",
        "an unless whose only body is an unless, mergeable by or ((unless a (unless b c)) is (unless (or a b) c))",
    ),
    (
        "nested-boolean",
        "suspicious",
        "a same-operator and/or nested in an and/or, which flattens ((or a (or b c)) is (or a b c))",
    ),
    (
        "nested-cxr",
        "suspicious",
        "nested car/cdr accessors that combine into one ((car (cdr x)) is (cadr x))",
    ),
    (
        "nth-constant-index",
        "suspicious",
        "an nth with a small constant index that has an ordinal accessor ((nth 0 x) is (first x))",
    ),
    (
        "nthcdr-zero",
        "suspicious",
        "an nthcdr with a zero count, which returns the list unchanged ((nthcdr 0 x) is x)",
    ),
    (
        "nthcdr-small-index",
        "suspicious",
        "an nthcdr with a 1-4 count that has a named cdr accessor ((nthcdr 2 x) is (cddr x))",
    ),
    (
        "redundant-body-progn",
        "suspicious",
        "a multi-form progn used as a when/unless/let/defun/... body (its forms splice in)",
    ),
    (
        "empty-let",
        "suspicious",
        "a let with an empty binding list, which is just progn ((let () body) is (progn body))",
    ),
    (
        "redundant-if-nil",
        "suspicious",
        "a three-argument if whose else branch is a redundant literal nil ((if c x nil) is (if c x))",
    ),
    (
        "redundant-let-star",
        "suspicious",
        "a let* with zero or one binding, which is just let (no sequential scope in play)",
    ),
    (
        "redundant-funcall",
        "suspicious",
        "a funcall of a sharp-quoted symbol ((funcall #'foo a b) is just (foo a b))",
    ),
    (
        "redundant-the",
        "suspicious",
        "a (the t form) type declaration, which is vacuous and is just form (t matches every object)",
    ),
    (
        "funcall-lambda",
        "suspicious",
        "a funcall of a lambda form, which applies directly ((funcall (lambda (x) x) a) is ((lambda (x) x) a))",
    ),
    (
        "sharp-quoted-lambda",
        "suspicious",
        "a lambda form with a redundant #' prefix (#'(lambda (x) x) is (lambda (x) x))",
    ),
    (
        "redundant-identity",
        "suspicious",
        "an identity call, which returns its argument unchanged ((identity x) is just x)",
    ),
    (
        "redundant-apply",
        "suspicious",
        "an apply of a sharp-quoted symbol to a literal list ((apply #'foo (list a b)) is (foo a b))",
    ),
    (
        "redundant-eql-test",
        "suspicious",
        "an eql-defaulting call with an explicit :test #'eql, the default ((find x l :test #'eql) is (find x l))",
    ),
    (
        "redundant-identity-key",
        "suspicious",
        "a :key-taking call with an explicit :key #'identity/nil, the default ((sort xs #'< :key #'identity) is (sort xs #'<))",
    ),
    (
        "negated-when-unless",
        "suspicious",
        "a when/unless whose test is a (not X)/(null X) negation (flip the macro instead)",
    ),
    (
        "negated-comparison",
        "suspicious",
        "a not/null of a two-argument numeric comparison ((not (= a b)) is (/= a b))",
    ),
    (
        "negated-if",
        "suspicious",
        "a three-argument if with a negated test ((if (not c) a b) is (if c b a))",
    ),
    (
        "if-to-or",
        "suspicious",
        "an if whose test and then are the same atom ((if x x y) is (or x y))",
    ),
    (
        "if-not",
        "suspicious",
        "a three-argument if with then=nil and else=t ((if test nil t) is (not test))",
    ),
    (
        "one-step-arithmetic",
        "suspicious",
        "a +/- of a literal 1 with a shorthand ((+ x 1) is (1+ x); (- x 1) is (1- x))",
    ),
    (
        "one-armed-if",
        "suspicious",
        "a two-argument if with no else branch ((if test then) is (when test then))",
    ),
    (
        "self-comparison",
        "suspicious",
        "a comparison whose two operands are structurally identical",
    ),
    (
        "nil-comparison",
        "suspicious",
        "an eq/eql/equal/equalp comparison against nil ((eq x nil) is just (null x))",
    ),
    (
        "t-comparison",
        "suspicious",
        "an eq/eql/equal/equalp comparison against t (only matches the symbol T, not any true value)",
    ),
    (
        "identical-if-branches",
        "dead-code",
        "an if whose then and else branches are structurally identical",
    ),
    (
        "constant-if-test",
        "dead-code",
        "an if whose test is the literal t or nil ((if t a b) is a; (if nil a b) is b)",
    ),
    (
        "constant-when-test",
        "dead-code",
        "a when/unless with a literal t/nil test ((when t b) is (progn b); (when nil b) is nil)",
    ),
    (
        "if-arity",
        "arity",
        "an if with the wrong number of arguments (Common Lisp if takes 2 or 3)",
    ),
    (
        "the-arity",
        "arity",
        "a the special form without exactly two arguments (a type and a form)",
    ),
    (
        "equality-arity",
        "arity",
        "an eq/eql/equal/equalp call without exactly two arguments",
    ),
    (
        "accessor-arity",
        "arity",
        "an nth/elt/gethash/getf/... accessor with the wrong number of arguments",
    ),
    (
        "append-list-to-cons",
        "suspicious",
        "a one-element append that is just a cons ((append (list x) rest) is (cons x rest))",
    ),
    (
        "format-to-string",
        "suspicious",
        "a (format nil \"~A\"/\"~S\" x), which is (princ-to-string x)/(prin1-to-string x)",
    ),
    (
        "format-newline",
        "suspicious",
        "a (format t \"~%\"), which is just (terpri) (write a newline to standard output)",
    ),
    (
        "redundant-divisor",
        "suspicious",
        "a quotient op with a redundant divisor of 1 ((floor x 1) is (floor x))",
    ),
    (
        "duplicate-case-keys",
        "duplicate",
        "a case/ecase/ccase key repeated across more than one clause",
    ),
    (
        "quoted-case-key",
        "suspicious",
        "a case/ecase/ccase clause with a quoted key ('a matches quote and a, not a)",
    ),
    (
        "case-nil-key",
        "dead-code",
        "a case/ecase/ccase clause with a bare nil key, which is the empty key list and never matches (use ((nil) …))",
    ),
    (
        "typecase-nil-key",
        "dead-code",
        "a typecase/etypecase/ctypecase clause with a bare nil type, which is the empty type and never matches (use null)",
    ),
    (
        "malformed-case-clause",
        "malformed",
        "a case/typecase clause that is not a non-empty list",
    ),
    (
        "unreachable-case-clause",
        "dead-code",
        "a case/typecase clause after a t/otherwise catch-all that can never run",
    ),
    (
        "exhaustive-case-otherwise",
        "malformed",
        "an ecase/ccase/etypecase/ctypecase with a forbidden t/otherwise clause",
    ),
    (
        "duplicate-cond-tests",
        "duplicate",
        "a cond test expression repeated across more than one clause",
    ),
    (
        "unreachable-cond-clause",
        "dead-code",
        "a cond clause after a t catch-all that can never run",
    ),
    (
        "malformed-cond-clause",
        "malformed",
        "a cond clause that is not a non-empty list",
    ),
    (
        "duplicate-let-bindings",
        "duplicate",
        "a parallel let that binds the same variable more than once",
    ),
    (
        "malformed-let-binding",
        "malformed",
        "a let/let* binding that is neither a symbol nor a (var value) pair",
    ),
    (
        "binds-constant",
        "malformed",
        "a let/let*/do/do* binding whose variable is a constant (nil, t, or a keyword)",
    ),
    (
        "malformed-iteration-spec",
        "malformed",
        "a dolist/dotimes spec that is not a (var form [result]) list",
    ),
    (
        "eval-when-situation",
        "malformed",
        "an eval-when with an invalid situation (not :compile-toplevel/:load-toplevel/:execute)",
    ),
    (
        "duplicate-boolean-operands",
        "duplicate",
        "an and/or that lists the same operand more than once",
    ),
    (
        "dead-boolean-operand",
        "dead-code",
        "an and/or whose non-final constant operand makes later operands dead",
    ),
    (
        "redundant-boolean-identity",
        "suspicious",
        "an and/or with a redundant identity operand (t in and, nil in or; (and a t b) is (and a b))",
    ),
    (
        "de-morgan",
        "suspicious",
        "an and/or of all negations, collapsible by De Morgan ((and (not a) (not b)) is (not (or a b)))",
    ),
    (
        "single-operand-boolean",
        "suspicious",
        "a single-operand and/or ((and X) and (or X) are just X)",
    ),
    (
        "single-operand-list-op",
        "suspicious",
        "a single-argument append/nconc/list*, which returns its argument unchanged ((append x) is x)",
    ),
    (
        "single-operand-arithmetic",
        "suspicious",
        "a single-operand +/* ((+ X) and (* X) are just X)",
    ),
    (
        "eql-string-comparison",
        "suspicious",
        "an eq/eql compared against a string literal (never reliably eql)",
    ),
    (
        "eql-list-comparison",
        "suspicious",
        "an eq/eql compared against a quoted list literal (never reliably eql)",
    ),
    (
        "eql-search-literal",
        "suspicious",
        "a member/assoc/find/substitute/adjoin/... searching for a string/list literal without :test (default eql won't match)",
    ),
    (
        "eq-number-comparison",
        "suspicious",
        "an eq compared against a number literal (eq on numbers is unreliable)",
    ),
    (
        "eq-char-comparison",
        "suspicious",
        "an eq compared against a character literal (eq on characters is unreliable; use eql/char=)",
    ),
    (
        "single-arg-comparison",
        "suspicious",
        "a numeric comparison (< > <= >= = /=) with one argument (always true; missing an operand?)",
    ),
    (
        "sign-comparison",
        "suspicious",
        "a =/</> comparison against 0 with a dedicated predicate ((= x 0) is (zerop x))",
    ),
    (
        "single-clause-cond",
        "suspicious",
        "a cond with one non-t clause that has a body ((cond (test a b)) is (when test a b))",
    ),
    (
        "cond-t-clause",
        "suspicious",
        "a cond with one t clause that has a body ((cond (t a b)) is (progn a b))",
    ),
    (
        "single-value-bind",
        "suspicious",
        "a multiple-value-bind of one variable ((multiple-value-bind (x) f body) is (let ((x f)) body))",
    ),
    (
        "format-missing-destination",
        "suspicious",
        "a format call whose first argument is a string literal (nil/t/stream destination is missing)",
    ),
    (
        "literal-place",
        "malformed",
        "an incf/decf/push/pop/pushnew/setf/psetf whose place is a self-evaluating literal (cannot be modified)",
    ),
    (
        "destructive-literal",
        "suspicious",
        "a destructive call (nreverse/sort/delete/nsubstitute/nconc/...) on a quoted list literal (undefined behavior)",
    ),
    (
        "char-op-string",
        "malformed",
        "a character function (char=/char-code/alpha-char-p/...) applied to a string literal (type error)",
    ),
    (
        "empty-body",
        "suspicious",
        "a when/unless/dolist/dotimes with no body (the test/spec runs, then nothing happens)",
    ),
    (
        "identity-arithmetic",
        "suspicious",
        "an arithmetic form with a redundant identity operand ((+ x 0), (* x 1), (- x 0), (/ x 1))",
    ),
    (
        "verbose-negation",
        "suspicious",
        "negation written the long way ((- 0 x) and (* x -1) are (- x))",
    ),
    (
        "list-star-to-cons",
        "suspicious",
        "a two-argument list*, which is just a cons ((list* a b) is (cons a b))",
    ),
    (
        "values-list-of-list",
        "suspicious",
        "a values-list of a list constructor ((values-list (list a b)) is (values a b))",
    ),
    (
        "redundant-prog1",
        "suspicious",
        "a prog1 wrapping a single form, which is just that form ((prog1 x) is x)",
    ),
    (
        "subseq-zero",
        "suspicious",
        "a subseq from index 0, a whole-sequence copy ((subseq seq 0) is (copy-seq seq))",
    ),
    (
        "car-nthcdr",
        "suspicious",
        "a car of an nthcdr, which is nth ((car (nthcdr n x)) is (nth n x))",
    ),
    (
        "car-reverse",
        "suspicious",
        "a car of a reverse, a wasteful full copy ((car (reverse x)) is (car (last x)))",
    ),
    (
        "append-nil",
        "suspicious",
        "a two-argument append with a nil tail, a fresh copy ((append x nil) is (copy-list x))",
    ),
    (
        "multiple-value-list-of-values",
        "suspicious",
        "a multiple-value-list of a values form ((multiple-value-list (values a b)) is (list a b))",
    ),
    (
        "typep-predicate",
        "suspicious",
        "a typep against a type with a dedicated predicate ((typep x 'string) is (stringp x))",
    ),
    (
        "coerce-to-t",
        "suspicious",
        "a coerce to type t, which returns the object unchanged ((coerce x t) is x)",
    ),
    (
        "gethash-default",
        "suspicious",
        "a gethash with an explicit nil default, the default ((gethash k h nil) is (gethash k h))",
    ),
    (
        "make-hash-table-test",
        "suspicious",
        "a make-hash-table with an explicit :test 'eql, the default ((make-hash-table :test 'eql) is (make-hash-table))",
    ),
    (
        "zero-divisor",
        "suspicious",
        "a division-family form with a literal 0 divisor, a guaranteed division-by-zero ((/ x 0))",
    ),
    (
        "duplicate-keyword",
        "duplicate",
        "a make-* call passing the same keyword argument twice ((make-instance 'c :x 1 :x 2))",
    ),
    (
        "defpackage-quoted",
        "malformed",
        "a quoted designator in a defpackage clause, which defpackage does not evaluate ((:export 'foo))",
    ),
    (
        "step-zero",
        "suspicious",
        "an incf/decf with an explicit step of 0, a no-op ((incf x 0))",
    ),
    (
        "if-to-unless",
        "suspicious",
        "a three-argument if with then=nil ((if c nil e) is (unless c e))",
    ),
    (
        "prog2-to-progn",
        "suspicious",
        "a two-form prog2, which is just progn ((prog2 a b) is (progn a b))",
    ),
    (
        "handler-case-no-clauses",
        "suspicious",
        "a handler-case with no handler clauses, which is just its body ((handler-case x) is x)",
    ),
    (
        "unwind-protect-no-cleanup",
        "suspicious",
        "an unwind-protect with no cleanup forms, which is just its body ((unwind-protect x) is x)",
    ),
    (
        "redundant-start-zero",
        "suspicious",
        "a bounded-sequence call with an explicit :start 0, the default ((find x seq :start 0) is (find x seq))",
    ),
    (
        "redundant-end-nil",
        "suspicious",
        "a bounded-sequence call with an explicit :end nil, the default ((find x seq :end nil) is (find x seq))",
    ),
    (
        "redundant-from-end-nil",
        "suspicious",
        "a sequence call with an explicit :from-end nil, the default ((find x seq :from-end nil) is (find x seq))",
    ),
    (
        "redundant-count-nil",
        "suspicious",
        "a remove/delete/substitute call with an explicit :count nil, the default ((remove x seq :count nil) is (remove x seq))",
    ),
    (
        "string-case-fold",
        "suspicious",
        "a string= of two same-case-folded operands ((string= (string-downcase a) (string-downcase b)) is (string-equal a b))",
    ),
    (
        "char-case-fold",
        "suspicious",
        "a char= of two same-case-folded operands ((char= (char-downcase a) (char-downcase b)) is (char-equal a b))",
    ),
    (
        "nested-string-case",
        "suspicious",
        "nested string case ops where the outer dominates ((string-upcase (string-downcase s)) is (string-upcase s))",
    ),
    (
        "code-char-char-code",
        "suspicious",
        "a code-char of a char-code, a round-trip that is just the character ((code-char (char-code c)) is c)",
    ),
    (
        "last-default-count",
        "suspicious",
        "a last call with an explicit count of 1, the default ((last x 1) is (last x))",
    ),
    (
        "butlast-default-count",
        "suspicious",
        "a butlast/nbutlast call with an explicit count of 1, the default ((butlast x 1) is (butlast x))",
    ),
    (
        "make-list-default-element",
        "suspicious",
        "a make-list call with an explicit :initial-element nil, the default ((make-list n :initial-element nil) is (make-list n))",
    ),
    (
        "parse-integer-default-radix",
        "suspicious",
        "a parse-integer call with an explicit :radix 10, the default ((parse-integer s :radix 10) is (parse-integer s))",
    ),
    (
        "getf-default-nil",
        "suspicious",
        "a getf call with an explicit nil default, the default ((getf p k nil) is (getf p k))",
    ),
    (
        "make-array-default-keyword",
        "suspicious",
        "a make-array call with an explicit :adjustable nil or :fill-pointer nil, the default ((make-array n :adjustable nil) is (make-array n))",
    ),
    (
        "nested-char-case",
        "suspicious",
        "nested char case ops where the outer dominates ((char-upcase (char-downcase c)) is (char-upcase c))",
    ),
    (
        "list-star-nil",
        "suspicious",
        "a list* with a nil tail, a spelled-out list ((list* a b nil) is (list a b))",
    ),
];

/// The one-line description for a rule name, or `None` if the name is unknown.
pub fn rule_description(name: &str) -> Option<&'static str> {
    RULE_DOCS
        .iter()
        .find(|(rule, _, _)| *rule == name)
        .map(|(_, _, doc)| *doc)
}

/// The category for a rule name, or `None` if the name is unknown.
pub fn rule_category(name: &str) -> Option<&'static str> {
    RULE_DOCS
        .iter()
        .find(|(rule, _, _)| *rule == name)
        .map(|(_, category, _)| *category)
}

/// The rules for which `inspect lint --fix` (and the SARIF `fixes` field) can
/// synthesize an automatic rewrite. This is the single source of truth for
/// fixability: the fix engine in the CLI layer keys off exactly these names,
/// and it is asserted to match that engine's actual behavior by a guard test
/// there. The rest of the rules are diagnostic-only (their repair depends on
/// intent a machine cannot infer).
pub const FIXABLE_RULES: [&str; 88] = [
    "manual-incf",
    "manual-push",
    "manual-pushnew",
    "explicit-step-delta",
    "negated-step-delta",
    "explicit-nil-return",
    "cons-to-list",
    "double-reverse",
    "append-list-to-cons",
    "format-to-string",
    "format-newline",
    "redundant-divisor",
    "redundant-quote",
    "redundant-progn",
    "nested-progn",
    "nested-when",
    "nested-unless",
    "nested-boolean",
    "nested-cxr",
    "nth-constant-index",
    "nthcdr-zero",
    "nthcdr-small-index",
    "redundant-body-progn",
    "empty-let",
    "redundant-if-nil",
    "redundant-let-star",
    "redundant-funcall",
    "redundant-the",
    "funcall-lambda",
    "sharp-quoted-lambda",
    "redundant-identity",
    "redundant-apply",
    "redundant-eql-test",
    "redundant-identity-key",
    "redundant-boolean-identity",
    "de-morgan",
    "single-operand-boolean",
    "single-operand-list-op",
    "single-operand-arithmetic",
    "negated-when-unless",
    "negated-comparison",
    "negated-if",
    "if-to-or",
    "if-not",
    "one-step-arithmetic",
    "one-armed-if",
    "constant-if-test",
    "constant-when-test",
    "nil-comparison",
    "sign-comparison",
    "single-clause-cond",
    "cond-t-clause",
    "single-value-bind",
    "eq-number-comparison",
    "eq-char-comparison",
    "verbose-negation",
    "list-star-to-cons",
    "values-list-of-list",
    "redundant-prog1",
    "subseq-zero",
    "car-nthcdr",
    "car-reverse",
    "append-nil",
    "multiple-value-list-of-values",
    "typep-predicate",
    "coerce-to-t",
    "gethash-default",
    "make-hash-table-test",
    "if-to-unless",
    "prog2-to-progn",
    "handler-case-no-clauses",
    "unwind-protect-no-cleanup",
    "redundant-start-zero",
    "redundant-end-nil",
    "redundant-from-end-nil",
    "redundant-count-nil",
    "string-case-fold",
    "char-case-fold",
    "nested-string-case",
    "code-char-char-code",
    "last-default-count",
    "butlast-default-count",
    "make-list-default-element",
    "parse-integer-default-radix",
    "getf-default-nil",
    "make-array-default-keyword",
    "nested-char-case",
    "list-star-nil",
];

/// Whether `inspect lint --fix` can automatically repair findings of `name`.
pub fn rule_is_fixable(name: &str) -> bool {
    FIXABLE_RULES.contains(&name)
}

/// How serious a rule's findings are. `Error` marks a likely or certain bug;
/// `Warning` marks correct-but-redundant/non-idiomatic code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }

    const fn rank(self) -> u8 {
        match self {
            Self::Error => 2,
            Self::Warning => 1,
        }
    }

    /// Whether this severity is at least as serious as `threshold`.
    pub fn at_least(self, threshold: Severity) -> bool {
        self.rank() >= threshold.rank()
    }
}

/// The rules whose findings are warnings (correct-but-redundant/style code):
/// the pure-redundancy and readability rules. Every other rule is an `error`
/// (a likely or certain bug). This is the single source of truth for severity;
/// a guard test asserts each entry is a real rule.
pub const WARNING_RULES: [&str; 93] = [
    "manual-incf",
    "manual-push",
    "manual-pushnew",
    "explicit-step-delta",
    "negated-step-delta",
    "explicit-nil-return",
    "cons-to-list",
    "double-reverse",
    "append-list-to-cons",
    "format-to-string",
    "format-newline",
    "redundant-divisor",
    "redundant-quote",
    "redundant-progn",
    "nested-progn",
    "nested-when",
    "nested-unless",
    "nested-boolean",
    "nested-cxr",
    "nth-constant-index",
    "nthcdr-zero",
    "nthcdr-small-index",
    "redundant-body-progn",
    "empty-let",
    "redundant-if-nil",
    "redundant-let-star",
    "redundant-funcall",
    "redundant-the",
    "funcall-lambda",
    "sharp-quoted-lambda",
    "redundant-identity",
    "redundant-apply",
    "redundant-eql-test",
    "redundant-identity-key",
    "redundant-boolean-identity",
    "de-morgan",
    "single-operand-boolean",
    "single-operand-list-op",
    "single-operand-arithmetic",
    "negated-when-unless",
    "negated-comparison",
    "negated-if",
    "if-to-or",
    "if-not",
    "one-step-arithmetic",
    "one-armed-if",
    "constant-if-test",
    "constant-when-test",
    "nil-comparison",
    "t-comparison",
    "sign-comparison",
    "single-clause-cond",
    "cond-t-clause",
    "single-value-bind",
    "empty-body",
    "identity-arithmetic",
    "verbose-negation",
    "list-star-to-cons",
    "values-list-of-list",
    "redundant-prog1",
    "subseq-zero",
    "car-nthcdr",
    "car-reverse",
    "append-nil",
    "multiple-value-list-of-values",
    "typep-predicate",
    "coerce-to-t",
    "gethash-default",
    "make-hash-table-test",
    "zero-divisor",
    "duplicate-keyword",
    "defpackage-quoted",
    "step-zero",
    "if-to-unless",
    "prog2-to-progn",
    "handler-case-no-clauses",
    "unwind-protect-no-cleanup",
    "redundant-start-zero",
    "redundant-end-nil",
    "redundant-from-end-nil",
    "redundant-count-nil",
    "string-case-fold",
    "char-case-fold",
    "nested-string-case",
    "code-char-char-code",
    "last-default-count",
    "butlast-default-count",
    "make-list-default-element",
    "parse-integer-default-radix",
    "getf-default-nil",
    "make-array-default-keyword",
    "nested-char-case",
    "list-star-nil",
];

/// The severity of a rule's findings (`error` unless it is a [`WARNING_RULES`]
/// style rule).
pub fn rule_severity(name: &str) -> Severity {
    if WARNING_RULES.contains(&name) {
        Severity::Warning
    } else {
        Severity::Error
    }
}

#[derive(Debug, Clone)]
pub struct LintFinding {
    pub rule: &'static str,
    pub path: PathBuf,
    pub span: ByteSpan,
    pub message: String,
}

#[derive(Debug)]
pub struct LintSummary {
    pub finding_count: usize,
    /// Count of findings per rule, in [`RULES`] order (rules with zero
    /// findings are included so a consumer sees the full checklist).
    pub per_rule: Vec<(&'static str, usize)>,
    pub findings: Vec<LintFinding>,
}

#[derive(Debug, Clone, Copy)]
pub struct LintPolicyOptions {
    fail_on_finding: bool,
    fail_on_severity: Option<Severity>,
}

impl LintPolicyOptions {
    pub fn new(fail_on_finding: bool, fail_on_severity: Option<Severity>) -> Self {
        Self {
            fail_on_finding,
            fail_on_severity,
        }
    }

    pub const fn fail_on_finding(self) -> bool {
        self.fail_on_finding
    }

    pub const fn fail_on_severity(self) -> Option<Severity> {
        self.fail_on_severity
    }
}

#[derive(Debug)]
pub struct LintPolicy {
    pub fail_on_finding: bool,
    pub finding_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Runs every lint over one file and returns all findings, tagged by rule.
pub fn collect_lint_findings(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<Vec<LintFinding>> {
    let mut findings = Vec::new();

    let (_, items) = collect_self_assignments(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "self-assignment",
            path: item.path,
            span: item.span,
            message: format!("{} assigns place {} to itself", item.operator, item.place),
        });
    }

    let (_, items) = collect_duplicate_setf_places(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "duplicate-setf-places",
            path: item.path,
            span: item.span,
            message: format!(
                "{} assigns variable {} more than once; the earlier assignment is dead",
                item.operator, item.place
            ),
        });
    }

    let (_, items) = collect_setf_arity_violations(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "setf-arity",
            path: item.path,
            span: item.span,
            message: format!(
                "{} has {} arguments; place/value pairs require an even count",
                item.operator, item.argument_count
            ),
        });
    }

    let (_, items) = collect_setq_non_variables(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "setq-non-variable",
            path: item.path,
            span: item.span,
            message: format!("{} place {} is not a variable", item.operator, item.place),
        });
    }

    let (_, items) = collect_manual_incfs(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "manual-incf",
            path: item.path,
            span: item.span,
            message: format!(
                "setf manually adjusts a variable; use {}",
                item.suggested_head
            ),
        });
    }

    let (_, items) = collect_manual_pushes(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "manual-push",
            path: item.path,
            span: item.span,
            message: "setf conses onto a variable; use push".to_owned(),
        });
    }

    let (_, items) = collect_manual_pushnews(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "manual-pushnew",
            path: item.path,
            span: item.span,
            message: "setf adjoins onto a variable; use pushnew".to_owned(),
        });
    }

    let (_, items) = collect_explicit_step_deltas(path, dialect, tree)?;
    for item in items {
        let operator = item.operator;
        findings.push(LintFinding {
            rule: "explicit-step-delta",
            path: item.path,
            span: item.span,
            message: format!(
                "{operator} delta of 1 is the default; ({operator} x 1) is ({operator} x)"
            ),
        });
    }

    let (_, items) = collect_negated_step_deltas(path, dialect, tree)?;
    for item in items {
        let opposite = item.opposite;
        findings.push(LintFinding {
            rule: "negated-step-delta",
            path: item.path,
            span: item.span,
            message: format!(
                "negative delta flips the operator; use {opposite} with a positive delta"
            ),
        });
    }

    let (_, items) = collect_explicit_nil_returns(path, dialect, tree)?;
    for item in items {
        let operator = item.operator;
        findings.push(LintFinding {
            rule: "explicit-nil-return",
            path: item.path,
            span: item.span,
            message: format!("{operator} nil result is the default; drop the redundant nil"),
        });
    }

    let (_, items) = collect_cons_to_lists(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "cons-to-list",
            path: item.path,
            span: item.span,
            message: "cons onto nil/a list is a list constructor; use list".to_owned(),
        });
    }

    let (_, items) = collect_double_reverses(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "double-reverse",
            path: item.path,
            span: item.span,
            message: "(reverse (reverse x)) is a wasteful copy; use (copy-seq x)".to_owned(),
        });
    }

    let (_, items) = collect_modify_macro_arity_violations(path, dialect, tree)?;
    for item in items {
        let expected = expected_arity_phrase(&item);
        findings.push(LintFinding {
            rule: "modify-macro-arity",
            path: item.path,
            span: item.span,
            message: format!(
                "{} takes {} argument(s) but has {}",
                item.operator, expected, item.argument_count
            ),
        });
    }

    let (_, items) = collect_duplicate_parameters(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "duplicate-parameters",
            path: item.path,
            span: item.span,
            message: format!(
                "{} names parameter {} more than once ({}×)",
                item.definition, item.parameter, item.occurrence_count
            ),
        });
    }

    let (_, items) = collect_duplicate_lambda_list_keywords(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "duplicate-lambda-list-keyword",
            path: item.path,
            span: item.span,
            message: format!(
                "{} repeats lambda-list keyword {} ({}×)",
                item.definition, item.keyword, item.occurrence_count
            ),
        });
    }

    let (_, items) = collect_lambda_list_keyword_order(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "lambda-list-keyword-order",
            path: item.path,
            span: item.span,
            message: format!(
                "{} lists lambda-list keyword {} after {}",
                item.definition, item.keyword, item.after_keyword
            ),
        });
    }

    let (_, items) = collect_redundant_quotes(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-quote",
            path: item.path,
            span: item.span,
            message: format!("quoting {} {} is redundant", item.kind, item.literal),
        });
    }

    let (_, items) = collect_redundant_progns(path, dialect, tree)?;
    for item in items {
        let detail = if item.body_form_count == 0 {
            "an empty progn is nil".to_owned()
        } else {
            "progn wraps a single form; it is equivalent to that form".to_owned()
        };
        findings.push(LintFinding {
            rule: "redundant-progn",
            path: item.path,
            span: item.span,
            message: format!("redundant progn: {detail}"),
        });
    }

    let (_, items) = collect_nested_progns(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "nested-progn",
            path: item.path,
            span: item.span,
            message: format!(
                "progn with {} forms is nested directly in another progn; splice its forms in",
                item.body_form_count
            ),
        });
    }

    let (_, items) = collect_nested_whens(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "nested-when",
            path: item.path,
            span: item.span,
            message: "when whose only body is a when merges by and; (when a (when b c)) is (when (and a b) c)"
                .to_owned(),
        });
    }

    let (_, items) = collect_nested_unlesses(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "nested-unless",
            path: item.path,
            span: item.span,
            message: "unless whose only body is an unless merges by or; (unless a (unless b c)) is (unless (or a b) c)"
                .to_owned(),
        });
    }

    let (_, items) = collect_nested_booleans(path, dialect, tree)?;
    for item in items {
        let operator = item.operator;
        findings.push(LintFinding {
            rule: "nested-boolean",
            path: item.path,
            span: item.span,
            message: format!("{operator} nested in a {operator} flattens; its operands splice in"),
        });
    }

    let (_, items) = collect_nested_cxrs(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "nested-cxr",
            path: item.path,
            span: item.span,
            message: format!(
                "nested car/cdr accessors combine into ({} …)",
                item.combined
            ),
        });
    }

    let (_, items) = collect_nth_constant_indexes(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "nth-constant-index",
            path: item.path,
            span: item.span,
            message: format!("nth with a constant index; use ({} …)", item.ordinal),
        });
    }

    let (_, items) = collect_nthcdr_zeros(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "nthcdr-zero",
            path: item.path,
            span: item.span,
            message: "nthcdr with a zero count returns the list unchanged; (nthcdr 0 x) is x"
                .to_owned(),
        });
    }

    let (_, items) = collect_nthcdr_small_indexes(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "nthcdr-small-index",
            path: item.path,
            span: item.span,
            message: format!(
                "nthcdr with a small count has a named cdr accessor; use ({} …)",
                item.accessor
            ),
        });
    }

    let (_, items) = collect_redundant_body_progns(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-body-progn",
            path: item.path,
            span: item.span,
            message: format!(
                "progn with {} forms is a {} body; splice its forms in",
                item.body_form_count, item.parent
            ),
        });
    }

    let (_, items) = collect_empty_lets(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "empty-let",
            path: item.path,
            span: item.span,
            message: "let with no bindings is just progn; (let () body) is (progn body)".to_owned(),
        });
    }

    let (_, items) = collect_redundant_if_nils(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-if-nil",
            path: item.path,
            span: item.span,
            message: "if else branch is a redundant nil; (if c x nil) is (if c x)".to_owned(),
        });
    }

    let (_, items) = collect_redundant_let_stars(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-let-star",
            path: item.path,
            span: item.span,
            message: format!(
                "let* with {} binding{} is just let; sequential scope is unused",
                item.binding_count,
                if item.binding_count == 1 { "" } else { "s" }
            ),
        });
    }

    let (_, items) = collect_redundant_funcalls(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-funcall",
            path: item.path,
            span: item.span,
            message: format!(
                "funcall of #'{} is a direct call; (funcall #'{} …) is ({} …)",
                item.callee, item.callee, item.callee
            ),
        });
    }

    let (_, items) = collect_redundant_thes(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-the",
            path: item.path,
            span: item.span,
            message: "(the t form) is a vacuous type declaration; it is just form".to_string(),
        });
    }

    let (_, items) = collect_funcall_lambdas(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "funcall-lambda",
            path: item.path,
            span: item.span,
            message: "funcall of a lambda applies it directly; ((lambda …) …) drops the funcall"
                .to_owned(),
        });
    }

    let (_, items) = collect_sharp_quoted_lambdas(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "sharp-quoted-lambda",
            path: item.path,
            span: item.span,
            message: "#' on a lambda is redundant; #'(lambda …) is (lambda …)".to_owned(),
        });
    }

    let (_, items) = collect_redundant_identities(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-identity",
            path: item.path,
            span: item.span,
            message: "identity returns its argument unchanged; (identity x) is x".to_owned(),
        });
    }

    let (_, items) = collect_redundant_applies(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-apply",
            path: item.path,
            span: item.span,
            message: format!(
                "apply of #'{} to a literal list is a direct call; use ({} …)",
                item.callee, item.callee
            ),
        });
    }

    let (_, items) = collect_redundant_eql_tests(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-eql-test",
            path: item.path,
            span: item.span,
            message: format!(
                "{} defaults :test to eql; the explicit :test #'eql is redundant",
                item.head
            ),
        });
    }

    let (_, items) = collect_redundant_identity_keys(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-identity-key",
            path: item.path,
            span: item.span,
            message: format!(
                "{} defaults :key to identity; the explicit :key #'identity is redundant",
                item.head
            ),
        });
    }

    let (_, items) = collect_negated_when_unless(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "negated-when-unless",
            path: item.path,
            span: item.span,
            message: format!(
                "{} test is ({} …); use {} on the un-negated test",
                item.head, item.negator, item.suggested_head
            ),
        });
    }

    let (_, items) = collect_negated_comparisons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "negated-comparison",
            path: item.path,
            span: item.span,
            message: format!(
                "negated comparison has a complement operator; use {}",
                item.complement
            ),
        });
    }

    let (_, items) = collect_negated_ifs(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "negated-if",
            path: item.path,
            span: item.span,
            message: "if test is negated; (if (not c) a b) is (if c b a)".to_owned(),
        });
    }

    let (_, items) = collect_if_to_ors(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "if-to-or",
            path: item.path,
            span: item.span,
            message: "if returns its test or the else; (if x x y) is (or x y)".to_owned(),
        });
    }

    let (_, items) = collect_if_nots(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "if-not",
            path: item.path,
            span: item.span,
            message: "if with then=nil and else=t is a negation; (if test nil t) is (not test)"
                .to_owned(),
        });
    }

    let (_, items) = collect_one_step_arithmetic(path, dialect, tree)?;
    for item in items {
        let shorthand = item.shorthand;
        findings.push(LintFinding {
            rule: "one-step-arithmetic",
            path: item.path,
            span: item.span,
            message: format!("add/subtract of 1 has a shorthand; use {shorthand}"),
        });
    }

    let (_, items) = collect_one_armed_ifs(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "one-armed-if",
            path: item.path,
            span: item.span,
            message: "if has no else branch; (if test then) is (when test then)".to_owned(),
        });
    }

    let (_, items) = collect_self_comparisons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "self-comparison",
            path: item.path,
            span: item.span,
            message: format!(
                "{} compares operand {} with itself",
                item.operator, item.operand
            ),
        });
    }

    let (_, items) = collect_nil_comparisons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "nil-comparison",
            path: item.path,
            span: item.span,
            message: format!("{} against nil is a null test; use (null X)", item.operator),
        });
    }

    let (_, items) = collect_t_comparisons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "t-comparison",
            path: item.path,
            span: item.span,
            message: format!(
                "{} against t matches only the symbol T, not any true value",
                item.operator
            ),
        });
    }

    let (_, items) = collect_identical_if_branches(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "identical-if-branches",
            path: item.path,
            span: item.span,
            message: format!("if branches are identical: {}", item.branch),
        });
    }

    let (_, items) = collect_constant_if_tests(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "constant-if-test",
            path: item.path,
            span: item.span,
            message: format!("if test is the constant {}; one branch is dead", item.test),
        });
    }

    let (_, items) = collect_constant_when_tests(path, dialect, tree)?;
    for item in items {
        let message = if item.always_runs {
            format!(
                "{} test is the constant {}; the body always runs, so this is a progn",
                item.head, item.test
            )
        } else {
            format!(
                "{} test is the constant {}; the body never runs, so this is nil",
                item.head, item.test
            )
        };
        findings.push(LintFinding {
            rule: "constant-when-test",
            path: item.path,
            span: item.span,
            message,
        });
    }

    let (_, items) = collect_if_arity_violations(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "if-arity",
            path: item.path,
            span: item.span,
            message: format!("if takes 2 or 3 arguments but has {}", item.argument_count),
        });
    }

    let (_, items) = collect_the_arity_violations(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "the-arity",
            path: item.path,
            span: item.span,
            message: format!(
                "the takes exactly 2 arguments (a type and a form) but has {}",
                item.argument_count
            ),
        });
    }

    let (_, items) = collect_equality_arity_violations(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "equality-arity",
            path: item.path,
            span: item.span,
            message: format!(
                "{} takes exactly 2 arguments but has {}",
                item.operator, item.argument_count
            ),
        });
    }

    let (_, items) = collect_accessor_arity_violations(path, dialect, tree)?;
    for item in items {
        let expected = accessor_arity_phrase(&item);
        findings.push(LintFinding {
            rule: "accessor-arity",
            path: item.path,
            span: item.span,
            message: format!(
                "{} takes {} argument(s) but has {}",
                item.operator, expected, item.argument_count
            ),
        });
    }

    let (_, items) = collect_append_list_to_cons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "append-list-to-cons",
            path: item.path,
            span: item.span,
            message: "a one-element append is just a cons; (append (list x) rest) is (cons x rest)"
                .to_owned(),
        });
    }

    let (_, items) = collect_format_to_strings(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "format-to-string",
            path: item.path,
            span: item.span,
            message: format!(
                "format to a string is just {}; use ({} x)",
                item.replacement, item.replacement
            ),
        });
    }

    let (_, items) = collect_format_newlines(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "format-newline",
            path: item.path,
            span: item.span,
            message: "(format t \"~%\") just writes a newline; use (terpri)".to_owned(),
        });
    }

    let (_, items) = collect_redundant_divisors(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-divisor",
            path: item.path,
            span: item.span,
            message: format!(
                "the divisor defaults to 1; ({} x 1) is ({} x)",
                item.operator, item.operator
            ),
        });
    }

    let (_, items) = collect_duplicate_case_keys(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "duplicate-case-keys",
            path: item.path,
            span: item.span,
            message: format!(
                "{} repeats key {} ({}×)",
                item.head, item.key, item.occurrence_count
            ),
        });
    }

    let (_, items) = collect_quoted_case_keys(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "quoted-case-key",
            path: item.path,
            span: item.span,
            message: format!(
                "{} key {} is quoted; case keys are not evaluated",
                item.head, item.key
            ),
        });
    }

    let (_, items) = collect_case_nil_keys(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "case-nil-key",
            path: item.path,
            span: item.span,
            message: format!(
                "{} clause key nil is the empty key list and never matches; use ((nil) …)",
                item.head
            ),
        });
    }

    let (_, items) = collect_typecase_nil_keys(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "typecase-nil-key",
            path: item.path,
            span: item.span,
            message: format!(
                "{} clause type nil is the empty type and never matches; use null",
                item.head
            ),
        });
    }

    let (_, items) = collect_malformed_case_clauses(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "malformed-case-clause",
            path: item.path,
            span: item.span,
            message: format!(
                "{} clause {} is not a non-empty list",
                item.head, item.clause
            ),
        });
    }

    let (_, items) = collect_unreachable_case_clauses(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "unreachable-case-clause",
            path: item.path,
            span: item.span,
            message: format!(
                "{} has {} unreachable clause(s) after a t/otherwise catch-all",
                item.head, item.unreachable_count
            ),
        });
    }

    let (_, items) = collect_exhaustive_case_otherwise(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "exhaustive-case-otherwise",
            path: item.path,
            span: item.span,
            message: format!(
                "{} does not permit a {} clause (it is exhaustive)",
                item.head, item.designator
            ),
        });
    }

    let (_, items) = collect_duplicate_cond_tests(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "duplicate-cond-tests",
            path: item.path,
            span: item.span,
            message: format!(
                "cond repeats test {} ({}×)",
                item.test, item.occurrence_count
            ),
        });
    }

    let (_, items) = collect_unreachable_cond_clauses(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "unreachable-cond-clause",
            path: item.path,
            span: item.span,
            message: format!(
                "cond has {} unreachable clause(s) after a t clause",
                item.unreachable_count
            ),
        });
    }

    let (_, items) = collect_malformed_cond_clauses(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "malformed-cond-clause",
            path: item.path,
            span: item.span,
            message: format!("cond clause {} is not a non-empty list", item.clause),
        });
    }

    let (_, items) = collect_duplicate_let_bindings(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "duplicate-let-bindings",
            path: item.path,
            span: item.span,
            message: format!(
                "let binds {} more than once ({}×)",
                item.name, item.occurrence_count
            ),
        });
    }

    let (_, items) = collect_malformed_let_bindings(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "malformed-let-binding",
            path: item.path,
            span: item.span,
            message: format!(
                "let binding {} has {} elements; expected a symbol or (var value)",
                item.binding, item.element_count
            ),
        });
    }

    let (_, items) = collect_binds_constant(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "binds-constant",
            path: item.path,
            span: item.span,
            message: format!("{} cannot bind the constant {}", item.head, item.variable),
        });
    }

    let (_, items) = collect_malformed_iteration_specs(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "malformed-iteration-spec",
            path: item.path,
            span: item.span,
            message: format!(
                "{} spec {} must be (var form [result])",
                item.head, item.spec
            ),
        });
    }

    let (_, items) = collect_eval_when_situations(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "eval-when-situation",
            path: item.path,
            span: item.span,
            message: format!("eval-when situation {} is not valid", item.situation),
        });
    }

    let (_, items) = collect_duplicate_boolean_operands(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "duplicate-boolean-operands",
            path: item.path,
            span: item.span,
            message: format!(
                "{} repeats operand {} ({}×)",
                item.head, item.operand, item.occurrence_count
            ),
        });
    }

    let (_, items) = collect_dead_boolean_operands(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "dead-boolean-operand",
            path: item.path,
            span: item.span,
            message: format!(
                "{} short-circuits at literal {}; later operands are dead",
                item.head, item.constant
            ),
        });
    }

    let (_, items) = collect_redundant_boolean_identities(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-boolean-identity",
            path: item.path,
            span: item.span,
            message: format!(
                "{} has a redundant {} operand; drop it",
                item.operator, item.identity
            ),
        });
    }

    let (_, items) = collect_de_morgans(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "de-morgan",
            path: item.path,
            span: item.span,
            message: format!(
                "{} of negations collapses by De Morgan to (not ({} …))",
                item.operator, item.opposite
            ),
        });
    }

    let (_, items) = collect_single_operand_booleans(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "single-operand-boolean",
            path: item.path,
            span: item.span,
            message: format!(
                "{} has a single operand; ({} X) is just X",
                item.operator, item.operator
            ),
        });
    }

    let (_, items) = collect_single_operand_list_ops(path, dialect, tree)?;
    for item in items {
        let head = item.head;
        findings.push(LintFinding {
            rule: "single-operand-list-op",
            path: item.path,
            span: item.span,
            message: format!("{head} of one argument returns it unchanged; ({head} x) is x"),
        });
    }

    let (_, items) = collect_single_operand_arithmetic(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "single-operand-arithmetic",
            path: item.path,
            span: item.span,
            message: format!(
                "{} has a single operand; ({} X) is just X",
                item.operator, item.operator
            ),
        });
    }

    let (_, items) = collect_eql_string_comparisons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "eql-string-comparison",
            path: item.path,
            span: item.span,
            message: format!(
                "{} compares against string literal {}",
                item.operator, item.literal
            ),
        });
    }

    let (_, items) = collect_eql_list_comparisons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "eql-list-comparison",
            path: item.path,
            span: item.span,
            message: format!(
                "{} compares against quoted list literal {}",
                item.operator, item.literal
            ),
        });
    }

    let (_, items) = collect_eql_search_literals(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "eql-search-literal",
            path: item.path,
            span: item.span,
            message: format!(
                "{} searches for literal {} with the default eql test; add :test",
                item.operator, item.literal
            ),
        });
    }

    let (_, items) = collect_eq_number_comparisons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "eq-number-comparison",
            path: item.path,
            span: item.span,
            message: format!("eq compares against number literal {}", item.literal),
        });
    }

    let (_, items) = collect_eq_char_comparisons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "eq-char-comparison",
            path: item.path,
            span: item.span,
            message: format!(
                "eq compares against character literal {}; use eql or char=",
                item.literal
            ),
        });
    }

    let (_, items) = collect_single_arg_comparisons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "single-arg-comparison",
            path: item.path,
            span: item.span,
            message: format!(
                "{} has a single argument; the comparison is always true (missing an operand?)",
                item.operator
            ),
        });
    }

    let (_, items) = collect_sign_comparisons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "sign-comparison",
            path: item.path,
            span: item.span,
            message: format!(
                "comparison against 0 has a dedicated predicate; use {}",
                item.predicate
            ),
        });
    }

    let (_, items) = collect_single_clause_conds(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "single-clause-cond",
            path: item.path,
            span: item.span,
            message:
                "single-clause cond with a body is just when; (cond (test a b)) is (when test a b)"
                    .to_owned(),
        });
    }

    let (_, items) = collect_cond_t_clauses(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "cond-t-clause",
            path: item.path,
            span: item.span,
            message: "single t-clause cond is just progn; (cond (t a b)) is (progn a b)".to_owned(),
        });
    }

    let (_, items) = collect_single_value_binds(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "single-value-bind",
            path: item.path,
            span: item.span,
            message: "multiple-value-bind of one variable is just let; (multiple-value-bind (x) f body) is (let ((x f)) body)"
                .to_owned(),
        });
    }

    let (_, items) = collect_format_missing_destinations(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "format-missing-destination",
            path: item.path,
            span: item.span,
            message: format!(
                "format destination is the string literal {}; a nil/t/stream destination is missing",
                item.literal
            ),
        });
    }

    let (_, items) = collect_literal_places(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "literal-place",
            path: item.path,
            span: item.span,
            message: format!(
                "{} place {} is a literal and cannot be modified",
                item.operator, item.place
            ),
        });
    }

    let (_, items) = collect_destructive_literals(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "destructive-literal",
            path: item.path,
            span: item.span,
            message: format!(
                "{} destructively modifies quoted literal {} (undefined behavior)",
                item.operator, item.literal
            ),
        });
    }

    let (_, items) = collect_char_op_strings(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "char-op-string",
            path: item.path,
            span: item.span,
            message: format!(
                "{} is given string literal {}; it requires a character (type error)",
                item.operator, item.literal
            ),
        });
    }

    let (_, items) = collect_empty_bodies(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "empty-body",
            path: item.path,
            span: item.span,
            message: format!(
                "{} has no body; the test/spec runs, then nothing",
                item.head
            ),
        });
    }

    let (_, items) = collect_identity_arithmetic(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "identity-arithmetic",
            path: item.path,
            span: item.span,
            message: format!(
                "{} has a redundant identity operand {} (it does nothing)",
                item.operator, item.identity
            ),
        });
    }

    let (_, items) = collect_verbose_negations(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "verbose-negation",
            path: item.path,
            span: item.span,
            message: "negation written the long way; use (- x)".to_owned(),
        });
    }

    let (_, items) = collect_list_star_to_cons(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "list-star-to-cons",
            path: item.path,
            span: item.span,
            message: "a two-argument list* is just a cons; (list* a b) is (cons a b)".to_owned(),
        });
    }

    let (_, items) = collect_values_list_of_lists(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "values-list-of-list",
            path: item.path,
            span: item.span,
            message: "values-list of a fresh list is just values; (values-list (list a b)) is (values a b)"
                .to_owned(),
        });
    }

    let (_, items) = collect_redundant_prog1s(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-prog1",
            path: item.path,
            span: item.span,
            message: "a prog1 wrapping a single form is just that form; (prog1 x) is x".to_owned(),
        });
    }

    let (_, items) = collect_subseq_zeros(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "subseq-zero",
            path: item.path,
            span: item.span,
            message:
                "a subseq from index 0 copies the whole sequence; (subseq seq 0) is (copy-seq seq)"
                    .to_owned(),
        });
    }

    let (_, items) = collect_car_nthcdrs(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "car-nthcdr",
            path: item.path,
            span: item.span,
            message: "car of an nthcdr is nth; (car (nthcdr n x)) is (nth n x)".to_owned(),
        });
    }

    let (_, items) = collect_car_reverses(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "car-reverse",
            path: item.path,
            span: item.span,
            message:
                "car of a reverse copies the whole list to read one element; use (car (last x))"
                    .to_owned(),
        });
    }

    let (_, items) = collect_append_nils(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "append-nil",
            path: item.path,
            span: item.span,
            message: "append with a nil tail is a fresh copy; (append x nil) is (copy-list x)"
                .to_owned(),
        });
    }

    let (_, items) = collect_multiple_value_list_of_values(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "multiple-value-list-of-values",
            path: item.path,
            span: item.span,
            message: "multiple-value-list of a values form is just list; (multiple-value-list (values a b)) is (list a b)"
                .to_owned(),
        });
    }

    let (_, items) = collect_typep_predicates(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "typep-predicate",
            path: item.path,
            span: item.span,
            message: format!(
                "typep against this type has a dedicated predicate; use ({} x)",
                item.predicate
            ),
        });
    }

    let (_, items) = collect_coerce_to_ts(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "coerce-to-t",
            path: item.path,
            span: item.span,
            message: "coerce to type t returns the object unchanged; (coerce x t) is x".to_owned(),
        });
    }

    let (_, items) = collect_gethash_defaults(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "gethash-default",
            path: item.path,
            span: item.span,
            message: "the gethash default is nil; (gethash k h nil) is (gethash k h)".to_owned(),
        });
    }

    let (_, items) = collect_make_hash_table_tests(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "make-hash-table-test",
            path: item.path,
            span: item.span,
            message: "the make-hash-table :test defaults to eql; drop the explicit :test 'eql"
                .to_owned(),
        });
    }

    let (_, items) = collect_zero_divisors(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "zero-divisor",
            path: item.path,
            span: item.span,
            message: format!(
                "{} by a literal 0 always signals division-by-zero",
                item.operator
            ),
        });
    }

    let (_, items) = collect_duplicate_keywords(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "duplicate-keyword",
            path: item.path,
            span: item.span,
            message: format!(
                "keyword {} is passed more than once; the leftmost value wins",
                item.keyword
            ),
        });
    }

    let (_, items) = collect_defpackage_quoted(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "defpackage-quoted",
            path: item.path,
            span: item.span,
            message: format!(
                "defpackage does not evaluate its options; drop the quote in the {} clause",
                item.clause
            ),
        });
    }

    let (_, items) = collect_step_zeros(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "step-zero",
            path: item.path,
            span: item.span,
            message: format!("{} by 0 is a no-op that changes nothing", item.operator),
        });
    }

    let (_, items) = collect_if_to_unless(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "if-to-unless",
            path: item.path,
            span: item.span,
            message: "an if with a nil then-branch is an unless; (if c nil e) is (unless c e)"
                .to_owned(),
        });
    }

    let (_, items) = collect_prog2_to_progn(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "prog2-to-progn",
            path: item.path,
            span: item.span,
            message: "a two-form prog2 is just progn; (prog2 a b) is (progn a b)".to_owned(),
        });
    }

    let (_, items) = collect_handler_case_no_clauses(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "handler-case-no-clauses",
            path: item.path,
            span: item.span,
            message: "a handler-case with no clauses is just its body; (handler-case x) is x"
                .to_owned(),
        });
    }

    let (_, items) = collect_unwind_protect_no_cleanup(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "unwind-protect-no-cleanup",
            path: item.path,
            span: item.span,
            message: "an unwind-protect with no cleanup is just its body; (unwind-protect x) is x"
                .to_owned(),
        });
    }

    let (_, items) = collect_redundant_start_zeros(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-start-zero",
            path: item.path,
            span: item.span,
            message: format!(
                "{} :start defaults to 0; drop the explicit :start 0",
                item.head
            ),
        });
    }

    let (_, items) = collect_redundant_end_nils(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-end-nil",
            path: item.path,
            span: item.span,
            message: format!(
                "{} :end defaults to nil; drop the explicit :end nil",
                item.head
            ),
        });
    }

    let (_, items) = collect_redundant_from_end_nils(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-from-end-nil",
            path: item.path,
            span: item.span,
            message: format!(
                "{} :from-end defaults to nil; drop the explicit :from-end nil",
                item.head
            ),
        });
    }

    let (_, items) = collect_redundant_count_nils(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "redundant-count-nil",
            path: item.path,
            span: item.span,
            message: format!(
                "{} :count defaults to nil; drop the explicit :count nil",
                item.head
            ),
        });
    }

    let (_, items) = collect_string_case_folds(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "string-case-fold",
            path: item.path,
            span: item.span,
            message: "case-folding both sides of string= is case-insensitive; use string-equal"
                .to_owned(),
        });
    }

    let (_, items) = collect_char_case_folds(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "char-case-fold",
            path: item.path,
            span: item.span,
            message: "case-folding both sides of char= is case-insensitive; use char-equal"
                .to_owned(),
        });
    }

    let (_, items) = collect_nested_string_cases(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "nested-string-case",
            path: item.path,
            span: item.span,
            message: "the outer string case op dominates; the inner one is dead work".to_owned(),
        });
    }

    let (_, items) = collect_code_char_char_codes(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "code-char-char-code",
            path: item.path,
            span: item.span,
            message: "code-char of char-code is a round-trip; (code-char (char-code c)) is c"
                .to_owned(),
        });
    }

    let (_, items) = collect_last_default_counts(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "last-default-count",
            path: item.path,
            span: item.span,
            message: "explicit count of 1 restates last's default; (last x 1) is (last x)"
                .to_owned(),
        });
    }

    let (_, items) = collect_butlast_default_counts(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "butlast-default-count",
            path: item.path,
            span: item.span,
            message: "explicit count of 1 restates butlast's default; (butlast x 1) is (butlast x)"
                .to_owned(),
        });
    }

    let (_, items) = collect_make_list_default_elements(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "make-list-default-element",
            path: item.path,
            span: item.span,
            message: "explicit :initial-element nil restates make-list's default; drop it"
                .to_owned(),
        });
    }

    let (_, items) = collect_parse_integer_default_radixes(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "parse-integer-default-radix",
            path: item.path,
            span: item.span,
            message: "explicit :radix 10 restates parse-integer's default; drop it".to_owned(),
        });
    }

    let (_, items) = collect_getf_default_nils(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "getf-default-nil",
            path: item.path,
            span: item.span,
            message: "explicit nil default restates getf's default; (getf p k nil) is (getf p k)"
                .to_owned(),
        });
    }

    let (_, items) = collect_make_array_default_keywords(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "make-array-default-keyword",
            path: item.path,
            span: item.span,
            message: format!(
                "explicit {} nil restates make-array's default; drop it",
                item.keyword
            ),
        });
    }

    let (_, items) = collect_nested_char_cases(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "nested-char-case",
            path: item.path,
            span: item.span,
            message: "the outer char case op dominates; the inner one is dead work".to_owned(),
        });
    }

    let (_, items) = collect_list_star_nils(path, dialect, tree)?;
    for item in items {
        findings.push(LintFinding {
            rule: "list-star-nil",
            path: item.path,
            span: item.span,
            message: "list* with a nil tail is a spelled-out list; (list* a b nil) is (list a b)"
                .to_owned(),
        });
    }

    Ok(findings)
}

/// Resolves the active rule set for one `inspect lint` run. The included set is
/// the rules named by `only`, or every rule in the categories named by
/// `categories`, or (when both are empty) all of [`RULES`]; `exclude` then
/// removes rules from it. `only` and `categories` are mutually exclusive at the
/// CLI layer. Every named rule must be one of [`RULES`] and every named
/// category one of [`CATEGORIES`] — an unknown name is a hard error rather than
/// a silent no-op, so a typo in CI fails loudly. The result preserves
/// [`RULES`] order.
pub fn resolve_active_rules(
    only: &[String],
    exclude: &[String],
    categories: &[String],
) -> Result<Vec<&'static str>> {
    for name in only.iter().chain(exclude) {
        if !RULES.contains(&name.as_str()) {
            anyhow::bail!(
                "unknown lint rule {name:?}; valid rules: {}",
                RULES.join(", ")
            );
        }
    }
    for name in categories {
        if !CATEGORIES.contains(&name.as_str()) {
            anyhow::bail!(
                "unknown lint category {name:?}; valid categories: {}",
                CATEGORIES.join(", ")
            );
        }
    }

    Ok(RULES
        .iter()
        .copied()
        .filter(|rule| {
            let included = if !only.is_empty() {
                only.iter().any(|name| name == rule)
            } else if !categories.is_empty() {
                rule_category(rule)
                    .is_some_and(|category| categories.iter().any(|name| name == category))
            } else {
                true
            };
            included && !exclude.iter().any(|name| name == rule)
        })
        .collect())
}

/// Summarizes findings, keeping only those from `active` rules; `per_rule`
/// lists exactly the active rules (in [`RULES`] order) so the checklist
/// reflects what actually ran.
pub fn summarize_lint_findings(findings: Vec<LintFinding>, active: &[&str]) -> LintSummary {
    let findings: Vec<LintFinding> = findings
        .into_iter()
        .filter(|finding| active.contains(&finding.rule))
        .collect();

    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for finding in &findings {
        *counts.entry(finding.rule).or_insert(0) += 1;
    }
    let per_rule = RULES
        .iter()
        .copied()
        .filter(|rule| active.contains(rule))
        .map(|rule| (rule, counts.get(&rule).copied().unwrap_or(0)))
        .collect();

    LintSummary {
        finding_count: findings.len(),
        per_rule,
        findings,
    }
}

pub fn evaluate_lint_policy(options: LintPolicyOptions, summary: &LintSummary) -> LintPolicy {
    let finding_count = summary.finding_count;
    let mut violations = Vec::new();
    if options.fail_on_finding() && finding_count > 0 {
        violations.push(format!("finding_count {finding_count} exceeds 0"));
    }
    if let Some(threshold) = options.fail_on_severity() {
        let count = summary
            .findings
            .iter()
            .filter(|finding| rule_severity(finding.rule).at_least(threshold))
            .count();
        if count > 0 {
            violations.push(format!(
                "{count} finding(s) at severity {} or higher",
                threshold.as_str()
            ));
        }
    }

    LintPolicy {
        fail_on_finding: options.fail_on_finding(),
        finding_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(input: &str) -> Vec<LintFinding> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_lint_findings(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect lint findings")
    }

    #[test]
    fn rule_docs_cover_every_rule() {
        // Descriptions and categories stay in lockstep with the rule list.
        let documented: Vec<&str> = RULE_DOCS.iter().map(|(rule, _, _)| *rule).collect();
        assert_eq!(documented, RULES.to_vec());
        for rule in RULES {
            assert!(
                rule_description(rule).is_some_and(|doc| !doc.is_empty()),
                "rule {rule} has no description"
            );
            let category = rule_category(rule).expect("rule has a category");
            assert!(
                CATEGORIES.contains(&category),
                "rule {rule} has unknown category {category}"
            );
        }
        assert_eq!(rule_description("no-such-rule"), None);
        assert_eq!(rule_category("no-such-rule"), None);
        // Every fixable rule is a real rule, and the set is non-empty.
        assert!(!FIXABLE_RULES.is_empty());
        for rule in FIXABLE_RULES {
            assert!(RULES.contains(&rule), "fixable rule {rule} is not in RULES");
            assert!(rule_is_fixable(rule));
        }
        assert!(!rule_is_fixable("if-arity"));
        assert!(!rule_is_fixable("no-such-rule"));
        // Every warning rule is a real rule; everything else is an error.
        for rule in WARNING_RULES {
            assert!(RULES.contains(&rule), "warning rule {rule} is not in RULES");
            assert_eq!(rule_severity(rule), Severity::Warning);
        }
        assert_eq!(rule_severity("if-arity"), Severity::Error);
        assert_eq!(rule_severity("literal-place"), Severity::Error);
        // CATEGORIES is sorted and deduplicated, and every category is used.
        let mut sorted = CATEGORIES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, CATEGORIES.to_vec());
        for category in CATEGORIES {
            assert!(
                RULES
                    .iter()
                    .any(|rule| rule_category(rule) == Some(category)),
                "category {category} has no rules"
            );
        }
    }

    #[test]
    fn aggregates_findings_from_multiple_rules() {
        let found = findings("(progn (setq x x) (eql y \"z\"))");
        let rules: Vec<&str> = found.iter().map(|finding| finding.rule).collect();
        assert!(rules.contains(&"self-assignment"));
        assert!(rules.contains(&"eql-string-comparison"));
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn a_clean_file_has_no_findings() {
        let found = findings("(defun f (x y) (+ x y))");
        assert!(found.is_empty());
    }

    #[test]
    fn per_rule_lists_every_active_rule_even_with_zero_findings() {
        let summary = summarize_lint_findings(findings("(setq x x)"), &RULES);
        assert_eq!(summary.per_rule.len(), RULES.len());
        assert_eq!(
            summary
                .per_rule
                .iter()
                .find(|(rule, _)| *rule == "self-assignment")
                .map(|(_, count)| *count),
            Some(1)
        );
        assert_eq!(summary.finding_count, 1);
    }

    #[test]
    fn resolve_active_rules_defaults_to_every_rule() {
        let active = resolve_active_rules(&[], &[], &[]).expect("resolve");
        assert_eq!(active, RULES.to_vec());
    }

    #[test]
    fn resolve_active_rules_honors_only() {
        let active =
            resolve_active_rules(&["self-assignment".to_owned()], &[], &[]).expect("resolve");
        assert_eq!(active, vec!["self-assignment"]);
    }

    #[test]
    fn resolve_active_rules_honors_exclude() {
        let active =
            resolve_active_rules(&[], &["self-assignment".to_owned()], &[]).expect("resolve");
        assert!(!active.contains(&"self-assignment"));
        assert_eq!(active.len(), RULES.len() - 1);
    }

    #[test]
    fn resolve_active_rules_honors_category() {
        let active = resolve_active_rules(&[], &[], &["arity".to_owned()]).expect("resolve");
        assert_eq!(
            active,
            vec![
                "setf-arity",
                "modify-macro-arity",
                "if-arity",
                "the-arity",
                "equality-arity",
                "accessor-arity"
            ]
        );
    }

    #[test]
    fn resolve_active_rules_category_minus_exclude() {
        let active = resolve_active_rules(&[], &["if-arity".to_owned()], &["arity".to_owned()])
            .expect("resolve");
        assert_eq!(
            active,
            vec![
                "setf-arity",
                "modify-macro-arity",
                "the-arity",
                "equality-arity",
                "accessor-arity"
            ]
        );
    }

    #[test]
    fn resolve_active_rules_rejects_an_unknown_rule() {
        assert!(resolve_active_rules(&["not-a-rule".to_owned()], &[], &[]).is_err());
    }

    #[test]
    fn resolve_active_rules_rejects_an_unknown_category() {
        assert!(resolve_active_rules(&[], &[], &["not-a-category".to_owned()]).is_err());
    }

    #[test]
    fn only_selection_filters_findings_to_the_named_rules() {
        // The file has both a self-assignment and an eql-string finding; a
        // `--rule self-assignment` run reports only the former.
        let found = findings("(progn (setq x x) (eql y \"z\"))");
        let summary = summarize_lint_findings(found, &["self-assignment"]);
        assert_eq!(summary.finding_count, 1);
        assert_eq!(summary.per_rule, vec![("self-assignment", 1)]);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(setq x x)").expect("parse input");
        let found = collect_lint_findings(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
            .expect("collect lint findings");
        assert!(found.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let summary = summarize_lint_findings(findings("(setq x x)"), &RULES);

        let quiet = evaluate_lint_policy(LintPolicyOptions::new(false, None), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.finding_count, 1);

        let strict = evaluate_lint_policy(LintPolicyOptions::new(true, None), &summary);
        assert!(!strict.passed);
    }

    #[test]
    fn severity_gate_fails_only_at_or_above_the_threshold() {
        // self-assignment is an error; redundant-quote is a warning.
        let error_only = summarize_lint_findings(findings("(setq x x)"), &RULES);
        let warning_only = summarize_lint_findings(findings("(list '5)"), &RULES);

        // --fail-on error trips on the error, not on the warning.
        let opts = LintPolicyOptions::new(false, Some(Severity::Error));
        assert!(!evaluate_lint_policy(opts, &error_only).passed);
        assert!(evaluate_lint_policy(opts, &warning_only).passed);

        // --fail-on warning trips on either (warning is the lower bar).
        let opts = LintPolicyOptions::new(false, Some(Severity::Warning));
        assert!(!evaluate_lint_policy(opts, &error_only).passed);
        assert!(!evaluate_lint_policy(opts, &warning_only).passed);
    }
}
