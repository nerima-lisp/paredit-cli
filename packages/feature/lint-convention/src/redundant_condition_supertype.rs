//! `redundant-condition-supertype`: a `define-condition` whose direct
//! supertype list names both a type and one of that type's own ancestors.
//!
//! `(define-condition my-error (file-error stream-error error) ())` lists
//! `error` alongside `file-error`, but `file-error` is itself a subtype of
//! `error` (in the same file) — so `error` contributes nothing `file-error`
//! did not already contribute. Writing it anyway reads as if the author meant
//! two independent branches of the hierarchy, when there is only one; the
//! redundant entry is either a leftover from a refactor or a sign the author
//! was not sure what `file-error` already inherits.
//!
//! Cycle detection — a condition that is (transitively) its own supertype —
//! is deliberately not this rule's job: `inspect class-cycles` already covers
//! `define-condition` alongside `defclass`/`defstruct`, sharing the same
//! superclass-edge extraction. Duplicating that check here would just be a
//! second place for the two to disagree.
//!
//! Scope, by design: only same-file supertypes are resolved, the same
//! resolution boundary `inspect class-hierarchy` uses. A supertype defined in
//! another file (nearly every condition's `error` itself, from the standard
//! library) is not walked further and is never called redundant relative to
//! one this rule cannot see.
//!
//! Report-only: whether the redundant entry should be deleted, or whether it
//! documents an intended future divergence, is a judgement this tool does not
//! make.
//!
//! Scope: Common Lisp only.

use std::collections::{HashMap, HashSet};

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{list_head, symbol_in, unqualified};

pub const META: RuleMeta = RuleMeta::new(
    "redundant-condition-supertype",
    RuleCategory::Conditions,
    Severity::Warning,
    "a define-condition whose direct supertype list names a type and one of that type's own \
     same-file ancestors",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Listing a type and one of its own ancestors as direct supertypes of the same condition \
         contributes nothing the more specific type did not already contribute; it usually means \
         the author was not sure what the more specific type already inherits.",
    )
    .with_example(
        "(define-condition my-error (file-error error) ())",
        "(define-condition my-error (file-error) ())",
    )
    .with_caveat(
        "Only same-file supertypes are resolved, the same boundary `inspect class-hierarchy` \
         uses. Cycles are `inspect class-cycles`'s job, not this rule's.",
    ),
);

/// One redundant entry in a `define-condition`'s direct supertype list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedundantSupertypeItem {
    pub span: ByteSpan,
    pub condition: String,
    /// The supertype that is redundant …
    pub redundant: String,
    /// … because this other listed supertype already implies it.
    pub implied_by: String,
}

/// Every `define-condition`'s name and direct supertype list, read off the
/// document in source order.
fn conditions(root: &ExpressionView) -> Vec<(String, Vec<String>, ByteSpan)> {
    let mut found = Vec::new();
    for form in &root.children {
        let Some(head) = list_head(form) else {
            continue;
        };
        if !symbol_in(head, &["define-condition"]) {
            continue;
        }
        let Some(name) = form.children.get(1).and_then(atom_symbol_text) else {
            continue;
        };
        let supertypes = form
            .children
            .get(2)
            .map(|list| {
                list.children
                    .iter()
                    .filter_map(atom_symbol_text)
                    .map(|name| unqualified(name).to_ascii_lowercase())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        found.push((
            unqualified(name).to_ascii_lowercase(),
            supertypes,
            form.span,
        ));
    }
    found
}

/// Whether `ancestor` is a same-file, transitive supertype of `start` — never
/// counting `start` itself as its own ancestor, and never looping forever on
/// a cycle (`inspect class-cycles`'s subject, not this rule's).
fn is_same_file_ancestor(
    start: &str,
    ancestor: &str,
    direct: &HashMap<String, Vec<String>>,
) -> bool {
    let mut visited = HashSet::new();
    let mut frontier = match direct.get(start) {
        Some(supertypes) => supertypes.clone(),
        None => return false,
    };
    while let Some(candidate) = frontier.pop() {
        if candidate == ancestor {
            return true;
        }
        if !visited.insert(candidate.clone()) {
            continue;
        }
        if let Some(supertypes) = direct.get(&candidate) {
            frontier.extend(supertypes.iter().cloned());
        }
    }
    false
}

/// Every redundant supertype entry across every `define-condition` in `root`.
#[must_use]
pub fn collect(root: &ExpressionView) -> Vec<RedundantSupertypeItem> {
    let entries = conditions(root);
    let direct: HashMap<String, Vec<String>> = entries
        .iter()
        .map(|(name, supertypes, _)| (name.clone(), supertypes.clone()))
        .collect();

    let mut found = Vec::new();
    for (name, supertypes, span) in &entries {
        for redundant in supertypes {
            for implied_by in supertypes {
                if redundant == implied_by {
                    continue;
                }
                if is_same_file_ancestor(implied_by, redundant, &direct) {
                    found.push(RedundantSupertypeItem {
                        span: *span,
                        condition: name.clone(),
                        redundant: redundant.clone(),
                        implied_by: implied_by.clone(),
                    });
                }
            }
        }
    }
    found
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // Judging one condition's supertype list needs every other
        // `define-condition` in the file, to resolve same-file ancestry.
        HeadFilter::WholeTree
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for item in collect(view) {
            sink.report(
                item.span,
                format!(
                    "{}'s supertype {} is redundant: {} already inherits from it in this file",
                    item.condition, item.redundant, item.implied_by
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
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn findings(input: &str) -> Vec<RedundantSupertypeItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect(&tree.root_view())
    }

    #[test]
    fn flags_a_supertype_already_implied_by_a_sibling_supertype() {
        let found = findings(
            "(define-condition file-error (error) ())\n\
             (define-condition my-error (file-error error) ())",
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].condition, "my-error");
        assert_eq!(found[0].redundant, "error");
        assert_eq!(found[0].implied_by, "file-error");
    }

    #[test]
    fn flags_a_transitively_implied_supertype() {
        let found = findings(
            "(define-condition a (error) ())\n\
             (define-condition b (a) ())\n\
             (define-condition c (b a) ())",
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].condition, "c");
        assert_eq!(found[0].redundant, "a");
        assert_eq!(found[0].implied_by, "b");
    }

    #[test]
    fn does_not_flag_two_independent_supertypes() {
        assert!(
            findings(
                "(define-condition mixin-a (error) ())\n\
                 (define-condition mixin-b (error) ())\n\
                 (define-condition combined (mixin-a mixin-b) ())"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_supertype_defined_in_another_file() {
        // `error` and `file-error` are both unresolved here: nothing in this
        // file says `file-error` is a subtype of `error`, so neither is
        // reported redundant relative to the other.
        assert!(findings("(define-condition my-error (file-error error) ())").is_empty());
    }

    #[test]
    fn a_single_supertype_is_never_redundant_against_itself() {
        assert!(findings("(define-condition my-error (error) ())").is_empty());
    }
}
