#![doc = include_str!("../README.md")]

pub mod contradictory_optimize;
pub mod defclass_slot_option;
pub mod definition_naming;
pub mod destructive_function_naming;
pub mod effectively_constant_variable;
pub mod ignore_declaration_conflict;
pub mod method_lambda_list_mismatch;
pub mod missing_docstring;

// One module per rule: these rules have no standalone `inspect <rule>` command,
// so the domain/usecase/cli split the older lint packages use would be
// indirection with one consumer on the other end.
//
// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2).
