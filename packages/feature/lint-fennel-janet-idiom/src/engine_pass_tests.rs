//! Every rule driven through the *real* engine rather than by calling
//! `examine` or `check` directly.
//!
//! Calling `check` bypasses the head index and the dialect filter, which is
//! where the two mistakes this package is most exposed to would hide:
//!
//! - A `HeadFilter::Heads` entry that does not match the source spelling. For
//!   Fennel and Janet `head_key` returns the head *verbatim*
//!   (`head_index.rs:82-87`), so `NormalizedHead::new` and the source have to
//!   agree byte for byte — there is no case folding to rescue a mismatch, and a
//!   rule with a wrong head simply never runs.
//! - A forgotten `dialect_scope`. The trait's default is
//!   `COMMON_LISP_ONLY` (`rule.rs:30-31`), so a rule that omits the override
//!   silently never fires on its own dialect while every unit test on
//!   `examine`, which takes the dialect as an argument, still passes.

use std::path::Path;

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::RuleCatalog;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::ENTRIES;

fn path_for(dialect: Dialect) -> &'static Path {
    match dialect {
        Dialect::Janet => Path::new("t.janet"),
        Dialect::Fennel => Path::new("t.fnl"),
        _ => Path::new("t.lisp"),
    }
}

/// The rule names that fire on `source`, sorted so the assertions do not
/// depend on registration order.
pub(crate) fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    let mut names: Vec<&'static str> = collect_lint_outcomes(
        catalog,
        &index,
        path_for(dialect),
        dialect,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("lint pass")
    .into_iter()
    .map(|outcome| outcome.into_parts().0.rule)
    .collect();
    names.sort_unstable();
    names
}

/// The findings' messages, for the assertions that care what was said.
fn messages(source: &str, dialect: Dialect) -> Vec<String> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    collect_lint_outcomes(
        catalog,
        &index,
        path_for(dialect),
        dialect,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("lint pass")
    .into_iter()
    .map(|outcome| outcome.into_parts().0.message)
    .collect()
}

// -- each rule reaches the engine ----------------------------------------

#[test]
fn every_rule_fires_through_the_real_dispatch() {
    assert_eq!(
        fired("(var total 0)\n(print total)", Dialect::Fennel),
        vec!["var-never-set"]
    );
    assert_eq!(
        fired("(var total 0)\n(print total)", Dialect::Janet),
        vec!["var-never-set"]
    );
    assert_eq!(
        fired("(require-macros :my.macros)", Dialect::Fennel),
        vec!["fennel-deprecated-form"]
    );
    assert_eq!(
        fired("(each [k v {:a 1}] (print k v))", Dialect::Fennel),
        vec!["fennel-each-over-non-iterator"]
    );
    assert_eq!(
        fired("(loop [x :in xs])", Dialect::Janet),
        vec!["janet-empty-loop-body"]
    );
    assert_eq!(
        fired("(put {:a 1} :b 2)", Dialect::Janet),
        vec!["janet-mutating-immutable-literal"]
    );
}

/// `var-never-set` declares `var-` as well as `var`. A head registered in the
/// index but never produced by any test would be indistinguishable from a
/// typo, so it gets its own arrival.
#[test]
fn the_private_janet_var_spelling_reaches_the_index() {
    assert_eq!(
        fired("(var- total :private 0)\n(print total)", Dialect::Janet),
        vec!["var-never-set"]
    );
}

/// Every head in `janet-mutating-immutable-literal`'s seventeen-entry filter,
/// exercised through the index. A misspelled entry there is invisible to the
/// domain tests, which never consult the index at all.
#[test]
fn every_declared_mutator_head_is_reachable_through_the_index() {
    for (head, arguments) in [
        ("put", ":b 2"),
        ("put-in", "[:b] 2"),
        ("update", ":b inc"),
        ("update-in", "[:b] inc"),
        ("array/push", "4"),
        ("array/pop", ""),
        ("array/concat", "@[4]"),
        ("array/insert", "0 4"),
        ("array/remove", "0"),
        ("array/fill", "0"),
        ("array/clear", ""),
        ("array/ensure", "8 2"),
        ("array/trim", ""),
        ("buffer/push", "65"),
        ("buffer/push-string", "\"x\""),
        ("buffer/clear", ""),
        ("buffer/format", "\"%d\" 1"),
    ] {
        let source = format!("({head} [1 2] {arguments})");
        assert_eq!(
            fired(&source, Dialect::Janet),
            vec!["janet-mutating-immutable-literal"],
            "{head} did not reach the rule through the head index"
        );
    }
}

// -- the dialect scope, which only the engine applies ---------------------

