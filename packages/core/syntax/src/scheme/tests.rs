use crate::dialect::Dialect;
use crate::sexpr::{ExpressionView, SyntaxTree};

use super::*;

fn parsed_form(source: &str) -> ExpressionView {
    parsed_form_in(source, Dialect::Scheme)
}

fn parsed_form_in(source: &str, dialect: Dialect) -> ExpressionView {
    SyntaxTree::parse_with_dialect(source, dialect)
        .expect("fixture parses")
        .root_view()
        .children
        .first()
        .cloned()
        .expect("fixture has one form")
}

fn formal_names(source: &str) -> Vec<String> {
    scheme_formals(&parsed_form(source))
        .into_iter()
        .map(|formal| formal.name)
        .collect()
}

fn formal_kinds(source: &str) -> Vec<SchemeFormalKind> {
    scheme_formals(&parsed_form(source))
        .into_iter()
        .map(|formal| formal.kind)
        .collect()
}

#[test]
fn operator_table_resolves_every_binding_form_scheme_actually_has() {
    let cases = [
        ("let", SchemeOperator::Let),
        ("let*", SchemeOperator::LetStar),
        ("letrec", SchemeOperator::Letrec),
        ("letrec*", SchemeOperator::LetrecStar),
        ("let-values", SchemeOperator::LetValues),
        ("let*-values", SchemeOperator::LetStarValues),
        ("let-syntax", SchemeOperator::LetSyntax),
        ("letrec-syntax", SchemeOperator::LetrecSyntax),
        ("do", SchemeOperator::Do),
        ("lambda", SchemeOperator::Lambda),
        ("case-lambda", SchemeOperator::CaseLambda),
        ("guard", SchemeOperator::Guard),
        ("parameterize", SchemeOperator::Parameterize),
        ("fluid-let", SchemeOperator::FluidLet),
    ];

    for (head, expected) in cases {
        assert_eq!(SchemeOperator::from_head(head), Some(expected), "{head}");
        assert!(expected.is_binder(), "{head} must open a scope");
    }
}

#[test]
fn operator_lookup_is_case_sensitive_because_scheme_is() {
    assert_eq!(SchemeOperator::from_head("let"), Some(SchemeOperator::Let));
    assert_eq!(SchemeOperator::from_head("LET"), None);
    assert_eq!(SchemeOperator::from_head("Define"), None);
}

#[test]
fn lambda_synonym_resolves_to_the_same_operator() {
    assert_eq!(SchemeOperator::from_head("λ"), Some(SchemeOperator::Lambda));
}

#[test]
fn letrec_and_letrec_star_share_recursive_visibility() {
    for head in ["letrec", "letrec*"] {
        let operator = SchemeOperator::from_head(head).expect("known head");
        assert_eq!(
            operator.binding_form(),
            Some(SchemeBindingForm::Let {
                kind: SchemeLetKind::Recursive,
                namespace: SchemeBindingNamespace::Value,
            }),
            "{head}"
        );
    }
}

#[test]
fn parameterize_and_fluid_let_are_marked_as_binding_nothing() {
    for head in ["parameterize", "fluid-let"] {
        let operator = SchemeOperator::from_head(head).expect("known head");
        assert_eq!(
            operator.binding_form(),
            Some(SchemeBindingForm::DynamicBinding),
            "{head}"
        );
    }
}

#[test]
fn an_ordinary_procedure_head_has_no_registered_semantics() {
    for head in ["car", "display", "vector-ref", "my-helper"] {
        assert!(!scheme_head_has_registered_semantics(head), "{head}");
    }
}

#[test]
fn fixed_formals_bind_every_parameter_in_order() {
    assert_eq!(formal_names("(a b c)"), vec!["a", "b", "c"]);
    assert_eq!(formal_kinds("(a b c)"), vec![SchemeFormalKind::Required; 3]);
}

#[test]
fn improper_formals_bind_the_rest_parameter_and_not_the_dot() {
    assert_eq!(formal_names("(a b . rest)"), vec!["a", "b", "rest"]);
    assert_eq!(
        formal_kinds("(a b . rest)"),
        vec![
            SchemeFormalKind::Required,
            SchemeFormalKind::Required,
            SchemeFormalKind::Rest,
        ]
    );
}

