//! `repeated-hash-table-lookup-same-key`: asking one table for one key twice.
//!
//! ```lisp
//! (defun limit-for (request)
//!   (if (gethash :rate-limit *policy*)
//!       (min (gethash :rate-limit *policy*) (request-budget request))
//!       (request-budget request)))
//! ```
//!
//! The second `gethash` hashes the same key, probes the same table, and cannot
//! return anything the first did not. `(let ((limit (gethash :rate-limit
//! *policy*))) …)`, or `multiple-value-bind` when the presence flag is wanted,
//! does the work once.
//!
//! # The three guards, and why each one exists
//!
//! Repeated-lookup detection is a common-subexpression claim, and a
//! common-subexpression claim is wrong the moment anything between the two
//! occurrences can change the answer. Three things can, and each has a guard
//! that is deliberately blunt in the direction of silence.
//!
//! - **Only one of them may run.** `(if p (gethash :a h) (gethash :a h))` is not
//!   a repeat: exactly one arm executes. Every reported pair is checked with
//!   `branches_are_exclusive`, which reads the conditional arms each lookup
//!   sits under. A lookup in an `if`'s *test* and one in its `then` arm are
//!   still a pair, because whenever the arm runs the test ran.
//!
//! - **The table may be written.** Rather than trying to decide whether a write
//!   falls *between* two reads — which needs an execution order this rule does
//!   not have — the table is disqualified outright if its name appears anywhere
//!   in the body other than as the table argument of a plain `gethash` read. A
//!   `(setf (gethash …) …)`, a `remhash`, a `clrhash`, and a bare `(update-cache
//!   h)` whose callee might do any of those all fail that test identically, and
//!   no callee has to be resolved to know it.
//!
//! - **The key may be a different key.** Only a *literal* key is grouped: a
//!   keyword, a quoted symbol, a string, a character, or a number. `(gethash key
//!   h)` twice is not reported, because `key` may have been rebound in between
//!   and this rule does not read the binding table.
//!
//! The middle guard is what limits the rule's reach, and the limit is worth
//! stating: it fires for a table that arrives as a parameter or lives in a
//! global, and never for one bound by a `let` in the same body — the binding
//! itself is an occurrence of the name, so the table is disqualified. That is a
//! false negative accepted on purpose. Recognizing binding positions means
//! modelling shadowing, and a rule that gets shadowing wrong reports a lookup of
//! a table that is not the table it thinks it is.
//!
//! # Relationship to `check-then-act`
//!
//! `lint-safety`'s `check-then-act` reports the complementary shape — a shared
//! place read in a test and *written* in the branch — and classifies it as a
//! concurrency defect. The two are disjoint by construction: any write to the
//! table disqualifies it here, which is exactly the precondition
//! `check-then-act` requires.
//!
//! # Cost
//!
//! Anchored on `defun`/`defmethod`, so the body walk is bounded by the matched
//! definition and the file is walked once in total rather than once per lookup —
//! the O(N²) shape that two rules in this repository shipped with and that
//! together accounted for 98% of a lint run.
//!
//! Ahead of the walk is a byte scan of the definition's own source for the
//! spelling `gethash`. A definition that does not contain one cannot contain a
//! repeated lookup, and pays a scan of its bytes instead of a walk of its nodes
//! — which is what the `clean/forms/*` benchmarks, linting files with no
//! findings, actually measure.
//!
//! Report-only. Where the binding goes, and whether the presence flag is wanted
//! alongside the value, is a decision about the program.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleTag,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head};

use crate::shared::{symbol_is, unqualified};
use crate::support::{
    BranchStep, branches_are_exclusive, for_each_evaluated_subview_with_branches, is_unevaluated_at,
};

pub const META: RuleMeta = RuleMeta::new(
    "repeated-hash-table-lookup-same-key",
    RuleCategory::Performance,
    Severity::Warning,
    "two or more gethash reads of one literal key on one table in a body that never writes it",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Pedantic])
