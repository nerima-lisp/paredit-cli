use crate::definition::DefinitionCategory;

use super::*;

#[test]
fn private_and_public_definition_heads_are_both_definitions() {
    for head in [
        "ns",
        "def",
        "defonce",
        "declare",
        "defn",
        "defn-",
        "defmacro",
        "defmulti",
        "defmethod",
        "defprotocol",
        "definterface",
        "defrecord",
        "deftype",
        "defstruct",
        "deftest",
    ] {
        assert!(is_clojure_definition_head(head), "{head}");
    }
}

#[test]
fn definition_categories_distinguish_clojure_type_forms() {
    for (head, category) in [
        ("ns", DefinitionCategory::Package),
        ("def", DefinitionCategory::Variable),
        ("defonce", DefinitionCategory::Variable),
        ("declare", DefinitionCategory::Variable),
        ("defn", DefinitionCategory::Function),
        ("defn-", DefinitionCategory::Function),
        ("defmacro", DefinitionCategory::Macro),
        ("defmulti", DefinitionCategory::GenericFunction),
        ("defmethod", DefinitionCategory::Method),
        ("defprotocol", DefinitionCategory::Class),
        ("deftype", DefinitionCategory::Class),
        ("defrecord", DefinitionCategory::Struct),
        ("deftest", DefinitionCategory::Test),
    ] {
        assert_eq!(
            ClojureOperator::from_head(head).and_then(ClojureOperator::definition_category),
            Some(category),
            "{head}"
        );
    }
}

#[test]
fn only_defn_dash_is_a_head_level_private_definition() {
    assert!(
        ClojureOperator::from_head("defn-")
            .expect("defn- is a known operator")
            .is_private_definition()
    );
    for head in ["defn", "def", "defmacro"] {
        assert!(
            !ClojureOperator::from_head(head)
                .expect("known operator")
                .is_private_definition(),
            "{head}"
        );
    }
}

#[test]
fn every_clojure_binding_form_is_sequential() {
    for head in [
        "let",
        "loop",
        "binding",
        "with-open",
        "with-redefs",
        "with-local-vars",
        "when-let",
        "when-some",
        "when-first",
        "if-let",
        "if-some",
    ] {
        assert_eq!(
            ClojureOperator::from_head(head).and_then(ClojureOperator::binding_form),
            Some(ClojureBindingForm::SequentialPairs),
            "{head}"
        );
    }
}

#[test]
fn threading_macros_report_their_argument_position() {
    for (head, form, threads_last) in [
        ("->", ClojureThreadingForm::First, false),
        ("->>", ClojureThreadingForm::Last, true),
        ("as->", ClojureThreadingForm::Named, false),
        ("some->", ClojureThreadingForm::SomeFirst, false),
        ("some->>", ClojureThreadingForm::SomeLast, true),
        ("cond->", ClojureThreadingForm::CondFirst, false),
        ("cond->>", ClojureThreadingForm::CondLast, true),
    ] {
        let resolved = ClojureOperator::from_head(head).and_then(ClojureOperator::threading_form);
        assert_eq!(resolved, Some(form), "{head}");
        assert_eq!(form.threads_last(), threads_last, "{head}");
        assert_eq!(form.label(), head);
    }
}

#[test]
fn core_qualified_heads_resolve_but_foreign_qualifiers_do_not() {
    assert_eq!(
        ClojureOperator::from_head("clojure.core/defn"),
        Some(ClojureOperator::Defn)
    );
    assert_eq!(
        ClojureOperator::from_head("cljs.core/let"),
        Some(ClojureOperator::Let)
    );
    assert_eq!(ClojureOperator::from_head("my.ns/defn"), None);
    assert_eq!(ClojureOperator::from_head("str/join"), None);
    assert_eq!(ClojureOperator::from_head("clojure.core/"), None);
}

#[test]
fn operator_lookup_is_case_sensitive() {
    assert_eq!(
        ClojureOperator::from_head("defn"),
        Some(ClojureOperator::Defn)
    );
    assert_eq!(ClojureOperator::from_head("Defn"), None);
    assert_eq!(ClojureOperator::from_head("DEFN"), None);
}

#[test]
fn only_defmacro_defines_a_macro_expander() {
    assert!(
        ClojureOperator::from_head("defmacro")
            .expect("defmacro is a known operator")
            .is_macro_expander_definition()
    );
    for head in ["defn", "defn-", "def", "defmulti"] {
        assert!(
            !ClojureOperator::from_head(head)
                .expect("known operator")
                .is_macro_expander_definition(),
            "{head}"
        );
    }
}

#[test]
fn indent_styles_cover_the_forms_a_common_lisp_table_would_misformat() {
    for (head, style) in [
        ("ns", ClojureIndentStyle::Body(1)),
        ("defn", ClojureIndentStyle::Definition),
        ("defn-", ClojureIndentStyle::Definition),
        ("fn", ClojureIndentStyle::Function),
        ("let", ClojureIndentStyle::BindingVector),
        ("doseq", ClojureIndentStyle::BindingVector),
        ("cond", ClojureIndentStyle::PairClauses(0)),
        ("case", ClojureIndentStyle::PairClauses(1)),
        ("condp", ClojureIndentStyle::PairClauses(2)),
        ("->", ClojureIndentStyle::Threading(1)),
        ("as->", ClojureIndentStyle::Threading(2)),
        ("defrecord", ClojureIndentStyle::Body(2)),
        ("defmethod", ClojureIndentStyle::Body(3)),
        ("defprotocol", ClojureIndentStyle::Body(1)),
        ("do", ClojureIndentStyle::HeadBody),
        ("comment", ClojureIndentStyle::HeadBody),
        ("some-user-macro", ClojureIndentStyle::Call),
    ] {
        assert_eq!(clojure_indent_style_for_head(head), style, "{head}");
    }
}

#[test]
fn type_and_protocol_forms_declare_a_method_body() {
    for head in [
        "defrecord",
        "deftype",
        "defprotocol",
        "definterface",
        "reify",
        "extend-type",
        "extend-protocol",
        "proxy",
    ] {
        assert!(
            ClojureOperator::from_head(head)
                .expect("known operator")
                .has_method_body(),
            "{head}"
        );
    }
    assert!(
        !ClojureOperator::from_head("defn")
            .expect("defn is a known operator")
            .has_method_body()
    );
}

#[test]
fn normalization_leaves_unqualified_and_foreign_heads_untouched() {
    assert_eq!(normalize_clojure_operator_head("defn"), "defn");
    assert_eq!(normalize_clojure_operator_head("my.ns/defn"), "my.ns/defn");
    assert_eq!(normalize_clojure_operator_head("clojure.core/defn"), "defn");
}
