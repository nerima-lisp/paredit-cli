//! `unreachable-handler-clause`: a handler shadowed by a broader one above it.
//!
//! `handler-case` picks the *first* clause whose type the condition belongs to,
//! so a clause on `error` placed before one on `file-error` makes the second
//! dead. The narrow handler that was written on purpose never runs, and nothing
//! about the form says so.
//!
//! Deciding this needs a subtype relation, and the rule carries only the part
//! of the standard hierarchy that is both fixed and commonly used. That is a
//! deliberate limit: a project's own condition classes have a hierarchy this
//! layer cannot see, so a clause on a user-defined type is never claimed to be
//! shadowed. Under-reporting is the only safe direction — telling someone their
//! carefully written handler is dead, wrongly, is worse than silence.
//!
//! Report-only: the repair is to reorder the clauses, and which order was
//! intended is not something the form records.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, unqualified};

pub const META: RuleMeta = RuleMeta::new(
    "unreachable-handler-clause",
    RuleCategory::Conditions,
    Severity::Error,
    "a handler-case clause whose condition type an earlier clause already covers, so it never runs",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`handler-case` selects the first clause whose type the condition is of. A clause on a \
         supertype therefore shadows every later clause on one of its subtypes, and the narrower \
         handler — which is the one somebody thought about — never runs.",
    )
    .with_example(
        "(handler-case (f) (error (c) (generic c)) (file-error (c) (retry c)))",
        "(handler-case (f) (file-error (c) (retry c)) (error (c) (generic c)))",
    )
    .with_caveat(
        "Only the standard condition hierarchy is known here. A clause on a project's own \
         condition class is never claimed to be shadowed, because this layer cannot see what it \
         inherits from.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("handler-case")];

/// The part of the standard condition hierarchy that is fixed by the standard
/// and appears in real handler lists, as `(subtype, supertype)` edges.
///
/// Transitivity is computed rather than listed, so adding one edge extends
/// every chain through it.
const HIERARCHY: [(&str, &str); 17] = [
    ("warning", "condition"),
    ("serious-condition", "condition"),
    ("error", "serious-condition"),
    ("storage-condition", "serious-condition"),
    ("simple-error", "error"),
    ("arithmetic-error", "error"),
    ("cell-error", "error"),
    ("control-error", "error"),
    ("file-error", "error"),
    ("package-error", "error"),
    ("parse-error", "error"),
    ("print-not-readable", "error"),
    ("program-error", "error"),
    ("stream-error", "error"),
    ("type-error", "error"),
    ("division-by-zero", "arithmetic-error"),
    ("floating-point-overflow", "arithmetic-error"),
];

/// Whether `subtype` is `supertype`, or inherits from it.
///
/// `t` names every type, so it covers everything. An unknown name — a project's
/// own class — is only ever equal to itself: nothing is claimed about what it
/// inherits.
#[must_use]
pub fn covers(supertype: &str, subtype: &str) -> bool {
    let supertype = unqualified(supertype).to_ascii_lowercase();
    let subtype = unqualified(subtype).to_ascii_lowercase();
    if supertype == "t" || supertype == subtype {
        return true;
    }
    let mut current = subtype;
    // The hierarchy is a tree of depth four, so the walk terminates; the bound
    // is a backstop against a future cycle in the table, not an expected limit.
    for _ in 0..HIERARCHY.len() {
        let Some((_, parent)) = HIERARCHY.iter().find(|(child, _)| *child == current) else {
            return false;
        };
        if *parent == supertype {
            return true;
        }
        current = (*parent).to_owned();
    }
    false
}

/// One shadowed clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowedClause {
    pub span: ByteSpan,
    pub shadowed: String,
    pub by: String,
}

/// Every shadowed clause in one `handler-case`.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<ShadowedClause> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    if !symbol_in(head, &["handler-case"]) {
        return Vec::new();
    }

    let mut seen: Vec<&str> = Vec::new();
    let mut found = Vec::new();
    for clause in view.children.iter().skip(2) {
        let Some(condition) = clause.children.first().and_then(atom_text) else {
            continue;
        };
        // `:no-error` is not a condition type; it is a separate clause kind.
        if condition.starts_with(':') {
            continue;
        }
        if let Some(earlier) = seen.iter().find(|earlier| covers(earlier, condition)) {
            found.push(ShadowedClause {
                span: clause.span,
                shadowed: condition.to_owned(),
                by: (*earlier).to_owned(),
            });
        }
        seen.push(condition);
    }
    found
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for clause in examine(view) {
            sink.report(
                clause.span,
                format!(
                    "the {} clause above already covers {}, so this clause never runs; \
                     put the narrower type first",
                    clause.by, clause.shadowed
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn shadowed(input: &str) -> Vec<(String, String)> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view)
            .into_iter()
            .map(|item| (item.shadowed, item.by))
            .collect()
    }

    #[test]
    fn reads_the_hierarchy_transitively() {
        assert!(covers("error", "file-error"));
        assert!(covers("condition", "file-error"));
        assert!(covers("serious-condition", "division-by-zero"));
        assert!(covers("t", "anything-at-all"));
        assert!(covers("file-error", "file-error"));
    }

    #[test]
    fn does_not_invent_a_relation_for_an_unknown_type() {
        assert!(!covers("my-app-error", "file-error"));
        assert!(!covers("error", "my-app-error"));
        assert!(!covers("file-error", "error"));
    }

    #[test]
    fn flags_a_narrow_clause_after_a_broad_one() {
        assert_eq!(
            shadowed("(handler-case (f) (error (c) (a c)) (file-error (c) (b c)))"),
            vec![("file-error".to_owned(), "error".to_owned())]
        );
    }

    #[test]
    fn flags_a_transitively_shadowed_clause() {
        assert_eq!(
            shadowed("(handler-case (f) (condition (c) (a c)) (division-by-zero (c) (b c)))"),
            vec![("division-by-zero".to_owned(), "condition".to_owned())]
        );
    }

    #[test]
    fn does_not_flag_the_correct_order() {
        assert!(shadowed("(handler-case (f) (file-error (c) (b c)) (error (c) (a c)))").is_empty());
    }

    #[test]
    fn does_not_flag_two_unrelated_types() {
        assert!(
            shadowed("(handler-case (f) (file-error (c) (a c)) (type-error (c) (b c)))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_project_condition_class() {
        assert!(
            shadowed("(handler-case (f) (my-error (c) (a c)) (my-other-error (c) (b c)))")
                .is_empty()
        );
    }

    #[test]
    fn a_repeated_type_shadows_itself() {
        assert_eq!(
            shadowed("(handler-case (f) (error (c) (a c)) (error (c) (b c)))"),
            vec![("error".to_owned(), "error".to_owned())]
        );
    }

    #[test]
    fn a_no_error_clause_is_not_a_condition_type() {
        assert!(shadowed("(handler-case (f) (error (c) (a c)) (:no-error (v) v))").is_empty());
    }
}
