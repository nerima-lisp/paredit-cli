//! `char-code-limit-loop-assumption`: walking the character set as if it had
//! 256 codes.
//!
//! `(dotimes (i 256) … (code-char i) …)` iterates the characters `0` through
//! `255` and stops. CLHS defines `char-code-limit` as "the upper exclusive
//! bound on the value returned by the function `char-code`", "a non-negative
//! integer, the exact magnitude of which is *implementation-dependent*". On
//! SBCL and CLISP it is 1114112. A loop that hard-codes 256 therefore covers
//! the whole character repertoire on exactly the implementations where it is
//! 256, and a fraction of a percent of it everywhere else — with no error, no
//! warning, and a table that is silently short.
//!
//! The portable spelling of "every character" is `char-code-limit` itself.
//!
//! # Trigger
//!
//! Both halves are required, and neither alone is reported:
//!
//! 1. The loop's upper bound is the literal `256` exclusive (`(dotimes (i
//!    256) …)`, `(loop for i from 0 below 256 …)`) or `255` inclusive (`(loop
//!    for i from 0 to 255 …)`).
//! 2. The loop body applies `code-char` **to the loop variable**.
//!
//! Requirement 2 is what separates this from an ordinary 256-iteration loop
//! over a byte buffer, which is not a character-set claim at all and is not
//! reported.
//!
//! # Boundary with `ascii-code-char`
//!
//! [`crate::ascii_code_char`] also fires on `code-char`, and the two cannot
//! overlap: that rule requires a *literal integer* argument in 32..=126 and
//! rewrites it to a character literal, while this rule requires the argument to
//! be the loop *variable* — a symbol, which never parses as an integer. So
//! `(dotimes (i 100) (code-char 65))` is `ascii-code-char`'s and not this
//! rule's, and `(dotimes (i 256) (code-char i))` is this rule's and not that
//! one's.
//!
//! # Limits, deliberately
//!
//! - A bound reached indirectly — `(dotimes (i +byte-max+) …)`, `(dotimes (i
//!   (length table)) …)` — is not reported. The rule reads literals and does
//!   not resolve constants.
//! - A *deliberate* single-byte table built this way is reported too; nothing
//!   syntactic distinguishes "I meant every character" from "I meant every
//!   byte". The finding is still true of the code as written — `code-char` of
//!   200 names no particular character in any portable sense — which is why
//!   this is a warning and not a fix.
//!
//! # Cost
//!
//! `loop` and `dotimes` are dense in ordinary code, so this rule's `check` runs
//! often and must reject fast. It does: the first thing either branch does is
//! compare *direct children's* atom text against two string constants — a
//! pointer dereference and a `str` compare, no allocation, no `symbol_is`
//! parsing, no descent. Only a form that literally spells `255` or `256` at its
//! top level pays for anything further, and only one that also names a loop
//! variable walks its own subtree. The `clean/forms/*` benchmarks lint files
//! with zero findings, so that first compare is all this rule costs there.
//!
//! `mentions_a_byte_wide_bound` is a *performance* device and not a
//! correctness guard: `read_loop` independently rejects every form it
//! rejects, so deleting the call site changes no finding — which a mutation run
//! confirmed. It is pinned by a direct assertion on the predicate rather than
//! through the rule's output, because there is no output to observe.
//!
//! Measured on a zero-finding corpus of 3000 definitions carrying four
//! `loop`/`dotimes` forms each: 12000 invocations in 260-272µs, or 0.022µs per
//! invocation — the cheapest per invocation of the three rules this package
//! gained, despite having four times their invocation count.
//!
//! Report-only: replacing `256` with `char-code-limit` changes how many
//! iterations run and how large the table must be, which is the author's
//! decision about their data, not a mechanical rewrite.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head, symbol_is};

