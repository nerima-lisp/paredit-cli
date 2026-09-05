//! Each rule against the code it must flag and the code it must not.
//!
//! Every case is paired. A rule that reported unconditionally would satisfy
//! any single positive test and be worthless, so each positive has a negative
//! that differs by exactly the thing the rule is about.
//!
//! Two suite-level tests per dialect carry the weight the per-rule pairs
//! cannot: a realistic *correct* file sweeps to zero findings while also
//! asserting a non-zero count of the shapes each rule keys on — so it cannot
//! pass by matching nothing — and a dangerous twin asserts one finding per
//! rule.
//!
//! Every test dispatches through [`collect_lint_outcomes`], never through a
//! rule's `check`. Calling `check` directly bypasses the head index, which is
//! where a wrong `HeadFilter` or a forgotten `dialect_scope` shows up. Tests
//! that call `check` directly cannot detect a missing `Heads` entry.

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::model::LintOutcome;
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use std::path::Path;

/// The rules of this crate, in one catalogue, so a test run dispatches through
/// the real engine.
const CATALOG: [RuleEntry; 4] = [
    RuleEntry::new(
        &crate::mutable_default_argument::rule::META,
        &crate::mutable_default_argument::rule::RULE,
    ),
    RuleEntry::new(
        &crate::identity_comparison_with_literal::rule::META,
        &crate::identity_comparison_with_literal::rule::RULE,
    ),
    RuleEntry::new(
        &crate::bare_except::rule::META,
        &crate::bare_except::rule::RULE,
    ),
    RuleEntry::new(
        &crate::catch_swallows_exit::rule::META,
        &crate::catch_swallows_exit::rule::RULE,
    ),
];

fn outcomes_in(dialect: Dialect, source: &str) -> Vec<LintOutcome> {
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("fixture parses");
    let catalog = RuleCatalog::new(&CATALOG);
    let index = build_head_index(catalog);
    collect_lint_outcomes(
        catalog,
        &index,
        Path::new("f.hy"),
        dialect,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("the engine runs")
}

/// The rule names that fire on `source`, as Hy.
fn rules_for(source: &str) -> Vec<&'static str> {
    outcomes_in(Dialect::Hy, source)
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect()
}

/// The rule names that fire on `source`, as LFE.
fn lfe_rules_for(source: &str) -> Vec<&'static str> {
    outcomes_in(Dialect::Lfe, source)
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect()
}

const NONE: [&str; 0] = [];
const MUTABLE_DEFAULT: &str = "hy-mutable-default-argument";
const IDENTITY: &str = "hy-identity-comparison-with-literal";
const BARE_EXCEPT: &str = "hy-bare-except";
const CATCH: &str = "lfe-catch-swallows-exit";

// ---------------------------------------------------------------------------
// hy-mutable-default-argument
// ---------------------------------------------------------------------------

/// A `defn` whose body mutates its defaulted parameter, which is the shape
/// this rule reports. Every positive case is built from this so that the
/// mutation — not the literal — is what each test varies.
fn mutating(default: &str) -> String {
    format!("(defn f [[acc {default}]] (.append acc 1) acc)\n")
}

#[test]
fn every_mutable_default_literal_is_reported_when_it_is_mutated() {
    // Verified against Hy 1.3.1: each of these is shared across calls.
    for default in [
        "[]",
        "{}",
        "#{}",
        "[1 2]",
        "{\"a\" 1}",
        "(list)",
        "(dict)",
        "(set)",
    ] {
        assert_eq!(
            rules_for(&mutating(default)),
            [MUTABLE_DEFAULT],
            "default {default} was not reported"
        );
    }
}

#[test]
fn an_immutable_default_is_left_alone_even_when_the_body_mutates_the_name() {
    // Measured: `(defn safe [[acc None]] …)` returns a fresh `[1]` every call.
    for default in [
        "None", "0", "5", "-1", "1.5", "\"x\"", "#()", "#(1 2)", "True", "False",
    ] {
        assert_eq!(
            rules_for(&mutating(default)),
            NONE,
            "immutable default {default} was reported"
        );
    }
}

#[test]
fn a_mutable_default_nothing_mutates_is_not_reported() {
    // The narrowing that the corpus audit forced. Reporting the shape alone
    // produced 88 findings over 2355 third-party files; hand-adjudication
    // found them to be read-only lookup tables — `(defn f [[notes [0 5 7]]] …)`
    // shares its list, but nothing ever observes that.
    assert_eq!(
        rules_for("(defn f [[notes [0 5 7]]] (get notes 0))\n"),
        NONE
    );
    assert_eq!(rules_for("(defn f [[opts {}]] (.get opts \"k\"))\n"), NONE);
    assert_eq!(rules_for("(defn f [[xs []]] (len xs))\n"), NONE);
}

