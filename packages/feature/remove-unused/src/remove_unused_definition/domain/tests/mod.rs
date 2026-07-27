use std::path::PathBuf;

use proptest::prelude::*;

use super::*;
use crate::domain::definition::DefinitionCategory;
use crate::domain::dialect::Dialect;
use crate::domain::package_report::PackageDefinitionReport;
use crate::domain::sexpr::{ByteOffset, ByteSpan, SyntaxTree};

mod basic;
mod policy;
mod property;
mod shadowing;
mod validation;

fn file_with_definitions(
    text: &str,
    definitions: Vec<UnusedDefinitionDefinition>,
) -> RemoveUnusedDefinitionInputFile {
    file_with_text(PathBuf::from("core.lisp"), text, definitions)
}

fn file_with_text(
    path: PathBuf,
    text: &str,
    definitions: Vec<UnusedDefinitionDefinition>,
) -> RemoveUnusedDefinitionInputFile {
    file_with_dialect(path, Dialect::CommonLisp, Some("app"), text, definitions)
}

fn file_with_dialect(
    path: PathBuf,
    dialect: Dialect,
    package: Option<&str>,
    text: &str,
    definitions: Vec<UnusedDefinitionDefinition>,
) -> RemoveUnusedDefinitionInputFile {
    let tree = SyntaxTree::parse_with_dialect(text, dialect).expect("fixture must parse");
    RemoveUnusedDefinitionInputFile {
        path,
        dialect,
        package: package.map(str::to_owned),
        definitions,
        atoms: tree.atom_occurrences(),
        text: text.to_owned(),
    }
}

fn definition(
    text: &str,
    form: &str,
    name: &str,
    category: DefinitionCategory,
) -> UnusedDefinitionDefinition {
    let start = text.find(form).expect("form must exist");
    UnusedDefinitionDefinition {
        path: "0".to_owned(),
        span: ByteSpan::new(ByteOffset::new(start), ByteOffset::new(start + form.len())),
        head: match category {
            DefinitionCategory::Test => "deftest",
            _ => "defun",
        }
        .to_owned(),
        name: Some(name.to_owned()),
        category,
        parameter_count: Some(0),
        body_form_count: Some(1),
        package: Some("app".to_owned()),
    }
}

fn request_for(
    text: &str,
    definitions: Vec<UnusedDefinitionDefinition>,
) -> RemoveUnusedDefinitionsRequest {
    RemoveUnusedDefinitionsRequest {
        files: vec![file_with_definitions(text, definitions)],
        package_definitions: Vec::new(),
        include_protected: false,
        include_exported: false,
    }
}

fn lisp_symbol_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,12}".prop_filter("avoid obvious generated edge names", |name| {
        !matches!(
            name.as_str(),
            "defun" | "in-package" | "lambda" | "nil" | "t"
        )
    })
}
