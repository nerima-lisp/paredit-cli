//! Running a project's own rules over one file.
//!
//! A second pass, alongside the shipped suite's. That is forced rather than
//! chosen: [`paredit_core_lint_engine::rule::RuleCatalog`] holds
//! `&'static [RuleEntry]`, because deriving `RULES`, `RULE_DOCS`,
//! `FIXABLE_RULES`, and `WARNING_RULES` from one array at compile time is what
//! keeps the four from drifting apart. A rule read from a file at startup has
//! no `'static` lifetime to offer.
//!
//! What the two passes share is the part that matters: the finding type, so
//! every output format renders both; the report order, so a run is
//! reproducible; and nothing else. A custom rule cannot slow the shipped suite
//! down, cannot shadow one of its rules (the name check in
//! [`super::ruleset::Ruleset::validate`] sees to that), and cannot make it
//! report differently.
//!
//! Matching and rewriting are the `--query`/`--rewrite` engine's own
//! [`match_all`] and [`plan_rewrite`] — not a second implementation. That is
//! what a custom rule's `:fix` gains the same rewrite-hazard detection a
//! `--rewrite` gets: a match this pass reports is still reported (a `defrule`
//! finding is one per occurrence, hazardous or not — a project should still
//! learn its rule fired), but a hazardous one's `fix` comes back `None`
//! rather than a replacement that would corrupt quoted data, drop a comment,
//! or overlap another match already applied.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::match_all;
use paredit_core_syntax::selector::rewrite::{RewriteAllowances, plan_rewrite};
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};

use crate::ruleset::{CustomRule, Ruleset};

/// One custom rule's complaint about one form.
///
/// Deliberately primitive and deliberately *not*
/// [`paredit_core_lint_engine::model::LintFinding`]: that type's `rule` is a
/// `&'static str`, which a name read from a file cannot be. The caller owns
/// the conversion, which is one `to_owned` and keeps the engine's contract
/// intact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomFinding {
    pub rule: String,
    pub span: ByteSpan,
    pub message: String,
    /// The replacement source for `span`, when the rule declared a `:fix`
    /// *and* rewriting this particular match is not one of the hazards
    /// [`paredit_core_syntax::selector::rewrite`] refuses by default.
    pub fix: Option<String>,
}

/// Every custom finding in one parsed file, in the same order the shipped pass
/// uses: rule order, then each rule's own pre-order over the document.
///
/// `dialect` is the *file being linted*'s dialect, not the rule file's (rule
/// files are always read as Common Lisp; see
/// [`crate::ruleset::parse_ruleset`]). Matching a custom pattern with the
/// target's own dialect is what lets `(defun ?name ...)` fold case in a
/// Common Lisp file and stay case-sensitive in an Emacs Lisp one, the same as
/// `--query` already does — the old bespoke matcher folded case
/// unconditionally, which was a latent wrong answer for every other dialect.
#[must_use]
pub fn run(ruleset: &Ruleset, tree: &SyntaxTree, dialect: Dialect) -> Vec<CustomFinding> {
    let source = tree.source();
    let mut found = Vec::new();
    for rule in &ruleset.rules {
        collect_rule(rule, tree, source, dialect, &mut found);
    }
    found
}

fn collect_rule(
    rule: &CustomRule,
    tree: &SyntaxTree,
    source: &str,
    dialect: Dialect,
    found: &mut Vec<CustomFinding>,
) {
    let matches = match_all(tree, &rule.pattern, dialect);

    // Computed once per rule rather than per match: `plan_rewrite` walks the
    // whole document itself, so asking it once for the spans it declines to
    // rewrite is the same cost as asking `match_all` for the matches, not the
    // product of the two.
    let skipped: Vec<ByteSpan> = match &rule.fix {
        None => Vec::new(),
        Some(template) => plan_rewrite(
            tree,
            &rule.pattern,
            template,
            dialect,
            RewriteAllowances::default(),
        )
        .skipped
        .into_iter()
        .map(|skipped| skipped.span)
        .collect(),
    };

    for found_match in matches {
        let fix = rule.fix.as_ref().and_then(|template| {
            (!skipped.contains(&found_match.span))
                .then(|| template.render(source, &found_match.captures))
        });
        found.push(CustomFinding {
            rule: rule.name.clone(),
            span: found_match.span,
            message: rule.message.clone(),
            fix,
        });
    }
}