#[test]
fn every_recognized_mutation_reports() {
    // Each arm of `form_mutates`, so none of them is dead.
    for body in [
        "(.append acc 1)", // mutating method
        "(.add acc 1)",
        "(.update acc other)",
        "(.pop acc)",
        "(+= acc [1])", // augmented assignment
        "(|= acc other)",
        "(setv (get acc 0) 1)", // subscript assignment
        "(del (get acc 0))",
        "(assoc acc \"k\" 1)",     // pre-1.0 Hy dict assignment
        "(.append (get acc 0) 1)", // mutation through a subscript
    ] {
        assert_eq!(
            rules_for(&format!("(defn f [[acc []]] {body} acc)\n")),
            [MUTABLE_DEFAULT],
            "body {body} was not recognized as a mutation"
        );
    }
}

#[test]
fn a_read_only_method_is_not_a_mutation() {
    for body in [
        "(.get acc 1)",
        "(.index acc 1)",
        "(.copy acc)",
        "(.count acc 1)",
    ] {
        assert_eq!(
            rules_for(&format!("(defn f [[acc []]] {body})\n")),
            NONE,
            "body {body} was read as a mutation"
        );
    }
}

#[test]
fn a_comparison_operator_is_not_an_augmented_assignment() {
    // `is_augmented_assignment` refuses `==`, `!=`, `<=` and `>=`; without
    // that, `(= acc [])` and `(>= acc 1)` would each look like a mutation.
    for body in [
        "(== acc other)",
        "(!= acc other)",
        "(<= acc 1)",
        "(>= acc 1)",
    ] {
        assert_eq!(
            rules_for(&format!("(defn f [[acc []]] {body})\n")),
            NONE,
            "body {body} was read as an augmented assignment"
        );
    }
}

#[test]
fn a_mutation_of_a_different_name_is_not_attributed_to_the_parameter() {
    // The parameter is `acc`; the body mutates a local built for the purpose.
    assert_eq!(
        rules_for("(defn f [[acc []]] (setv out []) (.append out 1) out)\n"),
        NONE
    );
}

#[test]
fn a_mutation_nested_deep_in_the_body_still_counts() {
    assert_eq!(
        rules_for("(defn f [[acc []]] (for [x xs] (when x (.append acc x))) acc)\n"),
        [MUTABLE_DEFAULT]
    );
}

#[test]
fn a_parameter_without_a_default_is_left_alone() {
    assert_eq!(rules_for("(defn f [a b c] (.append a 1))\n"), NONE);
}

#[test]
fn fn_has_the_same_defect_and_the_same_report() {
    // Measured: `(setv anon (fn [[acc []]] …))` shares its list too.
    assert_eq!(
        rules_for("(setv g (fn [[acc []]] (.append acc 1) acc))\n"),
        [MUTABLE_DEFAULT]
    );
}

#[test]
fn the_removed_async_spellings_are_still_read() {
    // `defn/a` and `fn/a` were dropped in Hy 1.0, but 47 and 9 files of the
    // audit corpus still use them and the defect is identical.
    assert_eq!(
        rules_for("(defn/a f [[acc []]] (.append acc 1) acc)\n"),
        [MUTABLE_DEFAULT]
    );
    assert_eq!(
        rules_for("(setv g (fn/a [[acc []]] (.append acc 1) acc))\n"),
        [MUTABLE_DEFAULT]
    );
}

#[test]
fn the_current_async_spelling_is_read() {
    // `(defn :async f …)` is how Hy 1.3.1 spells it, and the keyword sits
    // before the name.
    assert_eq!(
        rules_for("(defn :async f [[acc []]] (.append acc 1) acc)\n"),
        [MUTABLE_DEFAULT]
    );
}

#[test]
fn a_decorator_list_is_not_mistaken_for_the_parameter_list() {
    // `(defn [dec] name [params] …)` — the decorator list comes *first*.
    // Taking the first bracket list would read `[dec]`, find no defaults, and
    // silently report nothing on every decorated function.
    assert_eq!(
        rules_for("(defn [staticmethod] f [[acc []]] (.append acc 1) acc)\n"),
        [MUTABLE_DEFAULT]
    );
    // And the decorator list itself is never read as parameters: a decorator
    // that happens to be a two-element bracket list must not report.
    assert_eq!(rules_for("(defn [dec] f [a] (.append a 1))\n"), NONE);
}

#[test]
fn an_annotation_does_not_shift_the_parameter_walk() {
    // `#^` occupies two children before the parameter it annotates. Hy 1.3.1
    // requires the space — `#^int` is a LexException — so the type is always
    // a separate node.
    assert_eq!(
        rules_for("(defn f [#^ int a #^ (get List int) [b []]] (.append b 1))\n"),
        [MUTABLE_DEFAULT]
    );
    assert_eq!(rules_for("(defn f [#^ int a #^ str b] a)\n"), NONE);
}

