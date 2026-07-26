use super::*;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Path, SymbolName, SyntaxTree};

mod basic;
mod property;
mod rejection;
mod shadowing;
mod special;

fn request_with_dialect<'a>(
    input: &'a str,
    dialect: Dialect,
    path: &str,
    all_occurrences: bool,
) -> IntroduceLetRequest<'a> {
    let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
    let path = path.parse::<Path>().expect("path");
    let selection = tree.select_path(&path).expect("select");
    IntroduceLetRequest {
        input,
        dialect,
        path: Some(path),
        target: selection.view(),
        enclosing_span: selection.enclosing_list_span().expect("enclosing"),
        name: SymbolName::new("product").expect("symbol"),
        all_occurrences,
    }
}

fn request<'a>(input: &'a str, path: &str, all_occurrences: bool) -> IntroduceLetRequest<'a> {
    request_with_dialect(input, Dialect::CommonLisp, path, all_occurrences)
}

fn assert_plan(
    input: &str,
    path: &str,
    all_occurrences: bool,
    expected_occurrences: usize,
    expected_skipped: usize,
    expected_rewritten: &str,
) {
    let plan = plan_introduce_let(request(input, path, all_occurrences)).expect("plan");

    assert_eq!(plan.occurrence_spans.len(), expected_occurrences);
    assert_eq!(
        plan.skipped_shadowed_occurrence_spans.len(),
        expected_skipped
    );
    assert_eq!(plan.rewritten, expected_rewritten);
}

fn assert_plan_with_dialect(
    input: &str,
    dialect: Dialect,
    path: &str,
    all_occurrences: bool,
    expected_occurrences: usize,
    expected_skipped: usize,
    expected_rewritten: &str,
) {
    let plan = plan_introduce_let(request_with_dialect(input, dialect, path, all_occurrences))
        .expect("plan");

    assert_eq!(plan.occurrence_spans.len(), expected_occurrences);
    assert_eq!(
        plan.skipped_shadowed_occurrence_spans.len(),
        expected_skipped
    );
    assert_eq!(plan.rewritten, expected_rewritten);
    SyntaxTree::parse_with_dialect(&plan.rewritten, dialect)
        .expect("rewritten output remains parseable");
}

fn assert_shadowed_error(input: &str, path: &str) {
    let error = plan_introduce_let(request(input, path, false)).expect_err("shadowed");

    assert!(
        error
            .to_string()
            .contains("inside an existing binding for 'product'")
    );
}
