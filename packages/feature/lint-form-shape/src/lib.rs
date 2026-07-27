#![doc = include_str!("../README.md")]

pub mod butlast_default_count;
pub mod coerce_to_t;
pub mod defpackage_quoted;
pub mod duplicate_keyword;
pub mod duplicate_lambda_list_keyword;
pub mod duplicate_let_bindings;
pub mod duplicate_parameters;
pub mod duplicate_setf_places;
pub mod empty_let;
pub mod funcall_lambda;
pub mod getf_default_nil;
pub mod gethash_default;
pub mod lambda_list_keyword_order;
pub mod make_array_default_keyword;
pub mod make_hash_table_test;
pub mod make_list_default_element;
pub mod malformed_let_binding;
pub mod manual_incf;
pub mod manual_push;
pub mod manual_pushnew;
pub mod multiple_value_list_of_values;
pub mod nested_char_case;
pub mod nested_cxr;
pub mod parse_integer_default_radix;
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
pub mod the_arity;
pub mod typep_predicate;
pub mod values_list_of_list;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.
