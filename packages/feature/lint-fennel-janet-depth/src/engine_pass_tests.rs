//! Every rule driven through the *real* engine rather than by calling
//! `examine` or `check` directly.
//!
//! Calling `check` bypasses the head index and the dialect filter, which is
//! where the two mistakes this package is most exposed to would hide:
//!
//! - A `HeadFilter::Heads` entry that does not match the source spelling. For
//!   Fennel and Janet `head_key` returns the head *verbatim*
//!   (`head_index.rs`), so `NormalizedHead::new` and the source have to agree
//!   byte for byte — there is no case folding to rescue a mismatch, `λ` is not
//!   `lambda`, and a rule with a wrong head simply never runs.
//! - A forgotten `dialect_scope`. The trait's default is `COMMON_LISP_ONLY`,
//!   so a rule that omits the override silently never fires on its own dialect
//!   while every unit test on `examine`, which takes the dialect as an
//!   argument, still passes.
//!
//! A sibling package found that deleting a head from a rule's `Heads` left its
//! whole suite green, because nothing in that suite went through the index.
//! [`every_declared_head_arrives_through_the_index`] is the answer to that: it
//! walks the rules' own head constants and asserts each one reaches its rule.

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

fn outcomes(source: &str, dialect: Dialect) -> Vec<(&'static str, String)> {
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
    .map(|outcome| {
        let finding = outcome.into_parts().0;
        (finding.rule, finding.message)
    })
    .collect()
}

/// The rule names that fire on `source`, sorted so the assertions do not
/// depend on registration order.
pub(crate) fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = outcomes(source, dialect)
        .into_iter()
        .map(|(rule, _)| rule)
        .collect();
    names.sort_unstable();
    names
}

fn messages(source: &str, dialect: Dialect) -> Vec<String> {
    outcomes(source, dialect)
        .into_iter()
        .map(|(_, message)| message)
        .collect()
}

// -- each rule reaches the engine ----------------------------------------

#[test]
fn every_rule_fires_through_the_real_dispatch() {
    assert_eq!(
        fired("(+ 1 (table.unpack [2 3 4]))", Dialect::Fennel),
        vec!["fennel-bad-unpack"]
    );
    assert_eq!(
        fired("(and a (and b c))", Dialect::Fennel),
        vec!["fennel-nested-associative-operator"]
    );
    assert_eq!(
        fired("(fn [] (do (f) (g)))", Dialect::Fennel),
        vec!["fennel-redundant-do"]
    );
    assert_eq!(
        fired("(if true :a :b)", Dialect::Janet),
        vec!["janet-dead-branch-on-constant-condition"]
    );
    assert_eq!(
        fired("(match v x :first 99 :second)", Dialect::Janet),
        vec!["janet-unreachable-match-clause"]
    );
}

/// Every head every rule declares, exercised through the index.
///
/// This is the test that a deleted or misspelled `NormalizedHead` cannot
/// survive. It reads the domain modules' own head constants, so adding a head
/// to a rule without a source spelling that reaches it fails here rather than
/// passing silently.
#[test]
fn every_declared_head_arrives_through_the_index() {
    use crate::{
        fennel_bad_unpack, fennel_nested_associative_operator, fennel_redundant_do,
        janet_dead_branch_on_constant_condition, janet_unreachable_match_clause,
    };

    for head in fennel_bad_unpack::domain::HEADS {
        // Two arguments, so the five pass-through unary operators are in scope
        // too; the comparison operators reject one argument outright.
        let source = format!("({head} y (table.unpack xs))");
        assert_eq!(
            fired(&source, Dialect::Fennel),
            vec!["fennel-bad-unpack"],
            "bad-unpack head {head:?} did not reach the rule through the index"
        );
    }

    for head in fennel_nested_associative_operator::domain::HEADS {
        let source = format!("({head} a ({head} b c))");
        assert_eq!(
            fired(&source, Dialect::Fennel),
            vec!["fennel-nested-associative-operator"],
            "nested-operator head {head:?} did not reach the rule through the index"
        );
    }

    for head in fennel_redundant_do::domain::HEADS {
        // `do` and `eval-compiler` take no leading element; the rest do.
        let leading = if head == "do" || head == "eval-compiler" {
            ""
        } else {
            "[] "
        };
        let source = format!("({head} {leading}(do (f) (g)))");
        let mut names = fired(&source, Dialect::Fennel);
        names.dedup();
        assert_eq!(
            names,
            vec!["fennel-redundant-do"],
            "redundant-do head {head:?} did not reach the rule through the index"
        );
    }

    for head in janet_dead_branch_on_constant_condition::domain::HEADS {
        // `when`/`unless` need a falsy/truthy test respectively to have a dead
        // body; `if`/`if-not` report a dead branch either way.
        let source = match head {
            "when" => "(when false (f))".to_owned(),
            "unless" => "(unless true (f))".to_owned(),
            other => format!("({other} true :a :b)"),
        };
        assert_eq!(
            fired(&source, Dialect::Janet),
            vec!["janet-dead-branch-on-constant-condition"],
            "dead-branch head {head:?} did not reach the rule through the index"
        );
    }

    for head in janet_unreachable_match_clause::domain::HEADS {
        let source = format!("({head} v x :first 99 :second)");
        assert_eq!(
            fired(&source, Dialect::Janet),
            vec!["janet-unreachable-match-clause"],
            "match head {head:?} did not reach the rule through the index"
        );
    }
}

