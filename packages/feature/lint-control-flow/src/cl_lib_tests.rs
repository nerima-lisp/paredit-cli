use crate::engine_pass_tests::fired;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use std::path::Path;

fn el(source: &str) -> Vec<&'static str> {
    fired(source, Dialect::EmacsLisp)
}

#[test]
fn all_six_cl_lib_rules_are_reachable_through_the_engine() {
    for (source, rule) in [
        (
            "(cl-block x (cl-block x (cl-return-from x 1)))",
            "block-name-shadows-outer-block",
        ),
        ("(go missing)", "go-to-undefined-tag"),
        (
            "(cl-tagbody unused (message \"ok\"))",
            "tagbody-unreachable-tag",
        ),
        (
            "(cl-multiple-value-bind (x y) (list 1 2) 3)",
            "multiple-value-bind-all-ignored",
        ),
        ("(cl-return-from missing 1)", "return-from-unmatched-block"),
        ("(cl-return 1)", "return-outside-implicit-nil-block"),
    ] {
        assert_eq!(el(source), vec![rule], "{source}");
    }
}

#[test]
fn all_six_standalone_reports_model_emacs_lisp() {
    macro_rules! check {
        ($module:ident, $build:ident, $source:literal) => {{
            let tree = SyntaxTree::parse_with_dialect($source, Dialect::EmacsLisp).expect("parse");
            let report =
                crate::$module::domain::$build(Path::new("fixture.el"), Dialect::EmacsLisp, &tree)
                    .expect("report");
            assert!(report.dialect_modelled);
            assert_eq!(report.findings.len(), 1, $source);
        }};
    }
    check!(
        block_name_shadows_outer_block,
        build_block_name_shadows_outer_block_report,
        "(cl-block x (cl-block x (cl-return-from x 1)))"
    );
    check!(
        go_to_undefined_tag,
        build_go_to_undefined_tag_report,
        "(go missing)"
    );
    check!(
        tagbody_unreachable_tag,
        build_tagbody_unreachable_tag_report,
        "(cl-tagbody unused)"
    );
    check!(
        multiple_value_bind_all_ignored,
        build_multiple_value_bind_all_ignored_report,
        "(cl-multiple-value-bind (x) 1 2)"
    );
    check!(
        return_from_unmatched_block,
        build_return_from_unmatched_block_report,
        "(cl-return-from missing)"
    );
    check!(
        return_outside_implicit_nil_block,
        build_return_outside_implicit_nil_block_report,
        "(cl-return)"
    );
}

#[test]
fn tagbody_uses_go_and_case_sensitive_tags() {
    assert!(el("(cl-tagbody Done (go Done))").is_empty());
    assert_eq!(
        el("(cl-tagbody Done (go done))"),
        vec!["go-to-undefined-tag", "tagbody-unreachable-tag"]
    );
    assert!(el("(cl-tagbody 007 (go 7))").is_empty());
}

#[test]
fn named_blocks_are_case_sensitive_and_not_package_unqualified() {
    assert!(el("(cl-block Done (cl-return-from Done))").is_empty());
    assert_eq!(
        el("(cl-block Done (cl-return-from done))"),
        vec!["return-from-unmatched-block"]
    );
    assert_eq!(
        el("(cl-block a:x (cl-return-from b:x))"),
        vec!["return-from-unmatched-block"]
    );
    assert!(el(r"(cl-block D\one (cl-return-from Done))").is_empty());
}

#[test]
fn only_cl_lib_function_definitions_establish_implicit_blocks() {
    for head in ["cl-defun", "cl-defmacro", "cl-defsubst", "cl-iter-defun"] {
        assert!(el(&format!("({head} Done () (cl-return-from Done))")).is_empty());
    }
    assert_eq!(
        el("(defun Done () (cl-return-from Done))"),
        vec!["return-from-unmatched-block"]
    );
    assert_eq!(
        el("(cl-defun Done () (cl-return-from done))"),
        vec!["return-from-unmatched-block"]
    );
}

#[test]
fn local_function_blocks_are_limited_to_each_function_body() {
    for head in ["cl-flet", "cl-labels"] {
        assert!(el(&format!("({head} ((Done () (cl-return-from Done))) 1)")).is_empty());
        assert_eq!(
            el(&format!("({head} ((Done () 1)) (cl-return-from Done))")),
            vec!["return-from-unmatched-block"]
        );
        assert_eq!(
            el(&format!(
                "({head} ((Done () 1) (Other () (cl-return-from Done))) 2)"
            )),
            vec!["return-from-unmatched-block"]
        );
        assert_eq!(
            el(&format!("({head} ((Done () (cl-return-from done))) 1)")),
            vec!["return-from-unmatched-block"]
        );
    }
}

#[test]
fn nil_and_named_loop_blocks_are_distinct() {
    assert!(el("(cl-loop do (cl-return 1))").is_empty());
    assert!(el("(cl-loop named Done do (cl-return-from Done))").is_empty());
    assert_eq!(
        el("(cl-loop named Done do (cl-return 1))"),
        vec!["return-outside-implicit-nil-block"]
    );
    assert!(el("(cl-dotimes (i 2) (cl-return i))").is_empty());
}

#[test]
fn only_prog_bodies_have_implicit_tags() {
    for source in [
        "(cl-do () (nil) Done (go Done))",
        "(cl-do* () (nil) Done (go Done))",
        "(cl-dolist (x nil) Done (go Done))",
        "(cl-dotimes (i 2) Done (go Done))",
        "(cl-do-symbols (x) Done (go Done))",
        "(cl-do-all-symbols (x) Done (go Done))",
    ] {
        assert_eq!(el(source), vec!["go-to-undefined-tag"], "{source}");
    }
    for head in ["cl-prog", "cl-prog*"] {
        assert!(el(&format!("({head} () Done (go Done))")).is_empty());
        assert_eq!(
            el(&format!("({head} ((x (go Done))) Done (go Done))")),
            vec!["go-to-undefined-tag"]
        );
    }
    assert!(el("(cl-tagbody Done (cl-dotimes (i 2) (go Done)))").is_empty());
}

#[test]
fn unknown_macros_and_quoted_exits_are_not_diagnosed() {
    for source in [
        "(with-retry (cl-return-from retry))",
        "(with-loop (cl-return 1))",
        "(with-tags (go Done))",
        "(cl-tagbody Done (custom-jump))",
        "(cl-multiple-value-bind (x) (list 1) (custom-body))",
        "'(cl-return 1)",
        "'(cl-return-from Missing)",
        "'(go Missing)",
        "'(cl-tagbody Unused)",
        "'(cl-block x (cl-block x (cl-return-from x)))",
    ] {
        assert!(el(source).is_empty(), "{source}");
    }
}

#[test]
fn unprefixed_cl_operators_and_uppercase_heads_are_not_el_operators() {
    for source in [
        "(return 1)",
        "(return-from missing)",
        "(tagbody unused)",
        "(block x (block x (return-from x)))",
        "(multiple-value-bind (x) 1 2)",
        "(CL-RETURN 1)",
    ] {
        assert!(el(source).is_empty(), "{source}");
    }
}

#[test]
fn multiple_value_names_preserve_case() {
    assert!(el("(cl-multiple-value-bind (X) (list 1) X)").is_empty());
    assert_eq!(
        el("(cl-multiple-value-bind (X) (list 1) x)"),
        vec!["multiple-value-bind-all-ignored"]
    );
}