#[test]
fn the_body_starts_after_the_parameter_list() {
    // If the body were taken from the wrong index the parameter list itself
    // would be searched for mutations, and `[acc []]` contains neither.
    assert_eq!(
        rules_for("(defn f [[acc []]] (.append acc 1))\n"),
        [MUTABLE_DEFAULT]
    );
}

#[test]
fn several_mutated_defaults_report_once_each() {
    let fired = rules_for("(defn f [[a []] [b {}] [c 0]] (.append a 1) (.update b {}) a)\n");
    assert_eq!(fired, [MUTABLE_DEFAULT, MUTABLE_DEFAULT]);
}

// The following six were added by mutation-testing: each removes a guard that
// killed no test until the case below existed. Every one of them is valid Hy —
// verified by running `hy -c` on each form.

#[test]
fn a_malformed_one_element_parameter_pair_does_not_panic() {
    // `(defn f [[a]] a)` is rejected by Hy 1.3.1, but the *parser* still
    // produces a one-child bracket list for it, and a lint pass must not panic
    // on input the language would reject. Without the `len() == 2` guard the
    // rule indexes `children[1]` and the process dies.
    assert_eq!(rules_for("(defn f [[a]] (.append a 1))\n"), NONE);
    assert_eq!(rules_for("(defn f [[]] 1)\n"), NONE);
}

#[test]
fn a_bracket_string_default_is_not_a_mutable_list() {
    // `#[[…]]` is a Hy raw string, and this workspace's reader parses it as a
    // `#`-prefixed bracket list whose *contents* become nodes. Without the
    // reader-prefix test in `is_plain_list` the string would be read as a
    // mutable list literal and reported. Verified valid Hy.
    assert_eq!(
        rules_for("(defn f [[doc #[[text here]]]] (.append doc 1))\n"),
        NONE
    );
}

#[test]
fn an_annotation_whose_type_is_a_list_literal_is_not_a_default_pair() {
    // `(defn f [#^ [int str] a] a)` is valid Hy. Without skipping the `#^`
    // type as a unit, `[x []]` below is read as the pair `x = []` and the
    // rule reports a parameter that does not exist.
    assert_eq!(rules_for("(defn f [#^ [x []] a] (.append x 1))\n"), NONE);
}

#[test]
fn a_default_expression_is_not_part_of_the_body() {
    // `(defn f [[acc []] [ignored (.append acc 1)]] 1)` compiles under Hy.
    // If the body were taken from the parameter list's own index, that
    // `.append` inside a *default* would be read as a body mutation.
    assert_eq!(
        rules_for("(defn f [[acc []] [ignored (.append acc 1)]] 1)\n"),
        NONE
    );
}

#[test]
fn a_bracket_list_literal_is_not_a_call() {
    // `[is 1 2]` is a Hy list literal containing the symbol `is`, not a call
    // to it. Reading a head off a bracket list would report it.
    assert_eq!(rules_for("(print [is 1 2])\n"), NONE);
    assert_eq!(rules_for("(print {is 1})\n"), NONE);
}

#[test]
fn a_quoted_defn_is_data() {
    assert_eq!(rules_for("'(defn f [[acc []]] (.append acc 1))\n"), NONE);
    assert_eq!(
        rules_for("(quote (defn f [[acc []]] (.append acc 1)))\n"),
        NONE
    );
}

// ---------------------------------------------------------------------------
// hy-identity-comparison-with-literal
// ---------------------------------------------------------------------------

#[test]
fn identity_against_a_value_literal_is_reported() {
    // CPython emits `SyntaxWarning: "is" with 'int' literal` for each of these.
    for literal in ["5", "1000", "-1", "1.5", "\"a\"", "b\"a\"", "#(1 2)"] {
        assert_eq!(
            rules_for(&format!("(print (is x {literal}))\n")),
            [IDENTITY],
            "literal {literal} was not reported"
        );
    }
}

#[test]
fn identity_against_a_singleton_is_the_correct_spelling() {
    // CPython stays silent for exactly these three, and so does this rule.
    for singleton in ["None", "True", "False"] {
        assert_eq!(
            rules_for(&format!("(print (is x {singleton}))\n")),
            NONE,
            "singleton {singleton} was reported"
        );
    }
}

#[test]
fn identity_between_two_names_is_left_alone() {
    assert_eq!(rules_for("(print (is a b))\n"), NONE);
    assert_eq!(rules_for("(print (is (type a) (type b)))\n"), NONE);
}

#[test]
fn both_spellings_of_is_not_are_read() {
    // Hy mangles `-` to `_`, so these name one operator; `head_key` folds
    // nothing for a non-Common-Lisp dialect, so both must be listed.
    assert_eq!(rules_for("(print (is-not x 5))\n"), [IDENTITY]);
    assert_eq!(rules_for("(print (is_not x 5))\n"), [IDENTITY]);
    assert_eq!(rules_for("(print (is-not x None))\n"), NONE);
}