.with_explanation(
    RuleExplanation::new(
        "A second `gethash` for a key already looked up hashes the key again and probes the table \
         again to return what the first call returned. Binding the result once replaces n probes \
         with one, and `multiple-value-bind` keeps the presence flag if the code needs it.",
    )
    .with_example(
        "(if (gethash :limit *policy*) (use (gethash :limit *policy*)) (default))",
        "(let ((limit (gethash :limit *policy*))) (if limit (use limit) (default)))",
    )
    .with_caveat(
        "Nothing is reported when the two lookups are in arms of the same conditional that cannot \
         both run, when the key is not a literal, or when the table's name appears anywhere in \
         the body other than as a `gethash` read — which covers `setf`, `remhash`, `clrhash`, and \
         any call that might do one of those inside. It is tagged `pedantic` because one extra \
         probe is cheap; the finding is about the repetition, not the probe.",
    ),
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmethod"),
];

/// How a mutating operator lays out the places it writes.
#[derive(Debug, Clone, Copy)]
enum Places {
    /// `(setf place value place value …)`: every other child from 1.
    Alternating,
    /// The child at this index, and no other.
    Only(usize),
    /// Every child from 1, as `rotatef` and `shiftf` do.
    All,
}

/// The operators whose named arguments are written rather than read.
///
/// The value positions of a `setf` are deliberately not places: `(setf value
/// (gethash :k h))` is a lookup, and treating it as a write would disqualify the
/// table on the single most common spelling of a lookup there is.
///
/// Read with one [`unqualified`] call rather than twelve — see [`ARM_HEADS`]'s
/// counterpart in [`crate::support`] for why that mattered enough to measure.
const MUTATORS: [(&str, Places); 12] = [
    ("setf", Places::Alternating),
    ("setq", Places::Alternating),
    ("psetf", Places::Alternating),
    ("psetq", Places::Alternating),
    ("incf", Places::Only(1)),
    ("decf", Places::Only(1)),
    ("pop", Places::Only(1)),
    ("remf", Places::Only(1)),
    ("push", Places::Only(2)),
    ("pushnew", Places::Only(2)),
    ("rotatef", Places::All),
    ("shiftf", Places::All),
];

/// The place positions of `name`, given how many children the call has.
fn write_place_positions(name: &str, arity: usize) -> Vec<usize> {
    let Some((_, places)) = MUTATORS
        .iter()
        .find(|(head, _)| head.eq_ignore_ascii_case(name))
    else {
        return Vec::new();
    };
    match places {
        Places::Alternating => (1..arity).step_by(2).collect(),
        Places::Only(index) => vec![*index],
        Places::All => (1..arity).collect(),
    }
}

/// A key this rule is willing to call "the same key", in a normalized spelling.
///
/// Literals only. A symbol naming a variable is not a key, because the variable
/// may hold something different at the second lookup and deciding that needs the
/// binding table.
///
/// Case matters for a string and not for a symbol, which is the reader's own
/// rule: `"A"` and `"a"` are different strings, `:a` and `:A` are one keyword.
fn literal_key(view: &ExpressionView) -> Option<String> {
    // `'foo` and `(quote foo)` both name the symbol `foo`.
    if view.reader_prefixes.contains(&ReaderPrefix::Quote) {
        return atom_text(view).map(|text| format!("'{}", unqualified(text).to_ascii_lowercase()));
    }
    if list_head(view).is_some_and(|head| symbol_is(head, "quote")) {
        return view
            .children
            .get(1)
            .and_then(atom_text)
            .map(|text| format!("'{}", unqualified(text).to_ascii_lowercase()));
    }
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_text(view)?;
    let mut bytes = text.bytes();
    let first = bytes.next()?;
    match first {
        // A keyword. A bare `:` is the empty-name keyword, not a literal worth
        // grouping on.
        b':' if text.len() > 1 => Some(text.to_ascii_lowercase()),
        // A string, whose quotes the reader keeps as part of the one atom.
        b'"' => Some(text.to_owned()),
        // A character literal, `#\a`.
        b'#' if text.starts_with("#\\") => Some(text.to_owned()),
        b'0'..=b'9' => Some(text.to_owned()),
        b'-' | b'+' => bytes
            .next()
            .filter(u8::is_ascii_digit)
            .map(|_| text.to_owned()),
        _ => None,
    }
}