/// `λ` is a separate index key from `lambda`, because `head_key` does not fold
/// anything outside Common Lisp. A rule declaring only one of them covers only
/// one of them, and no domain test would notice.
#[test]
fn the_unicode_lambda_reaches_the_index_on_its_own() {
    assert_eq!(
        fired("(λ [] (do (f) (g)))", Dialect::Fennel),
        vec!["fennel-redundant-do"]
    );
}

// -- the dialect scope, which only the engine applies ---------------------

/// Every head this package registers is also an ordinary head in some other
/// Lisp: `if`, `when`, `let`, `fn`, `and`, `or`, `match`, `+`. The dispatcher
/// must drop each rule before the walk on a dialect outside its scope, not
/// report and filter afterwards.
#[test]
fn no_rule_fires_on_a_dialect_outside_its_scope() {
    // Bracket-free on purpose: `[` is not a delimiter in Common Lisp, so a
    // source containing one fails to *parse* there and the test would be
    // asserting nothing about scope. Every shape below is readable in all five
    // dialects, which is exactly what makes the assertion meaningful.
    let sources = [
        "(+ 1 (table.unpack xs))",
        "(and a (and b c))",
        "(fn () (do (f) (g)))",
        "(if true :a :b)",
        "(when false (f))",
        "(match v x :first 99 :second)",
        "(let () (do (f) (g)))",
    ];
    for dialect in [
        Dialect::CommonLisp,
        Dialect::Clojure,
        Dialect::Scheme,
        Dialect::EmacsLisp,
        Dialect::Hy,
    ] {
        for source in sources {
            assert_eq!(
                fired(source, dialect),
                Vec::<&str>::new(),
                "{dialect:?} fired on {source:?}"
            );
        }
    }
}

#[test]
fn the_fennel_rules_never_fire_on_janet() {
    // `(fn [] (do …))` is readable Janet — `fn` there takes an optional name
    // and a parameter tuple — and `(and a (and b c))` is an ordinary variadic
    // call. Neither means what the Fennel rules describe.
    assert_eq!(
        fired(
            "(fn [] (do (f) (g)))\n(and a (and b c))\n(+ 1 (table.unpack xs))",
            Dialect::Janet
        ),
        Vec::<&str>::new()
    );
}

#[test]
fn the_janet_rules_never_fire_on_fennel() {
    // Fennel has `if` and `match` too, with different grammars, and
    // `(if true :a :b)` there is ordinary code this package does not model.
    assert_eq!(
        fired(
            "(if true :a :b)\n(match v x :first 99 :second)",
            Dialect::Fennel
        ),
        Vec::<&str>::new()
    );
}

// -- the quote guard, which only the engine can exercise ------------------

/// The dispatcher descends into quoted data unconditionally, so every one of
/// these reaches its rule's `check` and is rejected there. Without the guard
/// each line fires.
#[test]
fn a_form_inside_a_macro_template_is_not_reported() {
    assert_eq!(
        fired("(macro m [] `(fn [] (do (f) (g))))", Dialect::Fennel),
        Vec::<&str>::new()
    );
    assert_eq!(
        fired("(macro m [] '(and a (and b c)))", Dialect::Fennel),
        Vec::<&str>::new()
    );
    assert_eq!(
        fired("(defmacro m [] ~(if true :a :b))", Dialect::Janet),
        Vec::<&str>::new()
    );
    assert_eq!(
        fired(
            "(defmacro m [] ~(match v x :first 99 :second))",
            Dialect::Janet
        ),
        Vec::<&str>::new()
    );
}

/// The measured false positive that the *form* span alone catches, from
/// `fennel-lang/fennel`'s own `src/fennel/match.fnl:125`.
///
/// The reported node is `,(unpack guards)`, whose `,` escapes the quasiquote,
/// so a guard asking only about the reported node calls it live code. The
/// `(and …)` around it is still a template and will never truncate anything.
#[test]
fn an_unquoted_argument_inside_a_quasiquoted_operator_is_not_reported() {
    assert_eq!(
        fired(
            "(macro m [c guards] `(and ,c ,(unpack guards)))",
            Dialect::Fennel
        ),
        Vec::<&str>::new()
    );
}