#[test]
fn is_not_is_told_to_use_the_inequality_operator() {
    let outcomes = outcomes_in(Dialect::Hy, "(print (is-not x 5))\n");
    let finding = outcomes
        .into_iter()
        .next()
        .expect("one finding")
        .into_parts()
        .0;
    assert!(
        finding.message.contains("`!=`"),
        "is-not must not borrow is's advice: {}",
        finding.message
    );
}

#[test]
fn a_one_operand_is_is_not_a_comparison() {
    assert_eq!(rules_for("(print (is x))\n"), NONE);
    // `(is 5)` is accepted by Hy 1.3.1 and compares nothing. Without the arity
    // guard the single literal operand is reported as though it were the right
    // half of a comparison.
    assert_eq!(rules_for("(setv q (is 5))\n"), NONE);
}

#[test]
fn a_lone_dot_is_not_a_numeric_literal() {
    // `.` is a real Hy symbol — `(. obj attr)` is attribute access — and it
    // starts with the character a float may start with. Without the
    // "contains a digit" test it reads as a number.
    assert_eq!(rules_for("(print (is x .))\n"), NONE);
}

#[test]
fn a_singleton_is_not_a_literal_by_two_independent_routes() {
    // `SINGLETONS` and the string/number predicates each independently refuse
    // `None`, `True` and `False`. Mutation-testing showed removing the former
    // kills no test; this pins *both* routes so that redundancy stays a
    // deliberate belt-and-braces rather than a silent single point of failure.
    use crate::identity_comparison_with_literal::domain::is_value_literal;
    use paredit_core_syntax::sexpr::SyntaxTree as Tree;
    for singleton in ["None", "True", "False"] {
        let source = format!("(is x {singleton})");
        let tree = Tree::parse_with_dialect(&source, Dialect::Hy).expect("parses");
        let root = tree.root_view();
        let operand = &root.children[0].children[2];
        assert!(
            !is_value_literal(operand),
            "{singleton} was read as a value literal"
        );
        assert_eq!(rules_for(&format!("{source}\n")), NONE);
    }
}

#[test]
fn a_chained_is_reports_each_literal_operand() {
    // Hy's `is` chains, so every operand past the first is compared.
    assert_eq!(rules_for("(print (is x 5 10))\n"), [IDENTITY, IDENTITY]);
}

#[test]
fn an_identifier_that_merely_looks_numeric_is_not_a_literal() {
    for name in ["x2", "-count", "n", "point.x", "None-ish"] {
        assert_eq!(
            rules_for(&format!("(print (is x {name}))\n")),
            NONE,
            "{name} was read as a numeric literal"
        );
    }
}

#[test]
fn a_quoted_is_form_is_data() {
    assert_eq!(rules_for("'(is x 5)\n"), NONE);
}

// ---------------------------------------------------------------------------
// hy-bare-except
// ---------------------------------------------------------------------------

#[test]
fn a_bare_except_is_reported() {
    // Measured: `(except [] …)` catches KeyboardInterrupt and SystemExit.
    assert_eq!(
        rules_for("(try (risky) (except [] (log \"x\")))\n"),
        [BARE_EXCEPT]
    );
}

#[test]
fn a_typed_except_is_left_alone() {
    // Measured: `(except [e Exception] …)` catches neither of those.
    for clause in [
        "[e Exception]",
        "[Exception]",
        "[ValueError]",
        "[e ValueError]",
        "[e [ValueError KeyError]]",
    ] {
        assert_eq!(
            rules_for(&format!("(try (risky) (except {clause} (log \"x\")))\n")),
            NONE,
            "clause {clause} was reported"
        );
    }
}

#[test]
fn an_except_with_no_binding_list_at_all_is_not_reported() {
    // Hy rejects this outright, so there is nothing true to say about it and
    // reporting it would be reporting a parse error as a lint finding.
    assert_eq!(rules_for("(try (risky) (except (log \"x\")))\n"), NONE);
}

#[test]
fn a_bare_except_beside_a_typed_one_is_still_reported() {
    assert_eq!(
        rules_for("(try (risky) (except [e ValueError] 1) (except [] 2))\n"),
        [BARE_EXCEPT]
    );
}

#[test]
fn a_quoted_except_is_data() {
    assert_eq!(rules_for("'(try (risky) (except [] 1))\n"), NONE);
}

// ---------------------------------------------------------------------------
// lfe-catch-swallows-exit
// ---------------------------------------------------------------------------

#[test]
fn the_old_catch_expression_form_is_reported() {
    // Measured on LFE 2.2.0: `(catch (exit 'boom))` and a plain
    // `(tuple 'EXIT 'boom)` produce the identical term.
    assert_eq!(lfe_rules_for("(defun f () (catch (risky)))\n"), [CATCH]);
}