/// One `gethash` read this rule can reason about.
#[derive(Debug, Clone)]
struct Read {
    span: ByteSpan,
    table: String,
    /// The key, when it is a literal — `None` for a computed key, which still
    /// counts as a read of the table but can never be grouped.
    key: Option<String>,
    /// The default argument's source text, so `(gethash :k h)` and `(gethash :k
    /// h 0)` are not treated as the same expression.
    default: String,
    path: Vec<BranchStep>,
}

/// Reads one `(gethash key table [default])` call, or `None` when it is not one
/// this rule can reason about.
///
/// A computed table expression is `None`: two calls to `(gethash :k (cache-of
/// x))` are not provably the same table, and nothing here resolves `cache-of`.
fn read_gethash(view: &ExpressionView, source: &str, path: &[BranchStep]) -> Option<Read> {
    if view.children.len() != 3 && view.children.len() != 4 {
        return None;
    }
    let table_view = view.children.get(2)?;
    if !table_view.reader_prefixes.is_empty() {
        return None;
    }
    let table_text = atom_text(table_view)?;
    // A keyword, a string, or a number in the table position is not a table.
    if table_text.starts_with(':')
        || table_text.starts_with('"')
        || !table_text.starts_with(|c: char| c.is_alphabetic() || "*+-<>=/_&%$@!?~^".contains(c))
    {
        return None;
    }
    let default = match view.children.get(3) {
        None => String::new(),
        // Only a literal or a symbol default. A computed default is evaluated on
        // every call and collapsing the two would change how often it runs.
        Some(value) if atom_text(value).is_some() => {
            source[value.span.start().get()..value.span.end().get()].to_owned()
        }
        Some(_) => return None,
    };
    Some(Read {
        span: view.span,
        table: unqualified(table_text).to_ascii_lowercase(),
        key: view.children.get(1).and_then(literal_key),
        default,
        path: path.to_vec(),
    })
}

/// The body forms of a definition, or `None` for a form whose shape this rule
/// does not claim to understand.
///
/// `(defun NAME LAMBDA-LIST BODY…)` is positional. `(defmethod NAME QUALIFIER*
/// (SPECIALIZED-LAMBDA-LIST) BODY…)` is not, so the lambda list is found as the
/// first `(…)` child at or after index 2 — which also reads
/// `(defmethod (setf slot) ((x thing)) …)`, whose *name* is a list.
///
/// The lambda list is deliberately outside the body. It is where a table
/// parameter is named, and counting that name would disqualify every table this
/// rule exists to report on.
fn definition_body(view: &ExpressionView) -> Option<&[ExpressionView]> {
    let head = list_head(view)?;
    if symbol_is(head, "defun") {
        return view.children.get(3..);
    }
    if symbol_is(head, "defmethod") {
        let offset = view.children.iter().skip(2).position(is_paren_list)?;
        return view.children.get(offset + 3..);
    }
    None
}

/// Whether the source contains the spelling `gethash` at all.
///
/// A byte scan, not a tree walk: no allocation, no per-node work, and it reads
/// each byte once instead of visiting each node and asking about its head. A
/// definition without the spelling cannot contain a lookup, so this is the whole
/// cost such a definition pays.
///
/// The converse does not hold — a mention inside a string or a comment answers
/// `true` — which is the harmless direction: the walk then runs and finds
/// nothing.
fn mentions_gethash(source: &str) -> bool {
    let needle = b"gethash";
    source
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// One lookup that repeats an earlier one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatedLookup {
    /// The repeating call, which is what is reported.
    pub span: ByteSpan,
    /// The span of the earlier call it repeats.
    pub first_span: ByteSpan,
    pub key: String,
    pub table: String,
}

