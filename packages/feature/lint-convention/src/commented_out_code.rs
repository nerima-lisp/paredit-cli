//! `commented-out-code`: a comment whose body looks like a disabled
//! s-expression rather than prose.
//!
//! `;; (old-helper x y)` is a form someone chose not to delete. Unlike a
//! `TODO`/`FIXME` marker (`inspect todo`'s subject), it carries no reason and
//! no attribution — just code nobody runs, silently rotting out of sync with
//! whatever replaced it, and confusing the next reader about which version is
//! current.
//!
//! Comments are trivia beside the tree rather than nodes in it (the same fact
//! `inspect todo` documents), so this rule reads [`SyntaxTree::comments`]
//! directly rather than walking forms — the same source `inspect todo` reads,
//! for a different shape.
//!
//! Distinguishing code from prose is a heuristic, not a parse: a comment
//! counts as disabled code only if, once the leading `;`s are stripped, its
//! *entire* body is one balanced, parenthesized expression, **and** the first
//! word inside the parentheses is not one of a short list of English words
//! that start a parenthetical remark (`see`, `note`, `this`, `also`, …).
//! `;; (see the documentation above)` and `;; (this needs fixing)` are prose
//! parentheticals and are not reported; `;; (old-helper x y)` and
//! `;; (defun f (x) (+ x 1))` are.
//!
//! Report-only: deleting the comment (version control already remembers it)
//! is the obvious fix, but that is an edit this rule does not make for you.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};

pub const META: RuleMeta = RuleMeta::new(
    "commented-out-code",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a comment whose entire body is a balanced s-expression rather than prose",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A comment that is a whole disabled form carries no reason and no attribution, unlike a \
         TODO/FIXME marker. It is code nobody runs, drifting silently out of sync with whatever \
         replaced it. Version control already remembers deleted code, so the comment adds nothing \
         a `git log` would not.",
    )
    .with_example(
        ";; (defun old-helper (x) (+ x 1))",
        "(defun new-helper (x) (+ x 2)) ; old-helper replaced by new-helper, see #123",
    )
    .with_caveat(
        "A prose parenthetical — `(see the documentation above)`, `(this needs fixing)` — is not \
         reported: the first word inside the parentheses must not be one of a short list of \
         English words that start a remark rather than a form.",
    ),
);

/// Words that mark a parenthetical as prose rather than code, when they are
/// the first word inside the parentheses.
const PROSE_LEAD_WORDS: [&str; 14] = [
    "see", "note", "this", "that", "also", "above", "below", "the", "a", "an", "is", "are", "not",
    "todo",
];

/// One comment that looks like a disabled form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentedOutCodeItem {
    pub span: ByteSpan,
    /// The comment's body, semicolons and surrounding whitespace stripped.
    pub body: String,
}

/// Whether `body` (semicolons and surrounding whitespace already stripped) is
/// shaped like a disabled form rather than a prose parenthetical.
#[must_use]
pub fn looks_like_disabled_form(body: &str) -> bool {
    let trimmed = body.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return false;
    }

    // The whole body must be exactly one balanced expression — not a
    // parenthetical sitting inside a longer sentence, and not two forms back
    // to back (which this rule leaves for a human to judge one at a time).
    let mut depth: i32 = 0;
    for (index, character) in trimmed.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && index != trimmed.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    if depth != 0 {
        return false;
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let Some(first_word) = inner.split_whitespace().next() else {
        // `()` alone is not a meaningful disabled form.
        return false;
    };
    let normalized = first_word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
    !PROSE_LEAD_WORDS.contains(&normalized.to_ascii_lowercase().as_str())
}

/// Every comment in `tree` that looks like a disabled form.
#[must_use]
pub fn collect(tree: &SyntaxTree) -> Vec<CommentedOutCodeItem> {
    tree.comments()
        .filter_map(|comment| {
            let body = comment.text().trim_start_matches(';').trim();
            looks_like_disabled_form(body).then(|| CommentedOutCodeItem {
                span: comment.span(),
                body: body.to_owned(),
            })
        })
        .collect()
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // Comments are trivia, not tree nodes; this rule reads
        // `context.tree().comments()` directly and needs the per-node
        // dispatch to run exactly once, on the whole document.
        HeadFilter::WholeTree
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        _view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for item in collect(context.tree()) {
            sink.report(
                item.span,
                format!("this comment looks like disabled code: {}", item.body),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn findings(input: &str) -> Vec<CommentedOutCodeItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect(&tree)
    }

    #[test]
    fn flags_a_disabled_call() {
        let found = findings("(defun f (x) x)\n;; (old-helper x y)\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].body, "(old-helper x y)");
    }

    #[test]
    fn flags_a_disabled_definition() {
        let found = findings(";; (defun old-helper (x) (+ x 1))\n(defun f (x) x)");
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn does_not_flag_ordinary_prose() {
        assert!(findings(";; This computes the total.\n(defun f (x) x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_prose_parenthetical() {
        assert!(findings(";; (see the documentation above)\n(defun f (x) x)").is_empty());
        assert!(findings(";; (this needs fixing)\n(defun f (x) x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_comment_with_a_parenthetical_inside_a_longer_sentence() {
        assert!(findings(";; Historically (before v2) this used a hash table.").is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_parenthetical() {
        assert!(findings(";; ()\n(defun f (x) x)").is_empty());
    }

    #[test]
    fn a_triple_semicolon_comment_is_read_the_same_way() {
        let found = findings(";;; (old-helper x y)\n(defun f (x) x)");
        assert_eq!(found.len(), 1, "{found:?}");
    }
}