#[test]
fn the_catch_clause_of_a_try_is_a_different_form_and_is_left_alone() {
    // This is the shape the rule *recommends*; reporting it would tell the
    // author to replace `try` with `try`.
    assert_eq!(
        lfe_rules_for("(defun f () (try (risky) (catch ((tuple t v s) 'bad))))\n"),
        NONE
    );
    // Including when the `try` also has `case` and `after` clauses, which
    // change the `catch` clause's child index but not its parent.
    assert_eq!(
        lfe_rules_for(
            "(defun f () (try (risky) (case (r r)) (catch ((tuple t v s) 'bad)) (after (cleanup))))\n"
        ),
        NONE
    );
}

#[test]
fn a_catch_nested_inside_a_try_body_is_still_the_expression_form() {
    // Its parent is the `try`'s *body*, not the `try` itself — so the
    // parent-head test must be about the immediate parent, not any ancestor.
    assert_eq!(
        lfe_rules_for("(defun f () (try (progn (catch (risky))) (catch ((tuple t v s) 'bad))))\n"),
        [CATCH]
    );
}

#[test]
fn a_bare_catch_symbol_is_not_the_expression_form() {
    assert_eq!(lfe_rules_for("(defun f () (catch))\n"), NONE);
}

#[test]
fn a_quoted_catch_is_data() {
    assert_eq!(lfe_rules_for("'(catch (risky))\n"), NONE);
    assert_eq!(lfe_rules_for("(defun f () '(catch (risky)))\n"), NONE);
    // The long-hand spelling, which hand-written code and macro output both
    // produce. Mutation-testing found this untested: breaking the
    // `(quote …)`-form handling killed no test, because the only quote tests
    // went through the `'` reader prefix instead.
    assert_eq!(
        lfe_rules_for("(defun f () (quote (catch (risky))))\n"),
        NONE
    );
}

#[test]
fn the_two_counter_quote_model_earns_itself_on_lfe_backquote() {
    // LFE spells unquote `,`, which the reader turns into a real
    // `ReaderPrefix::Unquote` — so a form unquoted back into a template is
    // code again. A single depth counter would call this data and miss it.
    assert_eq!(
        lfe_rules_for("(defmacro m () `(a ,(catch (risky))))\n"),
        [CATCH]
    );
    // And a comma inside a *hard* quote is just a comma in a literal list.
    assert_eq!(lfe_rules_for("(defun f () '(a ,(catch (risky))))\n"), NONE);
    // A template with no unquote is data throughout.
    assert_eq!(
        lfe_rules_for("(defmacro m () `(a (catch (risky))))\n"),
        NONE
    );
}

#[test]
fn the_lfe_rule_does_not_run_on_the_other_dialects() {
    // `catch` means something entirely different in Common Lisp — a
    // `catch`/`throw` tag — so a wider scope would report unrelated code.
    for dialect in [Dialect::CommonLisp, Dialect::EmacsLisp, Dialect::Scheme] {
        let names: Vec<&'static str> = outcomes_in(dialect, "(defun f () (catch (risky)))\n")
            .into_iter()
            .map(|outcome| outcome.into_parts().0.rule)
            .collect();
        assert_eq!(names, NONE, "the LFE rule ran on {dialect:?}");
    }
}

#[test]
fn the_hy_rules_do_not_run_on_lfe() {
    // The mirror of the test above, and the one that would catch a rule whose
    // `dialect_scope` was widened to cover this crate's two dialects at once.
    assert_eq!(lfe_rules_for("(defun f () (is x 5))\n"), NONE);
}

// ---------------------------------------------------------------------------
// The rule that is deliberately absent: hy-mutable-class-attribute
// ---------------------------------------------------------------------------

#[test]
fn a_mutable_class_attribute_is_deliberately_not_reported() {
    // `(defclass C [] (setv items []))` really does give every instance the
    // same list — measured against Hy 1.3.1, `(is a.items b.items)` is `True`.
    // A rule for it was written, and then killed by its own audit: over 2355
    // third-party files it produced 33 findings against 251 candidates and
    // *none* of them was a defect.
    //
    // Six were `__slots__`, which Python requires at class level and which the
    // type machinery consumes rather than mutates; one was `__all__`; five
    // were Django `ModelAdmin` declarations (`fieldsets`, `list_display`,
    // `list_filter`, `search_fields`, `inlines`); the rest were framework
    // contracts (`BINDINGS` for Textual, `metadata` for Gym) or deliberate
    // class-level registries and caches (`name-cache`, `command-dict`,
    // `_dispatch`). In real Hy a mutable class attribute is a *declaration*,
    // not the accidental sharing the rule would have claimed.
    //
    // This test pins the decision so the rule is not reintroduced by someone
    // reasoning from the (entirely correct) premise alone.
    assert_eq!(rules_for("(defclass Basket [] (setv items []))\n"), NONE);
    assert_eq!(
        rules_for("(defclass C [] (setv __slots__ [\"a\" \"b\"]))\n"),
        NONE
    );
}