/// `var` is a head in Common Lisp (`(defvar …)` is not it, but `var` is a
/// perfectly ordinary function name) and in Clojure it is `#'var`. Neither is
/// in scope, and the dispatcher must drop the rule before the walk rather than
/// report and filter afterwards.
#[test]
fn no_rule_fires_on_a_dialect_outside_its_scope() {
    for dialect in [
        Dialect::CommonLisp,
        Dialect::Clojure,
        Dialect::Scheme,
        Dialect::EmacsLisp,
        Dialect::Hy,
    ] {
        assert_eq!(
            fired("(var total 0)\n(print total)", dialect),
            Vec::<&str>::new(),
            "var-never-set fired on {dialect:?}"
        );
    }
}

#[test]
fn the_fennel_rules_never_fire_on_janet() {
    // `(global x 1)` and `(each [k v {:a 1}] …)` are both readable Janet — the
    // second is a call to a function named `each` — and neither means what the
    // Fennel rules describe.
    assert_eq!(
        fired(
            "(global x 1)\n(each [k v {:a 1}] (print k))",
            Dialect::Janet
        ),
        Vec::<&str>::new()
    );
}

#[test]
fn the_janet_rules_never_fire_on_fennel() {
    // In Fennel `[1 2]` is a mutable Lua table and `(loop …)` is not a special
    // at all, so both of these are ordinary function calls.
    assert_eq!(
        fired("(put [1 2] 0 3)\n(loop [x :in xs])", Dialect::Fennel),
        Vec::<&str>::new()
    );
}

// -- the quote guard, which only the engine can exercise ------------------

/// The dispatcher descends into quoted data unconditionally
/// (`dispatch.rs:291`), so every one of these reaches its rule's `check` and is
/// rejected there. Without the guard each line fires.
#[test]
fn a_form_inside_a_macro_template_is_not_reported() {
    assert_eq!(
        fired("(macro m [] `(var total 0))", Dialect::Fennel),
        Vec::<&str>::new()
    );
    assert_eq!(
        fired("(macro m [] '(global x 1))", Dialect::Fennel),
        Vec::<&str>::new()
    );
    assert_eq!(
        fired(
            "(macro m [] `(each [k v {:a 1}] (print k)))",
            Dialect::Fennel
        ),
        Vec::<&str>::new()
    );
    assert_eq!(
        fired("(defmacro m [] ~(loop [x :in xs]))", Dialect::Janet),
        Vec::<&str>::new()
    );
    assert_eq!(
        fired("(defmacro m [] ~(put {:a 1} :b 2))", Dialect::Janet),
        Vec::<&str>::new()
    );
}

/// The other half of the guard: an unquoted escape inside a quasiquote is
/// code again, and a rule that stopped at the quasiquote would miss it. Pairs
/// with the test above so neither passes for the wrong reason.
#[test]
fn an_unquoted_escape_inside_a_template_is_still_code() {
    assert_eq!(
        fired("(defmacro m [] ~(do ,(put {:a 1} :b 2)))", Dialect::Janet),
        vec!["janet-mutating-immutable-literal"]
    );
    assert_eq!(
        fired(
            "(macro m [] `(do ,(each [k v {:a 1}] (print k))))",
            Dialect::Fennel
        ),
        vec!["fennel-each-over-non-iterator"]
    );
}

/// A hard quote never clears, so a comma inside one is a comma character.
#[test]
fn a_hard_quote_swallows_its_own_unquote() {
    assert_eq!(
        fired(
            "(macro m [] '(do ,(each [k v {:a 1}] (print k))))",
            Dialect::Fennel
        ),
        Vec::<&str>::new()
    );
}

// -- messages -------------------------------------------------------------

#[test]
fn each_message_names_the_replacement_the_reader_needs() {
    let fennel = messages("(var total 0)\n(print total)", Dialect::Fennel);
    assert!(fennel[0].contains("local"), "{fennel:?}");
    let janet = messages("(var total 0)\n(print total)", Dialect::Janet);
    assert!(janet[0].contains("def"), "{janet:?}");
    let deprecated = messages("(require-macros :m)", Dialect::Fennel);
    assert!(deprecated[0].contains("import-macros"), "{deprecated:?}");
}

// -- more than one rule at a time -----------------------------------------

#[test]
fn several_rules_share_one_pass_over_one_file() {
    let mut names = fired(
        "(var total 0)\n(global cache {})\n(each [k v {:a 1}] (print k))",
        Dialect::Fennel,
    );
    names.dedup();
    assert_eq!(
        names,
        vec![
            "fennel-deprecated-form",
            "fennel-each-over-non-iterator",
            "var-never-set"
        ]
    );
}

/// Two `var`s in one file both arrive: the head index dispatches per node, and
/// a rule that reported only the first would still pass every single-finding
/// test above.
#[test]
fn every_matching_node_is_dispatched_not_just_the_first() {
    assert_eq!(
        fired("(var a 0)\n(var b 0)\n(print a b)", Dialect::Fennel),
        vec!["var-never-set", "var-never-set"]
    );
}