use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "char-code-limit-loop-assumption",
    RuleCategory::Portability,
    Severity::Warning,
    "a loop over character codes 0-255, which is the whole character set only where char-code-limit is 256",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`char-code-limit` is implementation-dependent — 1114112 on SBCL and CLISP — so a loop \
         bounded by a literal 256 that calls `code-char` on its index enumerates the entire \
         character repertoire on some implementations and a small prefix of it on others. \
         Bounding the loop by `char-code-limit` says what was meant.",
    )
    .with_example(
        "(dotimes (i 256) (vector-push (code-char i) table))",
        "(dotimes (i char-code-limit) (vector-push (code-char i) table))",
    )
    .with_caveat(
        "Both the literal bound and a `code-char` applied to the loop variable are required. An \
         ordinary 256-iteration loop over a byte buffer makes no claim about the character set \
         and is not reported, and neither is a bound written as a named constant.",
    ),
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("dotimes"), NormalizedHead::new("loop")];

/// The two literal bounds that mean "codes 0 through 255": exclusive `256` and
/// inclusive `255`.
const EXCLUSIVE_BOUND: &str = "256";
const INCLUSIVE_BOUND: &str = "255";

/// One loop that walks 0..=255 calling `code-char`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteWideCharLoop {
    pub span: ByteSpan,
    /// The loop variable, for the message.
    pub variable: String,
}

/// Whether any *direct* child of `view` is written `255` or `256`.
///
/// `loop`'s early rejection. A `loop`'s clauses are its direct children, so a
/// form that does not spell the bound among them cannot be bounded by it.
/// Reads `text` straight off each child and compares it to two constants; it
/// does not allocate, does not descend, and does not go through `symbol_is`'s
/// package-qualifier split.
///
/// `dotimes` needs no such scan: its count form is at one fixed position, so
/// [`read_dotimes`] rejects in constant time without looking at anything else.
fn mentions_a_byte_wide_bound(view: &ExpressionView) -> bool {
    view.children.iter().any(|child| {
        matches!(
            child.text.as_deref(),
            Some(EXCLUSIVE_BOUND | INCLUSIVE_BOUND)
        )
    })
}

/// `(dotimes (var 256) …)` read as its variable.
///
/// `dotimes`' count form is exclusive, so only `256` covers 0..=255.
fn read_dotimes(view: &ExpressionView) -> Option<&str> {
    let binding = view.children.get(1)?;
    if !is_paren_list(binding) {
        return None;
    }
    let variable = atom_text(binding.children.first()?)?;
    let count = atom_text(binding.children.get(1)?)?;
    (count == EXCLUSIVE_BOUND).then_some(variable)
}

/// Whether `terminator`/`limit` together bound an arithmetic clause at 255
/// inclusive.
fn is_byte_wide_limit(terminator: &str, limit: &str) -> bool {
    if symbol_is(terminator, "below") {
        return limit == EXCLUSIVE_BOUND;
    }
    if symbol_is(terminator, "to") || symbol_is(terminator, "upto") {
        return limit == INCLUSIVE_BOUND;
    }
    false
}

/// `(loop for var … below 256 …)` read as its variable.
///
/// `loop` clauses are direct children of the form, so one pass over them finds
/// the `for`/`as` that introduces a variable and the arithmetic terminator that
/// bounds it. The scan for a terminator stops at the next `for`/`as`, so a
/// bound belonging to a *different* variable's clause is not attributed here.
fn read_loop(view: &ExpressionView) -> Option<&str> {
    let children = &view.children;
    let introduces = |index: usize| {
        atom_text(&children[index])
            .is_some_and(|word| symbol_is(word, "for") || symbol_is(word, "as"))
    };
    for index in 1..children.len() {
        if !introduces(index) {
            continue;
        }
        let Some(variable) = children.get(index + 1).and_then(atom_text) else {
            continue;
        };
        let mut cursor = index + 2;
        while cursor + 1 < children.len() {
            let Some(token) = atom_text(&children[cursor]) else {
                break;
            };
            if introduces(cursor) {
                break;
            }
            // Written as a `match` rather than a `let` chain: the workspace's
            // 1.85 MSRV does not have them, and edition 2024 makes them look
            // available until the `msrv` check says otherwise.
            match atom_text(&children[cursor + 1]) {
                Some(limit) if is_byte_wide_limit(token, limit) => return Some(variable),
                _ => {}
            }
            cursor += 1;
        }
    }
    None
}