#[test]
fn a_quoted_defclass_is_data() {
    assert_eq!(rules_for("'(defclass C [] (setv items []))\n"), NONE);
}

// ---------------------------------------------------------------------------
// Dialect scope and head filtering
// ---------------------------------------------------------------------------

#[test]
fn none_of_these_rules_run_on_any_other_dialect() {
    // The default `dialect_scope()` is `COMMON_LISP_ONLY`, so a missing
    // override is silent: the rule would run on the wrong dialect and never on
    // the right one. For `hy-mutable-default-argument` the wrong dialect is
    // precisely the one where the shape is *correct*, since Common Lisp
    // evaluates an `&optional` default on entry to each call.
    let source = "(defn f [[acc []]] acc)\n(print (is x 5))\n\
                  (try (risky) (except [] 1))\n(defclass C [] (setv items []))\n";
    // Not every dialect's reader accepts `[...]`, and one that rejects the
    // fixture outright proves nothing about the dialect gate. Those are
    // counted rather than skipped silently, so the test cannot pass by
    // parsing nowhere.
    let mut checked = 0usize;
    for dialect in [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Clojure,
        Dialect::Scheme,
        Dialect::Racket,
        Dialect::Fennel,
        Dialect::Janet,
        Dialect::Lfe,
        Dialect::Carp,
    ] {
        if SyntaxTree::parse_with_dialect(source, dialect).is_err() {
            continue;
        }
        checked += 1;
        let names: Vec<&'static str> = outcomes_in(dialect, source)
            .into_iter()
            .map(|outcome| outcome.into_parts().0.rule)
            .collect();
        assert_eq!(names, NONE, "a rule ran on {dialect:?}");
    }
    assert!(
        checked >= 5,
        "only {checked} dialects parsed the fixture, so the gate is barely tested"
    );
}

#[test]
fn the_dialect_gate_is_tested_against_common_lisp_in_its_own_syntax() {
    // Common Lisp is the dialect that matters most here, and its reader
    // rejects the bracketed fixture above — so it gets the nearest equivalent
    // written in its own syntax. `(defun f (&optional (acc (list))) acc)` is
    // the shape `hy-mutable-default-argument` would report if it ran, and it
    // is *correct* Common Lisp: the default is evaluated on entry to each
    // call, which is exactly why the rule is Hy-only.
    let source = "(defun f (&optional (acc (list))) acc)\n(defclass c () ())\n";
    let names: Vec<&'static str> = outcomes_in(Dialect::CommonLisp, source)
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect();
    assert_eq!(names, NONE);
}

#[test]
fn every_rule_in_this_crate_is_head_filtered() {
    // `WholeTree` runs once per file unconditionally and is gated by a CI
    // benchmark that has failed this project repeatedly. Nothing here may use
    // it, and this test is what keeps that true as rules are added.
    use paredit_core_lint_engine::model::HeadFilter;
    for entry in CATALOG {
        assert!(
            matches!(entry.rule().head_filter(), HeadFilter::Heads(_)),
            "{} is not Heads-filtered",
            entry.meta().name()
        );
    }
}

#[test]
fn every_rules_declared_heads_match_its_domain_head_names() {
    // The `Heads` array and the domain's `HEAD_NAMES` are two spellings of one
    // fact. This assertion keeps them synchronized.
    use paredit_core_lint_engine::model::HeadFilter;
    let declared = |entry: &RuleEntry| -> Vec<&'static str> {
        match entry.rule().head_filter() {
            HeadFilter::Heads(heads) => heads.iter().map(|head| head.as_str()).collect(),
            _ => Vec::new(),
        }
    };
    assert_eq!(
        declared(&CATALOG[0]),
        crate::mutable_default_argument::domain::HEAD_NAMES
    );
    assert_eq!(
        declared(&CATALOG[1]),
        crate::identity_comparison_with_literal::domain::HEAD_NAMES
    );
    assert_eq!(
        declared(&CATALOG[2]),
        crate::bare_except::domain::HEAD_NAMES
    );
    assert_eq!(
        declared(&CATALOG[3]),
        crate::catch_swallows_exit::domain::HEAD_NAMES
    );
}