/// What running one rule's declarative tests found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFailure {
    pub rule: String,
    /// The clause that failed, as it was written (`:matches`, `:no-match`,
    /// `:fix`).
    pub clause: &'static str,
    pub input: String,
    /// What the clause asked for, and what happened.
    pub expected: String,
    pub actual: String,
}

/// Runs every `deftest` in the set and reports what failed.
///
/// A rule file is code, and code that nobody can check goes wrong quietly. The
/// harness is what makes a rule set something a project can keep correct while
/// the codebase moves: `:no-match` in particular is the clause that catches the
/// pattern which grew too broad.
#[must_use]
pub fn run_tests(ruleset: &Ruleset) -> Vec<TestFailure> {
    let mut failures = Vec::new();
    for test in &ruleset.tests {
        let Some(rule) = ruleset.rules.iter().find(|rule| rule.name == test.rule) else {
            // `validate` rejects this before a run reaches here; skipping keeps
            // the harness total for a caller that did not validate.
            continue;
        };

        for input in &test.matches {
            if findings_for(rule, input).is_empty() {
                failures.push(TestFailure {
                    rule: rule.name.clone(),
                    clause: ":matches",
                    input: input.clone(),
                    expected: "a finding".to_owned(),
                    actual: "none".to_owned(),
                });
            }
        }
        for input in &test.no_match {
            let count = findings_for(rule, input).len();
            if count > 0 {
                failures.push(TestFailure {
                    rule: rule.name.clone(),
                    clause: ":no-match",
                    input: input.clone(),
                    expected: "no finding".to_owned(),
                    actual: format!("{count} finding(s)"),
                });
            }
        }
        for (before, after) in &test.fixes {
            let produced = findings_for(rule, before)
                .into_iter()
                .find_map(|finding| finding.fix);
            let actual = produced.unwrap_or_else(|| "no fix".to_owned());
            if actual != *after {
                failures.push(TestFailure {
                    rule: rule.name.clone(),
                    clause: ":fix",
                    input: before.clone(),
                    expected: after.clone(),
                    actual,
                });
            }
        }
    }
    failures
}

