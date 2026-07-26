use std::path::PathBuf;

use proptest::{prelude::*, test_runner::TestCaseError};

use super::rewrite::append_top_level_definitions;
use super::*;
use crate::domain::definition::DefinitionCategory;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Path, SyntaxTree};

fn split_request<'a>(from_input: &'a str, to_input: &'a str) -> SplitFileRequest<'a> {
    SplitFileRequest {
        from_file: PathBuf::from("src/core.lisp"),
        to_file: PathBuf::from("src/ui/render.lisp"),
        from_input,
        to_input,
        from_dialect: Dialect::CommonLisp,
        to_dialect: Dialect::CommonLisp,
        paths: vec![Path::from_indexes(vec![1]), Path::from_indexes(vec![2])],
        names: Vec::new(),
        categories: Vec::new(),
        to_file_existed: false,
        to_parent_existed: false,
        write: false,
    }
}

fn generated_split_source(definition_count: usize) -> String {
    let mut input = "(in-package #:demo)\n\n(defun keep () :keep)\n".to_owned();
    for index in 0..definition_count {
        input.push_str(&format!("\n(defun moved-{index} (arg) (+ arg {index}))\n"));
    }
    input
}

#[test]
fn plan_split_file_moves_selected_definitions_in_source_order() {
    let plan = plan_split_file(split_request(
        "(in-package #:demo)\n\n(defun keep () :keep)\n\n(defun render () :render)\n\n(defmacro with-render () nil)\n",
        "",
    ))
    .expect("split-file plan should be valid");

    assert_eq!(plan.items.len(), 2);
    assert_eq!(plan.items[0].definition.name.as_deref(), Some("keep"));
    assert_eq!(plan.items[1].definition.name.as_deref(), Some("render"));
    assert!(plan.from_rewritten.contains("in-package"));
    assert!(!plan.from_rewritten.contains("defun keep"));
    assert!(!plan.from_rewritten.contains("defun render"));
    assert!(plan.to_rewritten.contains("(defun keep () :keep)"));
    assert!(plan.to_rewritten.contains("(defun render () :render)"));
    assert!(plan.to_rewritten.find("defun keep") < plan.to_rewritten.find("defun render"));
    assert!(plan.changed);
    assert!(!plan.written);
}

#[test]
fn plan_split_file_injects_in_package_header_into_new_destination() {
    let plan = plan_split_file(split_request(
        "(in-package #:demo)\n\n(defun keep () :keep)\n\n(defun render () :render)\n\n(defmacro with-render () nil)\n",
        "",
    ))
    .expect("split-file plan should be valid");

    assert!(
        plan.to_rewritten.contains("(in-package #:demo)"),
        "new destination file must declare the source package, got: {}",
        plan.to_rewritten
    );
    let package_index = plan
        .to_rewritten
        .find("(in-package #:demo)")
        .expect("in-package present");
    let first_definition_index = plan
        .to_rewritten
        .find("(defun keep () :keep)")
        .expect("first moved definition present");
    assert!(package_index < first_definition_index);
    assert_eq!(plan.to_rewritten.matches("in-package").count(), 1);
}