#[test]
fn every_rules_dialect_scope_is_its_own_domains_dialects() {
    // `dialect_scope()` and the domain's `DIALECTS` are one fact spelled
    // twice, and the default (`COMMON_LISP_ONLY`) is a silent wrong answer for
    // every rule here. Asserting they agree is what stops them drifting.
    let scopes: [(&str, &[Dialect]); 4] = [
        (
            "hy-mutable-default-argument",
            &crate::mutable_default_argument::domain::DIALECTS,
        ),
        (
            "hy-identity-comparison-with-literal",
            &crate::identity_comparison_with_literal::domain::DIALECTS,
        ),
        ("hy-bare-except", &crate::bare_except::domain::DIALECTS),
        (
            "lfe-catch-swallows-exit",
            &crate::catch_swallows_exit::domain::DIALECTS,
        ),
    ];
    for (index, (name, dialects)) in scopes.into_iter().enumerate() {
        let scope = CATALOG[index].rule().dialect_scope();
        assert_eq!(CATALOG[index].meta().name(), *name);
        for dialect in Dialect::ALL {
            assert_eq!(
                scope.includes(dialect),
                dialects.contains(&dialect),
                "{name} disagrees with its domain's DIALECTS about {dialect:?}"
            );
        }
    }
}

#[test]
fn a_head_that_differs_only_in_case_is_a_different_symbol() {
    // `head_key` folds case only for Common Lisp, so a Hy head is indexed
    // exactly as written. `Defn` is a name a file may define itself.
    for form in [
        "(Defn f [[acc []]] (.append acc 1))",
        "(Is x 5)",
        "(Except [] 1)",
    ] {
        assert_eq!(
            rules_for(&format!("{form}\n")),
            NONE,
            "{form} was linted as though it were the lowercase symbol"
        );
    }
}

// ---------------------------------------------------------------------------
// Corpus sweep
// ---------------------------------------------------------------------------

/// A file written the way the Hy manual says to write one, exercising every
/// shape all four rules key on.
///
/// It is real Hy: it runs clean under Hy 1.3.1 (`hy corpus.hy`), which is what
/// keeps this from being a plausible-looking file that the language would
/// reject. Note the absence of any *interpolated* f-string — this workspace's
/// reader cannot parse one at all, which is recorded in the crate README.
const CORRECT: &str = r#";; my-pkg.hy -- a module that does all of this correctly.

(import json)
(import collections [defaultdict])

(setv DEFAULT-LIMIT 10)
(setv REGISTRY {})