/// One rule's findings over a snippet, for the harness.
///
/// Always read as Common Lisp: a `deftest` clause tests a rule against a
/// snippet written in the rule *file*'s own language, independent of whichever
/// dialect the rule ends up applied to at lint time (see [`run`]).
fn findings_for(rule: &CustomRule, source: &str) -> Vec<CustomFinding> {
    let Ok(tree) = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    collect_rule(rule, &tree, source, Dialect::CommonLisp, &mut found);
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ruleset::parse_ruleset;

    fn ruleset(text: &str) -> Ruleset {
        parse_ruleset("rules.lisp", text).expect("a readable ruleset")
    }

    fn findings(rules: &str, source: &str) -> Vec<CustomFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        run(&ruleset(rules), &tree, Dialect::CommonLisp)
    }

    #[test]
    fn a_matching_form_produces_a_finding_with_the_rules_message() {
        let found = findings(
            r#"(defrule no-print :pattern (print ?x) :message "use format")"#,
            "(defun f () (print 1))",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, "no-print");
        assert_eq!(found[0].message, "use format");
        assert!(found[0].fix.is_none());
    }

    #[test]
    fn a_rule_with_a_fix_renders_the_substituted_template() {
        let found = findings(
            r#"(defrule no-print :pattern (print ?x) :message "m" :fix (format t "~a~%" ?x))"#,
            "(print (compute 1))",
        );
        assert_eq!(
            found[0].fix.as_deref(),
            Some(r#"(format t "~a~%" (compute 1))"#)
        );
    }

    #[test]
    fn a_non_matching_file_produces_nothing() {
        assert!(
            findings(
                r#"(defrule no-print :pattern (print ?x) :message "m")"#,
                "(defun f () (format t \"1\"))",
            )
            .is_empty()
        );
    }

    #[test]
    fn findings_come_back_in_rule_order_then_document_order() {
        let found = findings(
            r#"(defrule second :pattern (g ?x) :message "m")
               (defrule first :pattern (f ?x) :message "m")"#,
            "(g 1) (f 2) (f 3)",
        );
        // Rule order first: both `second` findings, then both `first` ones.
        let rules: Vec<&str> = found.iter().map(|f| f.rule.as_str()).collect();
        assert_eq!(rules, vec!["second", "first", "first"]);
        // And within a rule, document order.
        assert!(found[1].span.start().get() < found[2].span.start().get());
    }

    #[test]
    fn a_deprecation_matches_a_call_of_any_arity() {
        let found = findings(
            "(deprecate legacy-connect :use connect)",
            "(legacy-connect) (legacy-connect a b)",
        );
        assert_eq!(found.len(), 2);
        assert!(found[0].message.contains("use connect instead"));
    }

    #[test]
    fn a_deprecation_does_not_match_the_bare_symbol() {
        // `#'legacy-connect` and a variable of that name are references, not
        // calls, and a deprecation about calls should not claim them.
        assert!(findings("(deprecate old-fn)", "(list #'old-fn old-fn)").is_empty());
    }

    #[test]
    fn the_harness_passes_a_rule_that_does_what_its_tests_say() {
        let failures = run_tests(&ruleset(
            r#"(defrule no-print :pattern (print ?x) :message "m" :fix (princ ?x))
               (deftest no-print
                 (:matches "(print 1)")
                 (:no-match "(princ 1)")
                 (:fix "(print 1)" "(princ 1)"))"#,
        ));
        assert!(failures.is_empty(), "{failures:?}");
    }

    #[test]
    fn the_harness_reports_a_matches_clause_that_does_not() {
        let failures = run_tests(&ruleset(
            r#"(defrule no-print :pattern (print ?x) :message "m")
               (deftest no-print (:matches "(princ 1)"))"#,
        ));
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].clause, ":matches");
        assert_eq!(failures[0].actual, "none");
    }

    #[test]
    fn the_harness_reports_a_pattern_that_grew_too_broad() {
        // The clause that earns the harness its keep: a rule meant for `print`
        // whose pattern also takes `princ`.
        let failures = run_tests(&ruleset(
            r#"(defrule no-print :pattern (?op ?x) :message "m")
               (deftest no-print (:no-match "(princ 1)"))"#,
        ));
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].clause, ":no-match");
        assert_eq!(failures[0].actual, "1 finding(s)");
    }

    #[test]
    fn the_harness_reports_a_fix_that_produces_the_wrong_text() {
        let failures = run_tests(&ruleset(
            r#"(defrule no-print :pattern (print ?x) :message "m" :fix (princ ?x))
               (deftest no-print (:fix "(print 1)" "(write 1)"))"#,
        ));
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].clause, ":fix");
        assert_eq!(failures[0].expected, "(write 1)");
        assert_eq!(failures[0].actual, "(princ 1)");
    }

    #[test]
    fn the_harness_reports_a_fix_clause_on_a_rule_with_no_fix() {
        let failures = run_tests(&ruleset(
            r#"(defrule no-print :pattern (print ?x) :message "m")
               (deftest no-print (:fix "(print 1)" "(princ 1)"))"#,
        ));
        assert_eq!(failures[0].actual, "no fix");
    }

    #[test]
    fn a_test_input_that_does_not_parse_fails_its_matches_clause() {
        // Rather than panicking: a rule file is user input, and an unbalanced
        // test string is a mistake to report, not a crash.
        let failures = run_tests(&ruleset(
            r#"(defrule r :pattern (f ?x) :message "m")
               (deftest r (:matches "(f 1"))"#,
        ));
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].clause, ":matches");
    }
}