/// Every `gethash` read in a definition's body, with the arms each sits under.
///
/// One walk of the matched subtree. `written` is filled before any `gethash`
/// inside it is visited, because the walk is pre-order and a mutating operator
/// is visited before its own place arguments.
fn body_reads(body: &[ExpressionView], source: &str) -> Vec<Read> {
    let mut reads: Vec<Read> = Vec::new();
    let mut written: Vec<ByteSpan> = Vec::new();

    for form in body {
        for_each_evaluated_subview_with_branches(form, |node, path| {
            let Some(head) = list_head(node) else {
                return true;
            };
            let name = unqualified(head);
            // A declaration cannot read or write the table it names, and
            // `(declare (type hash-table table))` would otherwise count as an
            // occurrence and disqualify it.
            if name.eq_ignore_ascii_case("declare") || name.eq_ignore_ascii_case("declaim") {
                return false;
            }
            if name.eq_ignore_ascii_case("gethash") {
                if !written.contains(&node.span) {
                    if let Some(read) = read_gethash(node, source, path) {
                        reads.push(read);
                    }
                }
                return true;
            }
            for position in write_place_positions(name, node.children.len()) {
                if let Some(place) = node.children.get(position) {
                    written.push(place.span);
                }
            }
            true
        });
    }
    reads
}

/// Whether every occurrence of `table`'s name in the body is accounted for by
/// one of the reads that named it.
///
/// The disqualifier, and the reason it is phrased as a count rather than as a
/// search for writes: an occurrence that is not a read may be a `setf` place, a
/// `remhash`, a `clrhash`, a `let` binding, or an argument to a callee that does
/// any of those. None of them has to be told apart from the others, and no
/// callee has to be resolved.
///
/// Paid only once a repeat is already a candidate, which in code with no
/// findings is never.
fn every_occurrence_is_a_read(body: &[ExpressionView], table: &str, reads: usize) -> bool {
    let mut occurrences = 0;
    for form in body {
        for_each_evaluated_subview_with_branches(form, |node, _| {
            if let Some(head) = list_head(node) {
                let name = unqualified(head);
                if name.eq_ignore_ascii_case("declare") || name.eq_ignore_ascii_case("declaim") {
                    return false;
                }
                return true;
            }
            if atom_text(node).is_some_and(|text| unqualified(text).eq_ignore_ascii_case(table)) {
                occurrences += 1;
            }
            true
        });
    }
    occurrences == reads
}