#[test]
fn a_bare_symbol_formals_list_binds_one_rest_parameter() {
    assert_eq!(formal_names("args"), vec!["args"]);
    assert_eq!(formal_kinds("args"), vec![SchemeFormalKind::Rest]);
}

#[test]
fn bracketed_optional_binds_only_its_name_not_its_default() {
    assert_eq!(formal_names("(a [b 42])"), vec!["a", "b"]);
    assert_eq!(
        formal_kinds("(a [b 42])"),
        vec![SchemeFormalKind::Required, SchemeFormalKind::Optional]
    );
}

#[test]
fn racket_keyword_parameter_binds_the_name_after_the_keyword_token() {
    // `#:mode` is Racket-only syntax, so the fixture is parsed as Racket. The
    // keyword token names the call site; the binding is the token after it.
    let formals = parsed_form_in("(a #:mode mode)", Dialect::Racket);
    let parsed = scheme_formals(&formals);

    assert_eq!(
        parsed
            .iter()
            .map(|formal| formal.name.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "mode"]
    );
    assert_eq!(
        parsed.iter().map(|formal| formal.kind).collect::<Vec<_>>(),
        vec![SchemeFormalKind::Required, SchemeFormalKind::Keyword]
    );
}

#[test]
fn a_literal_in_a_parameter_position_is_not_read_as_a_name() {
    assert_eq!(formal_names("(a \"b\" 42 #t)"), vec!["a"]);
}

#[test]
fn formal_defaults_are_reported_separately_from_the_names() {
    let formals = parsed_form("(a [b (compute)])");
    let defaults = scheme_formal_defaults(&formals);

    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].children.len(), 1);
}

#[test]
fn empty_formals_bind_nothing_and_are_still_readable() {
    let formals = parsed_form("()");
    assert!(scheme_formals(&formals).is_empty());
    assert!(scheme_formals_are_readable(&formals));
}

#[test]
fn define_target_discriminates_variable_procedure_and_curried_procedure() {
    let variable = parsed_form("answer");
    let procedure = parsed_form("(answer x)");
    let curried = parsed_form("((adder n) x)");

    assert_eq!(
        scheme_define_target(&variable).map(|target| target.curry_depth()),
        Some(0)
    );
    assert_eq!(
        scheme_define_target(&procedure).map(|target| target.curry_depth()),
        Some(1)
    );
    assert_eq!(
        scheme_define_target(&curried).map(|target| target.curry_depth()),
        Some(2)
    );
}

#[test]
fn curried_define_names_the_symbol_at_the_bottom_of_the_leftmost_spine() {
    let target = parsed_form("((adder n) x)");
    let resolved = scheme_define_target(&target).expect("curried target");

    assert_eq!(resolved.name().text.as_deref(), Some("adder"));
}

#[test]
fn curried_define_reports_parameters_in_application_order() {
    let target = parsed_form("((adder n) x)");
    let resolved = scheme_define_target(&target).expect("curried target");
    let parameters: Vec<_> = resolved
        .parameters()
        .into_iter()
        .filter_map(|parameter| parameter.text.as_deref())
        .collect();

    // `(define ((adder n) x) ...)` is `(define (adder n) (lambda (x) ...))`,
    // so `n` is consumed before `x` even though it is nested deeper.
    assert_eq!(parameters, vec!["n", "x"]);
}

#[test]
fn define_target_skips_the_name_when_reporting_parameters() {
    let target = parsed_form("(f a . rest)");
    let resolved = scheme_define_target(&target).expect("procedure target");
    let names: Vec<_> = scheme_formals_in(
        &resolved
            .parameters()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>(),
    )
    .into_iter()
    .map(|formal| formal.name)
    .collect();

    assert_eq!(names, vec!["a", "rest"]);
}

#[test]
fn define_target_rejects_shapes_it_cannot_read() {
    for source in ["()", "\"string\"", "(())"] {
        let target = parsed_form(source);
        assert_eq!(scheme_define_target(&target), None, "{source}");
    }
}