/// The measured false positive that the *reported* span alone catches, from
/// `fennel-lang/fennel`'s `test/loops.fnl:60` and nine more like it.
///
/// The dispatched `(macro …)` form is ordinary code; the `(do …)` is a list
/// the macro constructs, and its `do` is what lets the expansion return two
/// forms at once. A guard asking only about the dispatched form reports it.
#[test]
fn a_quasiquoted_do_template_is_not_reported() {
    assert_eq!(
        fired("(macro m [expr] `(do ,expr ,expr))", Dialect::Fennel),
        Vec::<&str>::new()
    );
    assert_eq!(
        fired("(macro m [body] `(do ,(unpack body)))", Dialect::Fennel),
        Vec::<&str>::new()
    );
}

/// Janet's PEG grammars reuse `if` and `if-not` as combinators and are written
/// as quoted or quasiquoted structs. This shape is `janet-lang/spork`'s
/// `path.janet:142`, and it accounts for 36 of the 40 raw hits the third-party
/// sweep produced before the guard.
#[test]
fn a_peg_combinator_named_if_not_is_not_a_conditional() {
    assert_eq!(
        fired(
            "(def p (peg/compile ~{:main (some (if-not `\\` 1))}))",
            Dialect::Janet
        ),
        Vec::<&str>::new()
    );
    assert_eq!(
        fired(
            "(def p (peg/compile '{:main (any (if-not \"%}\" 1))}))",
            Dialect::Janet
        ),
        Vec::<&str>::new()
    );
}

/// The other half of the guard: an unquoted escape inside a quasiquote is code
/// again, so neither test above passes for the wrong reason.
#[test]
fn an_unquoted_escape_inside_a_template_is_still_code() {
    assert_eq!(
        fired("(defmacro m [] ~(do ,(if true :a :b)))", Dialect::Janet),
        vec!["janet-dead-branch-on-constant-condition"]
    );
    assert_eq!(
        fired("(macro m [] `(do ,(fn [] (do (f) (g)))))", Dialect::Fennel),
        vec!["fennel-redundant-do"]
    );
}

/// A hard quote never clears, so a comma inside one is a comma character.
#[test]
fn a_hard_quote_swallows_its_own_unquote() {
    assert_eq!(
        fired("(defmacro m [] '(do ,(if true :a :b)))", Dialect::Janet),
        Vec::<&str>::new()
    );
}

// -- messages -------------------------------------------------------------

#[test]
fn each_message_names_what_the_reader_needs_to_know() {
    let unpack = messages("(+ 1 (table.unpack xs))", Dialect::Fennel);
    assert!(unpack[0].contains("not variadic"), "{unpack:?}");
    assert!(unpack[0].contains("table.unpack"), "{unpack:?}");

    let concat = messages("(.. \"a\" (table.unpack xs))", Dialect::Fennel);
    assert!(concat[0].contains("table.concat"), "{concat:?}");

    let dead = messages("(if true :a :b)", Dialect::Janet);
    assert!(dead[0].contains("else"), "{dead:?}");

    let clause = messages("(match v x :a 1 :b 2 :c)", Dialect::Janet);
    assert!(clause[0].contains("matches anything"), "{clause:?}");
    assert!(clause[0].contains("after it"), "{clause:?}");

    let single = messages("(match v x :a 1 :b)", Dialect::Janet);
    assert!(
        !single[0].contains("after it"),
        "a lone shadowed clause must not claim company: {single:?}"
    );
}

// -- more than one rule at a time -----------------------------------------

#[test]
fn several_rules_share_one_pass_over_one_file() {
    let mut names = fired(
        "(fn [] (do (f) (g)))\n(and a (and b c))\n(+ 1 (table.unpack xs))",
        Dialect::Fennel,
    );
    names.dedup();
    assert_eq!(
        names,
        vec![
            "fennel-bad-unpack",
            "fennel-nested-associative-operator",
            "fennel-redundant-do"
        ]
    );
}

/// Two matching forms in one file both arrive: the head index dispatches per
/// node, and a rule that reported only the first would still pass every
/// single-finding test above.
#[test]
fn every_matching_node_is_dispatched_not_just_the_first() {
    assert_eq!(
        fired("(if true :a :b)\n(if false :c :d)", Dialect::Janet),
        vec![
            "janet-dead-branch-on-constant-condition",
            "janet-dead-branch-on-constant-condition"
        ]
    );
}