/// Every repeated lookup in one definition.
///
/// `source` is the whole file's text, which is what the spans index into.
///
/// Two phases, in that order for a reason: the pairing is decided from the reads
/// alone, and the whole-body occurrence count that disqualifies a written table
/// runs only for the tables a pair was actually found on. A definition with no
/// repeat therefore pays one walk, not two, and never counts a symbol.
#[must_use]
pub fn examine(view: &ExpressionView, source: &str) -> Vec<RepeatedLookup> {
    let Some(body) = definition_body(view) else {
        return Vec::new();
    };
    let reads = body_reads(body, source);
    if reads.len() < 2 {
        return Vec::new();
    }

    // Phase one: which reads repeat which, ignoring mutation entirely.
    let mut candidates: Vec<RepeatedLookup> = Vec::new();
    for (position, later) in reads.iter().enumerate() {
        let Some(key) = later.key.as_ref() else {
            continue;
        };
        let Some(earlier) = reads[..position].iter().find(|earlier| {
            earlier.table == later.table
                && earlier.key.as_ref() == Some(key)
                && earlier.default == later.default
                && !branches_are_exclusive(&earlier.path, &later.path)
        }) else {
            continue;
        };
        candidates.push(RepeatedLookup {
            span: later.span,
            first_span: earlier.span,
            key: key.clone(),
            table: later.table.clone(),
        });
    }
    if candidates.is_empty() {
        return Vec::new();
    }

    // Phase two, paid only now: is each candidate's table one this body never
    // writes and never hands to anything that could?
    let mut verdicts: Vec<(String, bool)> = Vec::new();
    candidates.retain(|candidate| {
        let table = candidate.table.as_str();
        if let Some((_, verdict)) = verdicts.iter().find(|(name, _)| name == table) {
            return *verdict;
        }
        let accounted = reads.iter().filter(|read| read.table == table).count();
        let verdict = every_occurrence_is_a_read(body, table, accounted);
        verdicts.push((candidate.table.clone(), verdict));
        verdict
    });
    candidates
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
        // The byte scan that keeps a definition without a lookup from paying for
        // a walk.
        if !mentions_gethash(context.slice(view.span)) {
            return Ok(());
        }
        let found = examine(view, context.source());
        if found.is_empty() {
            return Ok(());
        }
        // Only now: the engine's dispatch walks into quoted data, and a `defun`
        // inside a macro template is a list of symbols.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for repeat in found {
            sink.report(
                repeat.span,
                format!(
                    "this repeats the (gethash {} {}) already looked up in this body, which \
                     never writes {}; bind the value once",
                    repeat.key, repeat.table, repeat.table
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::testing::{messages, reported};

    fn findings(source: &str) -> Vec<String> {
        reported(&META, &RULE, source)
    }

    fn count(source: &str) -> usize {
        findings(source).len()
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_lookup_repeated_in_a_body() {
        assert_eq!(
            count("(defun f (h) (list (gethash :a h) (gethash :a h)))"),
            1
        );
    }

    #[test]
    fn flags_a_test_repeated_in_the_branch_it_guards() {
        // The canonical shape: presence tested, then value read again.
        assert_eq!(
            count("(defun f (h) (if (gethash :a h) (use (gethash :a h)) nil))"),
            1
        );
    }

    #[test]
    fn flags_the_third_lookup_as_well_as_the_second() {
        assert_eq!(
            count("(defun f (h) (list (gethash :a h) (gethash :a h) (gethash :a h)))"),
            2
        );
    }

    #[test]
    fn flags_a_lookup_on_a_global_table() {
        assert_eq!(
            count("(defun f () (list (gethash :a *cache*) (gethash :a *cache*)))"),
            1
        );
    }

    #[test]
    fn flags_a_defmethod_body_with_and_without_a_qualifier() {
        assert_eq!(
            count("(defmethod area ((s shape) h) (list (gethash :a h) (gethash :a h)))"),
            1
        );
        assert_eq!(
            count("(defmethod area :around ((s shape) h) (list (gethash :a h) (gethash :a h)))"),
            1
        );
    }

    #[test]
    fn flags_every_literal_key_kind() {
        for key in [":a", "'a", "\"a\"", "42", "-1", "#\\a"] {
            assert_eq!(
                count(&format!(
                    "(defun f (h) (list (gethash {key} h) (gethash {key} h)))"
                )),
                1,
                "key {key} must group with itself"
            );
        }
    }

    #[test]
    fn a_short_circuit_operand_still_repeats_its_predecessor() {
        // `(and a b)` runs `b` only when `a` was true — at which point both ran.
        assert_eq!(
            count("(defun f (h) (and (gethash :a h) (plusp (gethash :a h))))"),
            1
        );
    }

    #[test]
    fn the_package_qualified_spelling_is_read_the_same() {
        assert_eq!(
            count("(cl:defun f (h) (list (cl:gethash :a h) (cl:gethash :a h)))"),
            1
        );
    }

    #[test]
    fn the_message_names_the_key_and_the_table() {
        let said = messages(
            &META,
            &RULE,
            "(defun f (h) (list (gethash :a h) (gethash :a h)))",
        );
        assert_eq!(said.len(), 1);
        assert!(said[0].contains(":a"), "{said:?}");
        assert!(said[0].contains(" h)"), "{said:?}");
    }

    #[test]
    fn the_reported_span_is_the_repeat_not_the_original() {
        let source = "(defun f (h) (list (gethash :a h) (second (gethash :a h))))";
        let found = findings(source);
        assert_eq!(found, vec!["(gethash :a h)"]);
        // And it is the *second* one: its start is past the first's.
        let tree = paredit_core_syntax::sexpr::SyntaxTree::parse_with_dialect(
            source,
            paredit_core_syntax::dialect::Dialect::CommonLisp,
        )
        .expect("parse");
        let definition = &tree.root_view().children[0];
        let repeats = examine(definition, source);
        assert_eq!(repeats.len(), 1);
        assert!(repeats[0].span.start().get() > repeats[0].first_span.start().get());
    }

    // -- branch exclusivity ---------------------------------------------------

    #[test]
    fn does_not_flag_two_lookups_in_arms_of_one_if() {
        assert_eq!(
            count("(defun f (h) (if p (gethash :a h) (gethash :a h)))"),
            0
        );
    }

    #[test]
    fn does_not_flag_two_lookups_in_different_cond_clauses() {
        assert_eq!(
            count("(defun f (h) (cond (p (gethash :a h)) (t (gethash :a h))))"),
            0
        );
    }

    #[test]
    fn does_not_flag_two_lookups_in_different_case_clauses() {
        assert_eq!(
            count("(defun f (h k) (case k (:x (gethash :a h)) (:y (gethash :a h))))"),
            0
        );
    }

    #[test]
    fn does_not_flag_two_lookups_in_different_loop_clauses() {
        assert_eq!(
            count(
                "(defun f (h xs) (loop for x in xs if (gethash :a h) collect x else collect (gethash :a h)))"
            ),
            0
        );
    }

    #[test]
    fn does_flag_two_lookups_in_the_same_arm() {
        assert_eq!(
            count("(defun f (h) (if p (list (gethash :a h) (gethash :a h)) nil))"),
            1
        );
    }

    // -- mutation and escape --------------------------------------------------

    #[test]
    fn does_not_flag_a_table_the_body_writes_with_setf() {
        assert_eq!(
            count("(defun f (h) (gethash :a h) (setf (gethash :a h) 1) (gethash :a h))"),
            0
        );
    }

    #[test]
    fn does_not_flag_a_table_the_body_clears_or_removes_from() {
        assert_eq!(
            count("(defun f (h) (gethash :a h) (remhash :a h) (gethash :a h))"),
            0
        );
        assert_eq!(
            count("(defun f (h) (gethash :a h) (clrhash h) (gethash :a h))"),
            0
        );
        assert_eq!(
            count("(defun f (h) (gethash :a h) (incf (gethash :a h)) (gethash :a h))"),
            0
        );
    }

    #[test]
    fn does_not_flag_a_table_handed_to_a_call_that_might_write_it() {
        assert_eq!(
            count("(defun f (h) (gethash :a h) (refresh-cache h) (gethash :a h))"),
            0
        );
        assert_eq!(
            count("(defun f (h) (gethash :a h) (maphash #'note h) (gethash :a h))"),
            0
        );
    }

    /// The value position of a `setf` is an ordinary read, and it is the most
    /// common way a lookup is spelled. Treating it as a write would silence the
    /// rule everywhere.
    #[test]
    fn a_lookup_in_a_setf_value_position_is_still_a_lookup() {
        assert_eq!(
            count("(defun f (h) (setf v (gethash :a h)) (setf w (gethash :a h)))"),
            1
        );
    }

    /// A `let` binding of the table is an occurrence of its name, so the table
    /// is disqualified. A deliberate false negative — see the module docs.
    #[test]
    fn does_not_flag_a_table_bound_in_the_body_itself() {
        assert_eq!(
            count(
                "(defun f () (let ((h (make-hash-table))) (list (gethash :a h) (gethash :a h))))"
            ),
            0
        );
    }

    #[test]
    fn a_declaration_naming_the_table_does_not_disqualify_it() {
        assert_eq!(
            count(
                "(defun f (h) (declare (type hash-table h)) (list (gethash :a h) (gethash :a h)))"
            ),
            1
        );
    }

    // -- near-miss negatives --------------------------------------------------

    #[test]
    fn does_not_flag_two_different_keys() {
        assert_eq!(
            count("(defun f (h) (list (gethash :a h) (gethash :b h)))"),
            0
        );
    }

    #[test]
    fn does_not_flag_the_same_key_on_two_tables() {
        assert_eq!(
            count("(defun f (g h) (list (gethash :a g) (gethash :a h)))"),
            0
        );
    }

    #[test]
    fn does_not_flag_a_computed_key() {
        assert_eq!(
            count("(defun f (h k) (list (gethash k h) (gethash k h)))"),
            0
        );
        assert_eq!(
            count("(defun f (h x) (list (gethash (name x) h) (gethash (name x) h)))"),
            0
        );
    }

    #[test]
    fn does_not_flag_a_computed_table() {
        assert_eq!(
            count("(defun f (x) (list (gethash :a (cache-of x)) (gethash :a (cache-of x))))"),
            0
        );
    }

    #[test]
    fn does_not_flag_two_lookups_with_different_defaults() {
        assert_eq!(
            count("(defun f (h) (list (gethash :a h) (gethash :a h 0)))"),
            0
        );
        assert_eq!(
            count("(defun f (h) (list (gethash :a h 0) (gethash :a h 0)))"),
            1
        );
    }

    #[test]
    fn does_not_flag_a_single_lookup() {
        assert_eq!(count("(defun f (h) (gethash :a h))"), 0);
    }

    #[test]
    fn does_not_flag_lookups_in_two_separate_definitions() {
        assert_eq!(
            count("(defun f (h) (gethash :a h))\n(defun g (h) (gethash :a h))"),
            0
        );
    }

    #[test]
    fn does_not_flag_a_form_that_is_not_a_definition() {
        assert_eq!(count("(let ((h t)) (gethash :a h) (gethash :a h))"), 0);
    }

    #[test]
    fn does_not_flag_a_lookup_of_the_wrong_arity() {
        assert_eq!(count("(defun f (h) (gethash h) (gethash h))"), 0);
        assert_eq!(
            count("(defun f (h) (gethash :a h 0 1) (gethash :a h 0 1))"),
            0
        );
    }

    // -- quote-context negative -----------------------------------------------

    #[test]
    fn reports_nothing_inside_quoted_data() {
        assert_eq!(
            count("'(defun f (h) (list (gethash :a h) (gethash :a h)))"),
            0
        );
        assert_eq!(
            count("(quote (defun f (h) (list (gethash :a h) (gethash :a h))))"),
            0
        );
        assert_eq!(
            count("`(defun f (h) (list (gethash :a h) (gethash :a h)))"),
            0
        );
    }

    /// A lookup written into a macro template is data, and must not pair with a
    /// lookup in the surrounding code.
    #[test]
    fn a_lookup_inside_a_quoted_subform_is_not_a_lookup() {
        assert_eq!(
            count("(defun f (h) (gethash :a h) (emit '(gethash :a h)))"),
            0
        );
    }

    #[test]
    fn reports_a_definition_escaped_back_into_code_by_an_unquote() {
        assert_eq!(
            count("`(a ,(defun f (h) (list (gethash :a h) (gethash :a h))))"),
            1
        );
    }

    // -- string-literal negative ----------------------------------------------

    #[test]
    fn reports_nothing_spelled_only_inside_a_string() {
        assert_eq!(
            count("(defun f (h) (format t \"(gethash :a h) (gethash :a h)\"))"),
            0
        );
        assert_eq!(count("(defun doc () \"(gethash :a h) (gethash :a h)\")"), 0);
    }

    /// The byte-scan gate answers `true` for a mention in a string, and the walk
    /// must then find nothing — the same answer it gave before the gate existed.
    #[test]
    fn the_walk_guard_answers_yes_only_for_the_spelling() {
        assert!(mentions_gethash("(gethash :a h)"));
        assert!(mentions_gethash("(GETHASH :a h)"));
        assert!(mentions_gethash("(cl:gethash :a h)"));
        assert!(!mentions_gethash("(remhash :a h)"));
        assert!(!mentions_gethash("(gethas"));
        assert!(!mentions_gethash(""));
    }
}
