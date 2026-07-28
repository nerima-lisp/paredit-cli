use super::*;

/// Returns every name a Clojure parameter vector binds, in traversal order.
fn parameter_names(source: &str) -> Vec<String> {
    let view = selected_form_with_dialect(source, Dialect::Clojure);
    lambda_list_bound_names(&view)
        .into_iter()
        .map(|bound| bound.name)
        .collect()
}

#[test]
fn keys_strs_and_syms_shorthands_all_bind_their_vector() {
    // These three differ only in the key type they look up — keyword, string,
    // and symbol — which does not change what they bind.
    assert_eq!(parameter_names("[{:keys [a b]}]"), vec!["a", "b"]);
    assert_eq!(parameter_names("[{:strs [a b]}]"), vec!["a", "b"]);
    assert_eq!(parameter_names("[{:syms [a b]}]"), vec!["a", "b"]);
}

#[test]
fn namespaced_keys_shorthand_binds_the_local_names() {
    assert_eq!(
        parameter_names("[{:person/keys [name age]}]"),
        vec!["name", "age"]
    );
}

#[test]
fn or_defaults_bind_nothing_of_their_own() {
    // `:or` supplies defaults for names `:keys` already bound. Recursing into
    // its map would register the literal `1` as a binding.
    assert_eq!(parameter_names("[{:keys [k] :or {k 1}}]"), vec!["k"]);
    assert_eq!(
        parameter_names(r#"[{:keys [a b] :or {a 1 b "text"}}]"#),
        vec!["a", "b"]
    );
}

#[test]
fn as_binds_the_whole_value_alongside_the_destructured_names() {
    assert_eq!(
        parameter_names("[{:keys [a] :as whole}]"),
        vec!["a", "whole"]
    );
}

#[test]
fn nested_vector_destructuring_binds_every_level() {
    assert_eq!(parameter_names("[[a [b c]] d]"), vec!["a", "b", "c", "d"]);
}

#[test]
fn rest_parameters_and_underscores_are_handled() {
    assert_eq!(parameter_names("[a & more]"), vec!["a", "more"]);
    assert_eq!(parameter_names("[_ b]"), vec!["b"]);
}

#[test]
fn explicit_key_destructuring_binds_names_not_keys() {
    assert_eq!(parameter_names("[{a :alpha b :beta}]"), vec!["a", "b"]);
}

#[test]
fn a_fully_loaded_pattern_binds_exactly_its_names() {
    assert_eq!(
        parameter_names(
            r#"[{:keys [first-name last-name] :or {last-name "Doe"} :as person} & opts]"#
        ),
        vec!["first-name", "last-name", "person", "opts"]
    );
}