/// Whether `(code-char <variable>)` appears anywhere under `view`.
///
/// Iterative so a deeply nested loop body cannot overflow the stack, and
/// bounded by the matched form's own subtree — never the file.
fn applies_code_char_to(view: &ExpressionView, variable: &str) -> bool {
    let mut stack = vec![view];
    while let Some(node) = stack.pop() {
        if list_head(node).is_some_and(|head| symbol_is(head, "code-char"))
            && node.children.len() == 2
            && atom_text(&node.children[1]).is_some_and(|argument| symbol_is(argument, variable))
        {
            return true;
        }
        stack.extend(node.children.iter());
    }
    false
}

/// Reads one loop and reports the character-set assumption it makes.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<ByteWideCharLoop> {
    let head = list_head(view)?;
    // Each branch rejects on the literal bound before doing anything else. See
    // the module's `Cost` section.
    let variable = if symbol_is(head, "dotimes") {
        // Constant time: the count form is at one fixed position.
        read_dotimes(view)?
    } else if symbol_is(head, "loop") {
        if !mentions_a_byte_wide_bound(view) {
            return None;
        }
        read_loop(view)?
    } else {
        return None;
    };
    // A variable named by an empty atom would make `symbol_is` match anything.
    if variable.is_empty() {
        return None;
    }
    applies_code_char_to(view, variable).then(|| ByteWideCharLoop {
        span: view.span,
        variable: variable.to_owned(),
    })
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
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(found) = examine(view) else {
            return Ok(());
        };
        // Asked only once a finding already exists, never per visited node.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(
            found.span,
            format!(
                "this walks (code-char {}) over the codes 0-255, which is the whole character set \
                 only where char-code-limit is 256; it is implementation-dependent, and 1114112 \
                 on SBCL and CLISP — bound the loop by char-code-limit",
                found.variable
            ),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn variable(input: &str) -> Option<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.variable)
    }

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    /// How many findings the *real* dispatch produces, which is the only thing
    /// that exercises the quote guard, the head filter, and the dialect scope.
    fn reports(input: &str) -> usize {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect_lint_outcomes(
            catalog,
            &index,
            std::path::Path::new("t.lisp"),
            Dialect::CommonLisp,
            &tree,
            input,
            RuleSelection::All,
        )
        .expect("lint pass")
        .len()
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_dotimes_over_256_codes() {
        assert_eq!(
            variable("(dotimes (i 256) (vector-push (code-char i) table))"),
            Some("i".to_owned())
        );
        assert_eq!(
            reports("(dotimes (i 256) (vector-push (code-char i) table))"),
            1
        );
    }

    #[test]
    fn flags_a_loop_below_256() {
        assert_eq!(
            variable("(loop for c from 0 below 256 collect (code-char c))"),
            Some("c".to_owned())
        );
    }

    #[test]
    fn flags_a_loop_to_255() {
        assert_eq!(
            variable("(loop for c from 0 to 255 collect (code-char c))"),
            Some("c".to_owned())
        );
        assert_eq!(
            variable("(loop for c from 0 upto 255 do (print (code-char c)))"),
            Some("c".to_owned())
        );
    }

    #[test]
    fn finds_a_code_char_nested_deep_in_the_body() {
        assert_eq!(
            variable("(dotimes (i 256) (when (f i) (let ((c (code-char i))) (g c))))"),
            Some("i".to_owned())
        );
    }

    #[test]
    fn reads_the_heads_and_the_call_case_insensitively() {
        assert_eq!(
            variable("(DOTIMES (I 256) (CODE-CHAR I))"),
            Some("I".to_owned())
        );
        assert_eq!(
            variable("(dotimes (i 256) (cl:code-char i))"),
            Some("i".to_owned())
        );
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_byte_loop_that_names_no_character() {
        // The bound is there; the character-set claim is not.
        assert_eq!(variable("(dotimes (i 256) (setf (aref buffer i) 0))"), None);
        assert_eq!(
            variable("(loop for i from 0 below 256 sum (aref buffer i))"),
            None
        );
    }

    #[test]
    fn does_not_flag_a_code_char_loop_with_a_portable_bound() {
        assert_eq!(
            variable("(dotimes (i char-code-limit) (code-char i))"),
            None
        );
        assert_eq!(
            variable("(loop for c from 0 below char-code-limit collect (code-char c))"),
            None
        );
    }

    #[test]
    fn does_not_flag_a_different_literal_bound() {
        assert_eq!(variable("(dotimes (i 128) (code-char i))"), None);
        assert_eq!(variable("(dotimes (i 1114112) (code-char i))"), None);
        assert_eq!(
            variable("(loop for c from 0 below 128 collect (code-char c))"),
            None
        );
    }

    #[test]
    fn does_not_confuse_an_inclusive_bound_with_an_exclusive_one() {
        // `below 255` stops at 254 and `to 256` reaches 256; neither is the
        // 0..=255 span the rule is about.
        assert_eq!(
            variable("(loop for c from 0 below 255 collect (code-char c))"),
            None
        );
        assert_eq!(
            variable("(loop for c from 0 to 256 collect (code-char c))"),
            None
        );
        // `dotimes` is exclusive, so a literal 255 is 0..=254.
        assert_eq!(variable("(dotimes (i 255) (code-char i))"), None);
    }

    #[test]
    fn does_not_flag_a_code_char_of_something_other_than_the_loop_variable() {
        assert_eq!(variable("(dotimes (i 256) (code-char base))"), None);
        assert_eq!(variable("(dotimes (i 256) (code-char (+ i 1)))"), None);
    }

    #[test]
    fn does_not_attribute_a_bound_to_another_clauses_variable() {
        // `j` is the one bounded at 255; `i` is not, and it is `i` that
        // `code-char` is applied to.
        assert_eq!(
            variable("(loop for i from 0 below n for j from 0 to 255 do (code-char i))"),
            None
        );
    }

    #[test]
    fn does_not_flag_a_malformed_loop() {
        assert_eq!(variable("(dotimes)"), None);
        assert_eq!(variable("(dotimes (i))"), None);
        assert_eq!(variable("(dotimes i 256)"), None);
        assert_eq!(variable("(loop for)"), None);
    }

    /// The early rejection is the hot path, so it gets its own test: a form
    /// with no `255`/`256` among its direct children never gets past it.
    /// The early rejections are the hot path, so they get their own test.
    #[test]
    fn rejects_a_loop_that_does_not_spell_the_bound_among_its_clauses() {
        let form = SyntaxTree::parse_with_dialect(
            "(loop for x in xs do (code-char x))",
            Dialect::CommonLisp,
        )
        .expect("parse")
        .select_path(&Path::root_child(0))
        .expect("root form")
        .view();
        assert!(!mentions_a_byte_wide_bound(&form));
        assert_eq!(variable("(loop for x in xs do (code-char x))"), None);
        // A `256` nested inside a clause is not a direct child, so the loop is
        // rejected — a deliberate false negative the module documents.
        assert_eq!(
            variable("(loop for c from 0 below (identity 256) collect (code-char c))"),
            None
        );
        // The same deliberate false negative on `dotimes`: the count form has
        // to *be* the literal, not compute it.
        assert_eq!(variable("(dotimes (i (f 256)) (code-char i))"), None);
    }

    // -- quote-context negative ----------------------------------------------

    #[test]
    fn does_not_flag_a_loop_in_quoted_data() {
        assert_eq!(reports("'(dotimes (i 256) (code-char i))"), 0);
        assert_eq!(reports("(quote (dotimes (i 256) (code-char i)))"), 0);
        assert_eq!(reports("`(dotimes (i 256) (code-char i))"), 0);
        assert_eq!(reports("'(a ,(dotimes (i 256) (code-char i)))"), 0);
        assert_eq!(reports("'(outer (dotimes (i 256) (code-char i)))"), 0);
    }

    #[test]
    fn flags_a_loop_unquoted_back_into_code() {
        assert_eq!(reports("`(a ,(dotimes (i 256) (code-char i)))"), 1);
    }

    // -- string-literal negative ---------------------------------------------

    #[test]
    fn does_not_flag_a_loop_written_inside_a_string() {
        assert_eq!(
            reports(r#"(format nil "(dotimes (i 256) (code-char i))")"#),
            0
        );
        assert_eq!(
            reports(r#"(defun f () "walks (dotimes (i 256) (code-char i))" nil)"#),
            0
        );
    }
}