#[test]
fn plan_split_file_skips_in_package_header_when_destination_already_matches() {
    let plan = plan_split_file(split_request(
        "(in-package #:demo)\n\n(defun keep () :keep)\n\n(defun render () :render)\n\n(defmacro with-render () nil)\n",
        "(in-package #:demo)\n\n(defun boot () :boot)\n",
    ))
    .expect("split-file plan should be valid");

    assert_eq!(
        plan.to_rewritten.matches("in-package").count(),
        1,
        "must not duplicate an already-matching in-package header, got: {}",
        plan.to_rewritten
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn pbt_plan_split_file_preserves_parseability_and_order(definition_count in 1usize..8) {
        let source = generated_split_source(definition_count);
        let mut request = split_request(&source, "(in-package #:demo.moved)\n");
        request.paths = Vec::new();
        request.names = (0..definition_count)
            .map(|index| format!("moved-{index}"))
            .collect();

        let plan = plan_split_file(request)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(plan.items.len(), definition_count);
        prop_assert!(plan.changed);
        prop_assert!(!plan.written);
        prop_assert!(SyntaxTree::parse(&plan.from_rewritten).is_ok());
        prop_assert!(SyntaxTree::parse(&plan.to_rewritten).is_ok());
        prop_assert!(plan.from_rewritten.contains("defun keep"));

        let mut previous_position = 0;
        for index in 0..definition_count {
            let needle = format!("defun moved-{index}");
            prop_assert!(!plan.from_rewritten.contains(&needle));
            let position = plan.to_rewritten.find(&needle)
                .ok_or_else(|| TestCaseError::fail(format!("{needle} missing from destination")))?;
            prop_assert!(position >= previous_position);
            previous_position = position;
        }
    }
}

#[test]
fn plan_split_file_rejects_duplicate_paths() {
    let mut request = split_request("(defun a () 1)\n", "");
    request.paths = vec![Path::from_indexes(vec![0]), Path::from_indexes(vec![0])];

    let error = plan_split_file(request).expect_err("duplicate paths should be rejected");

    assert!(error.to_string().contains("duplicate split-file path"));
}

#[test]
fn plan_split_file_selects_definitions_by_name_and_kind() {
    let mut request = split_request(
        "(in-package #:demo)\n\n(defun keep () :keep)\n\n(defun render () :render)\n\n(defmacro with-render () nil)\n\n(define-symbol-macro current-user (slot-value *session* 'user))\n",
        "",
    );
    request.paths = Vec::new();
    request.names = vec!["render".to_owned()];
    request.categories = vec![DefinitionCategory::Macro, DefinitionCategory::Variable];

    let plan = plan_split_file(request).expect("name and kind selectors should be valid");

    assert_eq!(plan.items.len(), 3);
    assert_eq!(plan.items[0].definition.name.as_deref(), Some("render"));
    assert_eq!(
        plan.items[1].definition.name.as_deref(),
        Some("with-render")
    );
    assert_eq!(
        plan.items[2].definition.name.as_deref(),
        Some("current-user")
    );
    assert_eq!(
        plan.items[2].definition.category,
        DefinitionCategory::Variable
    );
    assert!(plan.from_rewritten.contains("defun keep"));
    assert!(!plan.from_rewritten.contains("defun render"));
    assert!(!plan.from_rewritten.contains("defmacro with-render"));
    assert!(
        !plan
            .from_rewritten
            .contains("define-symbol-macro current-user")
    );
}

#[test]
fn plan_split_file_rejects_missing_name_selector() {
    let mut request = split_request("(defun a () 1)\n", "");
    request.paths = Vec::new();
    request.names = vec!["missing".to_owned()];

    let error = plan_split_file(request).expect_err("missing selector should fail");

    assert!(error.to_string().contains("--name did not match"));
}

#[test]
fn plan_split_file_requires_at_least_one_selector() {
    let mut request = split_request("(defun a () 1)\n", "");
    request.paths = Vec::new();

    let error = plan_split_file(request).expect_err("empty selectors should fail");

    assert!(error.to_string().contains("requires at least one"));
}

#[test]
fn append_top_level_definitions_keeps_spacing() {
    let definitions = vec![
        "(defun moved-a () 1)".to_owned(),
        "(defmacro moved-b () 2)".to_owned(),
    ];

    assert_eq!(
        append_top_level_definitions("(in-package #:demo)\n\n", &definitions),
        "(in-package #:demo)\n\n(defun moved-a () 1)\n\n(defmacro moved-b () 2)\n"
    );
}

#[test]
fn append_top_level_definitions_creates_destination_from_empty_input() {
    let definitions = vec!["(defun moved () :ok)".to_owned()];

    assert_eq!(
        append_top_level_definitions("", &definitions),
        "(defun moved () :ok)\n"
    );
}

#[test]
fn plan_split_file_moves_a_definitions_leading_comment_with_it() {
    let mut request = split_request(
        "(defun render-widget (w) w)\n\n\
         ;; Counts the widgets in a list.\n\
         (defun widget-count (widgets) (length widgets))\n\n\
         (defun widget-noop () nil)\n",
        "",
    );
    request.paths = vec![Path::from_indexes(vec![1])];
    let plan = plan_split_file(request).expect("split-file plan should be valid");

    assert!(
        !plan
            .from_rewritten
            .contains(";; Counts the widgets in a list."),
        "comment must move with its definition, got: {}",
        plan.from_rewritten
    );
    assert!(
        plan.to_rewritten
            .contains(";; Counts the widgets in a list.\n(defun widget-count"),
        "moved comment must stay directly above its definition, got: {}",
        plan.to_rewritten
    );
}

#[test]
fn plan_split_file_does_not_glue_remaining_definitions_when_removing_a_middle_item() {
    let mut request = split_request(
        "(defun render-widget (w) w)\n\n\
         ;; Counts the widgets in a list.\n\
         (defun widget-count (widgets) (length widgets))\n\n\
         ;; Trailing helper, unrelated.\n\
         (defun widget-noop () nil)\n",
        "",
    );
    request.paths = vec![Path::from_indexes(vec![1])];
    let plan = plan_split_file(request).expect("split-file plan should be valid");

    assert!(
        !plan.from_rewritten.contains(")(defun"),
        "remaining definitions must not be glued together: {:?}",
        plan.from_rewritten
    );
    assert!(SyntaxTree::parse(&plan.from_rewritten).is_ok());
}

#[test]
fn plan_split_file_preserves_unselected_common_lisp_vector_literals() {
    let mut request = split_request(
        "(defun moved () :moved)\n\n(defun keep () #(one two))\n",
        "",
    );
    request.paths = vec![Path::from_indexes(vec![0])];

    let plan = plan_split_file(request).expect("split-file plan should be valid");

    assert!(plan.from_rewritten.contains("#(one two)"));
    assert!(plan.to_rewritten.contains("(defun moved () :moved)"));
    assert!(SyntaxTree::parse(&plan.from_rewritten).is_ok());
    assert!(SyntaxTree::parse(&plan.to_rewritten).is_ok());
}

#[test]
fn plan_split_file_uses_each_file_dialect_for_character_literal_collisions() {
    for (case_name, dialect, character_literal) in [
        ("Common Lisp", Dialect::CommonLisp, "#\\)"),
        ("Emacs Lisp", Dialect::EmacsLisp, "?\\)"),
    ] {
        let from_input =
            format!("(defun keep () {character_literal})\n\n(defun moved () :moved)\n");
        let to_input = format!("(defun existing () {character_literal})\n");
        let mut request = split_request(&from_input, &to_input);
        request.from_dialect = dialect;
        request.to_dialect = dialect;
        request.paths = vec![Path::from_indexes(vec![1])];

        let plan = plan_split_file(request)
            .unwrap_or_else(|error| panic!("{case_name} split should succeed: {error:#}"));

        assert!(plan.from_rewritten.contains(character_literal));
        assert!(plan.to_rewritten.contains(character_literal));
        assert!(plan.to_rewritten.contains("(defun moved () :moved)"));
        SyntaxTree::parse_with_dialect(&plan.from_rewritten, dialect)
            .unwrap_or_else(|error| panic!("{case_name} source output should parse: {error:#}"));
        SyntaxTree::parse_with_dialect(&plan.to_rewritten, dialect).unwrap_or_else(|error| {
            panic!("{case_name} destination output should parse: {error:#}")
        });
    }
}

#[test]
fn plan_split_file_preserves_unknown_generic_syntax_compatibility() {
    let mut request = split_request(
        "(defn keep [] :kept)\n\n(defn moved [] :moved)\n",
        "(defn existing [] :existing)\n",
    );
    request.from_dialect = Dialect::Unknown;
    request.to_dialect = Dialect::Unknown;
    request.paths = vec![Path::from_indexes(vec![1])];

    let plan = plan_split_file(request).expect("Unknown dialect should retain generic parsing");

    assert!(plan.from_rewritten.contains("(defn keep [] :kept)"));
    assert!(plan.to_rewritten.contains("(defn moved [] :moved)"));
    SyntaxTree::parse_with_dialect(&plan.from_rewritten, Dialect::Unknown)
        .expect("Unknown source output should parse");
    SyntaxTree::parse_with_dialect(&plan.to_rewritten, Dialect::Unknown)
        .expect("Unknown destination output should parse");
}

#[test]
fn plan_split_file_does_not_inject_common_lisp_package_into_non_common_lisp_destination() {
    let mut request = split_request(
        "(in-package #:demo)\n\n(defun moved () :moved)\n",
        "(setq destination-ready t)\n",
    );
    request.from_dialect = Dialect::CommonLisp;
    request.to_dialect = Dialect::EmacsLisp;
    request.paths = vec![Path::from_indexes(vec![1])];

    let plan = plan_split_file(request).expect("Common Lisp to Emacs Lisp split should succeed");

    assert!(!plan.to_rewritten.contains("(in-package"));
    assert!(plan.to_rewritten.contains("(defun moved () :moved)"));
    SyntaxTree::parse_with_dialect(&plan.from_rewritten, Dialect::CommonLisp)
        .expect("Common Lisp source output should parse");
    SyntaxTree::parse_with_dialect(&plan.to_rewritten, Dialect::EmacsLisp)
        .expect("Emacs Lisp destination output should parse");
}
