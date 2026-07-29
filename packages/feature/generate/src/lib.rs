#![doc = include_str!("../README.md")]

pub mod generate_accessors;
pub mod generate_defgeneric;
pub mod generate_defpackage;
pub mod generate_defsystem;
pub mod generate_docstring;
pub mod generate_tests;

// The composition root sees each slice's Args type and run fn (section 4.2).
pub use generate_accessors::cli::{GenerateAccessorsArgs, generate_accessors};
pub use generate_defgeneric::cli::{GenerateDefgenericArgs, generate_defgeneric};
pub use generate_defpackage::cli::{GenerateDefpackageArgs, generate_defpackage};
pub use generate_defsystem::cli::{GenerateDefsystemArgs, generate_defsystem};
pub use generate_docstring::cli::{GenerateDocstringArgs, generate_docstring};
pub use generate_tests::cli::{GenerateTestsArgs, generate_tests};