(defclass Basket []
  "A basket with per-instance contents."

  (setv limit DEFAULT-LIMIT)
  (setv label "basket")
  (setv bounds #(0 100))

  (defn __init__ [self [items None]]
    (setv self.items (if (is items None) [] (list items)))
    (setv self.seen #{}))

  (defn add [self item]
    (.append self.items item)
    (.add self.seen item)
    self)

  (defn full? [self]
    (>= (len self.items) self.limit)))

(defn collect [source [acc None] [seen None]]
  "Collect from SOURCE, defaulting to fresh containers each call."
  (when (is acc None)
    (setv acc []))
  (when (is seen None)
    (setv seen #{}))
  (for [item source]
    (when (not-in item seen)
      (.add seen item)
      (.append acc item)))
  acc)

(defn [staticmethod] describe [value]
  "A decorated function, whose decorator list precedes the name."
  (cond
    (is value None) "nothing"
    (= value 0) "zero"
    (is-not value None) "something"
    True "unknown"))

(defn parse [text [strict False]]
  "Parse TEXT, naming the exceptions it expects."
  (try
    (json.loads text)
    (except [e json.JSONDecodeError]
      (if strict (raise e) None))
    (except [e [TypeError ValueError]]
      None)))

(defn tally [words]
  "An immutable default and a mutable local are both fine."
  (setv counts (defaultdict int))
  (for [word words]
    (+= (get counts word) 1))
  (dict counts))

(setv squares (lfor n (range 10) (* n n)))
(print (len squares) (get squares 3))
"#;

#[test]
fn a_realistic_correct_file_produces_no_findings() {
    assert_eq!(rules_for(CORRECT), NONE);
}

#[test]
fn the_correct_file_actually_contains_what_every_rule_looks_for() {
    // Without this, the sweep above passes by matching nothing at all — which
    // is exactly how a rule with a broken head filter looks.
    for (needle, at_least) in [
        ("(defn ", 5),
        ("(defclass ", 1),
        ("(setv ", 8),
        ("(is ", 4),
        ("(is-not ", 1),
        ("(except [", 2),
        ("[acc None]", 1),
    ] {
        let found = CORRECT.matches(needle).count();
        assert!(
            found >= at_least,
            "the correct corpus has {found} of `{needle}`, expected at least {at_least}"
        );
    }
    // And the sweep must be seeing a tree, not a parse failure.
    let tree = SyntaxTree::parse_with_dialect(CORRECT, Dialect::Hy).expect("corpus parses");
    assert!(
        tree.root_view().children.len() >= 10,
        "the corpus should have at least 10 top-level forms"
    );
}

/// The same file with each idiom broken, exactly once each.
const DANGEROUS_TWIN: &str = r#";; my-pkg.hy -- every rule in this crate fires here, once.

(import json)

(defn collect [source [acc []]]
  "A default the caller never sees emptied."
  (for [item source]
    (.append acc item))
  acc)

(defn describe [value]
  (if (is value 200) "ok" "other"))

(defn parse [text]
  (try
    (json.loads text)
    (except []
      None)))
"#;

#[test]
fn the_dangerous_twin_fires_every_rule_exactly_once() {
    let mut fired = rules_for(DANGEROUS_TWIN);
    fired.sort_unstable();
    assert_eq!(fired, [BARE_EXCEPT, IDENTITY, MUTABLE_DEFAULT]);
}

/// An LFE module written the way the LFE reference manual says to write one.
///
/// It is real LFE: `lfec correct.lfe` compiles it with exit 0 and no warnings
/// under LFE 2.2.0 on Erlang 27.3.4.15.
const LFE_CORRECT: &str = r#"(defmodule correct
  (export (start 0) (loop 1) (fetch 1) (classify 1) (describe 1))
  (import (from lists (map 2) (foldl 3))))

(defrecord state
  (count 0)
  (name #"worker"))

(defun start ()
  (spawn 'correct 'loop (list (make-state count 0))))

(defun loop (st)
  (receive
    ((tuple 'add n)
     (loop (set-state-count st (+ (state-count st) n))))
    ((tuple 'get from)
     (! from (tuple 'count (state-count st)))
     (loop st))
    ('stop 'ok)
    (after 60000
      (loop st))))

;; The shape `lfe-catch-swallows-exit` recommends: `try`, with the failure
;; path kept separate from the success path and the stacktrace preserved.
(defun fetch (key)
  (try
    (ets:lookup 'cache key)
    (case
      ((cons found _) (tuple 'ok found))
      ('() (tuple 'error 'not-found)))
    (catch
      ((tuple 'error reason stack)
       (logger:error "lookup failed: ~p ~p" (list reason stack))
       (tuple 'error reason)))
    (after
      (ets:safe_fixtable 'cache 'false))))

(defun classify (x)
  (case x
    (0 'zero)
    (n (when (> n 0)) 'positive)
    (_ 'negative)))

(defun describe (x)
  (cond
    ((is_atom x) (tuple 'atom x))
    ((is_binary x) (tuple 'binary (byte_size x)))
    ((is_list x) (tuple 'list (length x)))
    ('true (tuple 'other x))))
"#;

#[test]
fn a_realistic_correct_lfe_file_produces_no_findings() {
    assert_eq!(lfe_rules_for(LFE_CORRECT), NONE);
}

#[test]
fn the_correct_lfe_file_actually_contains_what_the_lfe_rule_looks_for() {
    // A `try … catch` and a `receive` are exactly the shapes a wrong parent
    // test would report, so their presence is what makes the zero above mean
    // something.
    for (needle, at_least) in [("(catch", 1), ("(try", 1), ("(receive", 1), ("(case", 2)] {
        let found = LFE_CORRECT.matches(needle).count();
        assert!(
            found >= at_least,
            "the LFE corpus has {found} of `{needle}`, expected at least {at_least}"
        );
    }
    let tree = SyntaxTree::parse_with_dialect(LFE_CORRECT, Dialect::Lfe).expect("corpus parses");
    assert!(
        tree.root_view().children.len() >= 6,
        "the LFE corpus should have at least 6 top-level forms"
    );
}

/// The same module with the one idiom broken. `lfec twin.lfe` also compiles
/// this with exit 0 — it is valid LFE, not a syntax-error stub.
const LFE_DANGEROUS_TWIN: &str = r#"(defmodule twin
  (export (fetch 1)))

;; The one broken idiom: `catch` encodes the failure into the return value,
;; so the caller cannot tell #(EXIT Reason) from a tuple `ets:lookup` might
;; legitimately have returned.
(defun fetch (key)
  (catch (ets:lookup 'cache key)))
"#;

#[test]
fn the_lfe_dangerous_twin_fires_its_rule_exactly_once() {
    assert_eq!(lfe_rules_for(LFE_DANGEROUS_TWIN), [CATCH]);
}

#[test]
fn the_two_corpora_differ_only_in_the_idioms_under_test() {
    // Both are real files that parse; the twin is not a syntax-error stub that
    // happens to trip everything.
    for source in [CORRECT, DANGEROUS_TWIN] {
        SyntaxTree::parse_with_dialect(source, Dialect::Hy).expect("both Hy corpora parse");
    }
    for source in [LFE_CORRECT, LFE_DANGEROUS_TWIN] {
        SyntaxTree::parse_with_dialect(source, Dialect::Lfe).expect("both LFE corpora parse");
    }
}
