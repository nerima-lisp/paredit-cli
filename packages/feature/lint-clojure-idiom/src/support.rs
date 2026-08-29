//! What the Clojure-idiom rules share: which parts of a file are *code*, which
//! literal spellings the reader produces, and the laziness vocabulary.
//!
//! # Evaluation context
//!
//! The quote machinery below (`QuoteState` — private — plus
//! [`for_each_evaluated_subview`] and [`is_unevaluated_at`]) is a deliberate
//! copy of `paredit-feature-lint-condition-system`'s `support.rs`, semantics
//! included, by way of `paredit-feature-lint-concurrency`'s copy of it. It is
//! copied rather than depended on because a feature package must not depend on
//! another feature package (§4.2), and it is copied *exactly* because the two
//! things a hand-rolled version keeps getting wrong are subtle:
//!
//! - `'` and `` ` `` are not one counter. A comma inside `'(…)` is a comma
//!   character in a literal list — it does not escape back to code — so `hard`
//!   is a latch that never clears, while `quasi` counts up and down. A single
//!   `i32` depth would call `'(a ,x)` code.
//! - The verdict is read *at* the node, not one level up. A node one level
//!   inside a quote is still data even though it carries no reader prefix of
//!   its own, so checking a node's own `reader_prefixes` is not enough.
//!
//! In Clojure the unquote is `~`, and `,` is plain whitespace the reader drops
//! — so `` `(a ,x) `` is `` `(a x) ``, all data. The same two counters give the
//! right answer because they are driven by [`ReaderPrefix`], which the
//! dialect-aware parser has already resolved.
//!
//! This matters more here than in a Common Lisp package: `def` and `apply`
//! both appear inside macro templates all the time, and `defmacro` bodies are
//! where `` `(def ~name …) `` lives.
//!
//! Two forms make this harder than the reader alone suggests, because they hold
//! unevaluated code with **no prefix at all** — `case`'s test constants and
//! `(comment …)`'s whole body. Both were found reporting false positives on
//! real code; see `ChildEvaluation` and this package's README.
//!
//! # Cost
//!
//! Every rule in this package calls [`is_unevaluated_at`], and every one calls
//! it **only after it already holds a candidate finding** — never as a
//! precondition on the head match. That ordering is the whole cost story:
//! [`is_unevaluated_at`] materializes the enclosing top-level form, so calling
//! it on every head match would cost the file's size once per candidate.
//! `atom-swap-with-side-effect` established the ordering (`rule.rs`:
//! `if items.is_empty() { return Ok(()); }` *before* the quote check) and every
//! rule here follows it.
//!
//! Nothing else here allocates before a head has matched. The one subtree walk
//! in the package — `def-inside-function-body`'s — runs over the
//! `ExpressionView` the dispatcher already built and handed to `check`, so it
//! is pointer-chasing plus one `Vec` for the traversal stack, not a second
//! materialization of the document.
//!
//! ## The guards that are cost, not behaviour
//!
//! Mutation testing over this package reports four surviving guards, and all
//! four survive **correctly** — deleting them changes cost and not one
//! finding. They are listed here so that a later reader does not mistake a
//! recorded result for an untested gap:
//!
//! - `for_each_evaluated_call`'s childless-child skip, which halves the stack
//!   traffic over a function body.
//! - `def-inside-function-body`'s `may_be_definition_head`, a three-byte
//!   pre-filter whose licence to exist is that it is *exact*; that exactness is
//!   asserted directly by
//!   `the_three_byte_pre_filter_agrees_with_the_membership_test_on_every_head`,
//!   because a mutation test cannot prove it.
//! - `with-open-returns-lazy-seq`'s cheap laziness pre-check and its
//!   `names.is_empty()` early return, both of which only avoid work that the
//!   `find` below would then decline anyway.
//! - each `rule.rs`'s `if items.is_empty() { return Ok(()); }`, which is the
//!   ordering that keeps [`is_unevaluated_at`] off the per-head-match path.
//!   This one is the package's whole cost story and is measured in the README,
//!   not asserted by a test.
//!
//! # A known gap in head normalization
//!
//! [`paredit_core_syntax::view_query::unqualified`] splits on `:`, the Common
//! Lisp package marker. Clojure's `/` namespace separator is *not* stripped, so
//! a fully qualified `clojure.core/count` does not normalize to `count` and
//! neither the engine's head index nor [`symbol_in`] matches it. That is an
//! engine-wide property, not something this package can fix locally; the rules
//! here therefore match the bare spelling, which is how core functions are
//! written in practice. A missed `clojure.core/`-qualified call is a false
//! negative, which is the direction this package errs in throughout.

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in, symbol_is};

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuoteState {
    hard: bool,
    quasi: u32,
}

impl QuoteState {
    const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    const fn is_data(self) -> bool {
        self.hard || self.quasi > 0
    }

    /// The state inside a node, given the state outside it and the node's own
    /// reader prefixes.
    ///
    /// `#'`, `#.`, metadata and the rest are deliberately neutral: none of them
    /// turns code into data. Note that Clojure's `@` deref also reads as
    /// [`ReaderPrefix::Function`], and is likewise neutral — `@x` is code. So is
    /// [`ReaderPrefix::HashLiteral`], which is how `#(…)` and `#{…}` are
    /// carried: a reader lambda's body is code, and a set literal's elements are
    /// evaluated.
    fn after_prefixes(mut self, view: &ExpressionView) -> Self {
        for prefix in &view.reader_prefixes {
            match prefix {
                ReaderPrefix::Quote => self.hard = true,
                ReaderPrefix::Quasiquote => self.quasi += 1,
                ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing => {
                    self.quasi = self.quasi.saturating_sub(1);
                }
                _ => {}
            }
        }
        self
    }

    const fn quoted(mut self) -> Self {
        self.hard = true;
        self
    }
}

/// Whether a head names `quote`.
///
/// The `ends_with` test is a pre-filter, not the answer: `symbol_is` goes
/// through `unqualified`, which is an `rsplit_once(':')` and therefore scans the
/// whole symbol, and this runs on **every list node** of every walk in this
/// package. A qualified spelling still ends in `quote`, so the pre-filter never
/// rejects something `symbol_is` would have accepted; `misquote` gets past it
/// and is then correctly rejected by `symbol_is`.
fn is_quote_head(head: &str) -> bool {
    head.len() >= 5
        && head.as_bytes()[head.len() - 5..].eq_ignore_ascii_case(b"quote")
        && symbol_is(head, "quote")
}

/// `case`, whose test constants the reader leaves as data.
const CASE_HEADS: &[&str] = &["case"];

/// `comment`, whose entire body is dropped.
const COMMENT_HEADS: &[&str] = &["comment"];

/// Whether a head names `case`.
///
/// The length-and-suffix pre-filter is the same trick as [`is_quote_head`]: this
/// runs on every list node of every walk in this package, and `symbol_in` goes
/// through `unqualified`, which scans the whole symbol.
fn is_case_head(head: &str) -> bool {
    head.len() >= 4
        && head.as_bytes()[head.len() - 4..].eq_ignore_ascii_case(b"case")
        && symbol_in(head, CASE_HEADS)
}

/// Whether a head names `comment`.
fn is_comment_head(head: &str) -> bool {
    head.len() >= 7
        && head.as_bytes()[head.len() - 7..].eq_ignore_ascii_case(b"comment")
        && symbol_in(head, COMMENT_HEADS)
}

/// Which of a form's children the reader and the compiler actually evaluate.
///
/// Three forms in `clojure.core` hold code that is never run, and **none of
/// them marks it with a reader prefix** — which is what makes them invisible to
/// the [`QuoteState`] model on its own. Each was found by the corpus audit in
/// this package's README, on real code, not by reasoning about the language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChildEvaluation {
    /// Ordinary code: every child is evaluated.
    Evaluated,
    /// Nothing below this form is evaluated — `(quote …)` and `(comment …)`.
    ///
    /// `comment` is a macro whose expansion is `nil`; its body is read, then
    /// discarded. `(defn f [] (comment (def x 1)))` interns nothing, and
    /// clj-kondo's own test suite asserts that its `:inline-def` linter is
    /// silent on exactly that shape.
    None,
    /// `case`: the test constants are data, the result expressions are code.
    CaseTestConstants,
}

impl ChildEvaluation {
    /// What a head says about its own children.
    fn of(head: &str) -> Self {
        if is_quote_head(head) || is_comment_head(head) {
            Self::None
        } else if is_case_head(head) {
            Self::CaseTestConstants
        } else {
            Self::Evaluated
        }
    }

    /// Whether child `index` of a form with `child_count` children is data.
    const fn is_data_child(self, child_count: usize, index: usize) -> bool {
        match self {
            Self::Evaluated => false,
            Self::None => true,
            Self::CaseTestConstants => is_case_test_position(child_count, index),
        }
    }
}

/// Whether child `index` of a `case` form is a **test constant**.
///
/// `case` is the one `clojure.core` form other than `quote` that leaves part of
/// its own body unevaluated, and it does so without any reader prefix to say so:
///
/// ```clojure
/// (case tag
///   (def defonce defmulti) (handle-def tag)   ; ← a *list of constants*
///   (defn defn- defmacro)  (handle-defn tag)
///   (fallback tag))
/// ```
///
/// The test position is compared against `tag` as literal data — `case` never
/// evaluates it — so `(def defonce defmulti)` is a three-element list of
/// symbols and **not** a call to `def`. Read as code it is a textbook
/// `def-inside-function-body`, which is exactly the false positive this
/// function exists to prevent: it was found in `clj-kondo`'s own
/// `analyzer.clj`, twice, by the corpus audit in this package's README.
///
/// The layout is `(case e t1 r1 t2 r2 … default?)`. Children 0 and 1 are the
/// head and the expression; the body is everything after. An **odd** body has a
/// trailing default, which is a result expression and therefore code — so only
/// the paired region holds test constants, at every other index from 2.
const fn is_case_test_position(child_count: usize, index: usize) -> bool {
    if index < 2 || index >= child_count {
        return false;
    }
    // `index >= 2` and `index < child_count` give `child_count >= 3`.
    let body = child_count - 2;
    let paired_end = if body % 2 == 1 {
        child_count - 1
    } else {
        child_count
    };
    index < paired_end && (index - 2) % 2 == 0
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Calls `visit` on every node of `root` that is reachable as evaluated code,
/// in the same pre-order the lint engine's own walk produces.
///
/// Quoted subtrees are still *descended* — `` `(a ~(f)) `` has code inside data
/// — but their data nodes are never visited.
pub fn for_each_evaluated_subview(root: &ExpressionView, visit: impl FnMut(&ExpressionView)) {
    for_each_evaluated_subview_where(root, |_| true, visit);
}

/// [`for_each_evaluated_subview`], with a say in where the walk stops.
///
/// `descend` is asked about each visited node *before* its children are queued;
/// answering `false` visits that node and nothing under it.
///
/// A pruned node's subtree is skipped *including* any quoted data in it, which
/// is correct here and would not be if this were used to find data.
pub fn for_each_evaluated_subview_where(
    root: &ExpressionView,
    mut descend: impl FnMut(&ExpressionView) -> bool,
    mut visit: impl FnMut(&ExpressionView),
) {
    let mut stack = vec![(root, QuoteState::EVALUATED)];
    while let Some((view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() {
            visit(view);
            if !descend(view) {
                continue;
            }
        }
        // `(quote …)`, `(comment …)` and a `case` test constant are all data
        // without carrying a reader prefix; see `ChildEvaluation`.
        let evaluation = list_head(view).map_or(ChildEvaluation::Evaluated, ChildEvaluation::of);
        let child_count = view.children.len();
        for (index, child) in view.children.iter().enumerate().rev() {
            let child_state = if evaluation.is_data_child(child_count, index) {
                state.quoted()
            } else {
                state
            };
            stack.push((child, child_state));
        }
    }
}

/// Calls `visit(node, head)` for every evaluated `(head …)` call under `root`,
/// in the same pre-order as [`for_each_evaluated_subview`].
///
/// The narrow-and-fast counterpart of that function, for the one rule here that
/// examines a whole subtree per head match rather than a fixed number of
/// children — `def-inside-function-body`, whose per-node work is multiplied by
/// the file's size. Three differences, all measured:
///
/// - **Childless nodes are never pushed.** Atoms are the majority of a Clojure
///   function body and can carry no head, so they cannot satisfy any caller of
///   this function; not queueing them halves the stack traffic. An empty `()`
///   is skipped for the same reason and by the same test.
/// - **`list_head` is called once.** The generic walk computes it in the
///   visitor and again in its own `(quote …)` test.
/// - **The stack is pre-sized**, so a function body does not grow it five times.
///
/// The quote model is the *same* `QuoteState`, not a second copy of it: that
/// is the whole point of it living here, and a private re-derivation is the
/// mistake this module's header is about.
pub fn for_each_evaluated_call(
    root: &ExpressionView,
    mut visit: impl FnMut(&ExpressionView, &str),
) {
    let mut stack: Vec<(&ExpressionView, QuoteState)> = Vec::with_capacity(32);
    stack.push((root, QuoteState::EVALUATED));
    while let Some((view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        let head = list_head(view);
        if !state.is_data() {
            if let Some(head) = head {
                visit(view, head);
            }
        }
        // `(quote …)`, `(comment …)` and a `case` test constant are all data
        // without carrying a reader prefix; see `ChildEvaluation`.
        let evaluation = head.map_or(ChildEvaluation::Evaluated, ChildEvaluation::of);
        let child_count = view.children.len();
        for (index, child) in view.children.iter().enumerate().rev() {
            // Nothing without children can be a call, and nothing below it can
            // exist. A quoted atom is data, but this visitor never sees atoms.
            if child.children.is_empty() {
                continue;
            }
            let child_state = if evaluation.is_data_child(child_count, index) {
                state.quoted()
            } else {
                state
            };
            stack.push((child, child_state));
        }
    }
}

/// The one child of `view` whose span covers `target`, found without reading
/// the others.
///
/// A node's children are in document order and do not overlap, so the only
/// child that can contain `target` is the last one beginning at or before it —
/// which a binary search finds in `log₂ k` comparisons instead of `k`.
fn child_containing(view: &ExpressionView, target: ByteSpan) -> Option<(usize, &ExpressionView)> {
    let after = view
        .children
        .partition_point(|child| child.span.start().get() <= target.start().get());
    let index = after.checked_sub(1)?;
    let child = view.children.get(index)?;
    span_contains(child.span, target).then_some((index, child))
}

/// The top-level form containing `target`, materialized on its own.
///
/// The reason this is not `tree.root_view()` followed by a search: `root_view`
/// builds an `ExpressionView` — a `Vec` of children and a `Vec` of reader
/// prefixes — for *every node in the file*, so asking it about one node costs
/// the whole document, and a file of T candidates would cost T×T.
///
/// Selecting the one root child instead costs a binary search over the top
/// level plus that one form's own subtree.
fn root_child_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let start_of = |index: usize| {
        tree.select_path(&Path::root_child(index))
            .ok()
            .map(|selection| selection.span().start().get())
    };
    // Top-level forms are in document order and do not overlap, so the only
    // candidate is the last one beginning at or before `target`.
    let mut low = 0;
    let mut high = tree.root_children().len();
    while low < high {
        let middle = low + (high - low) / 2;
        if start_of(middle)? <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let selection = tree
        .select_path(&Path::root_child(low.checked_sub(1)?))
        .ok()?;
    span_contains(selection.span(), target).then(|| selection.view())
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends to `target` through the one child at each level whose span contains
/// it, so the cost is the enclosing top-level form's size, and never the
/// file's.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ~(f)) `` has a quasiquoted ancestor and an
/// evaluated target. Being inside a hard `'` does settle it, and that is
/// already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let Some(top_level) = root_child_containing(tree, target) else {
        return false;
    };
    let mut view: &ExpressionView = &top_level;
    // The root carries no reader prefix and is not a `(quote …)` form, so the
    // state entering the top-level form is whatever that form's own prefixes
    // say.
    let mut state = QuoteState::EVALUATED.after_prefixes(view);

    while view.span != target {
        // `(quote …)`, `(comment …)` and a `case` test constant are all data
        // without carrying a reader prefix; see `ChildEvaluation`.
        let evaluation = list_head(view).map_or(ChildEvaluation::Evaluated, ChildEvaluation::of);
        let child_count = view.children.len();
        // A span that names no node is judged by the innermost node that
        // contains it, which is the honest answer for a span the caller
        // synthesized rather than took from the tree.
        let Some((index, child)) = child_containing(view, target) else {
            return state.is_data();
        };
        state = state.after_prefixes(child);
        if evaluation.is_data_child(child_count, index) {
            state = state.quoted();
        }
        view = child;
    }
    state.is_data()
}

// ---------------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------------

/// An atom's symbol text, past any reader prefix, lowercased — the spelling
/// every comparison here is written in.
///
/// Deliberately **not** `unqualified`: Clojure symbols are case-sensitive at
/// the reader level, but every core name these rules match is lowercase anyway,
/// and lowercasing keeps one spelling for the head index (which lowercases
/// nothing for Clojure) and for these comparisons to meet at.
#[must_use]
pub fn normalized_symbol(text: &str) -> String {
    text.to_ascii_lowercase()
}

/// The symbol an atom names, in the normalized spelling, or `None` for a
/// non-atom.
///
/// The reader prefix is dropped before the name is read, so Clojure's `@reader`
/// and a bare `reader` both come back as `reader`. That is what the rules here
/// want: `with-open-returns-lazy-seq` asks whether a bound name is *mentioned*,
/// not how it was spelled.
#[must_use]
pub fn symbol_name(view: &ExpressionView) -> Option<String> {
    atom_symbol_text(view)
        .filter(|text| !text.is_empty())
        .map(normalized_symbol)
}

/// Whether `view` is a `(...)` list whose head names one of `heads`.
#[must_use]
pub fn head_is(view: &ExpressionView, heads: &[&str]) -> bool {
    list_head(view).is_some_and(|head| symbol_in(head, heads))
}

/// Whether `view` is a Clojure metadata form — `^:private`, `^String`,
/// `^{:doc "…"}`.
///
/// The reader makes these a **sibling** of what they annotate rather than a
/// prefix on it, so `(defn ^:private f [] …)` has five children and the
/// function's name is at index 2. Any rule reading a positional child of a
/// definition has to skip them or it reads the metadata as the name.
#[must_use]
pub fn is_metadata_form(view: &ExpressionView) -> bool {
    view.reader_prefixes.contains(&ReaderPrefix::Metadata)
}

/// `children`, with Clojure metadata forms dropped.
pub fn skip_metadata_forms(
    children: &[ExpressionView],
) -> impl Iterator<Item = &ExpressionView> + use<'_> {
    children.iter().filter(|child| !is_metadata_form(child))
}

// ---------------------------------------------------------------------------
// Literal spellings, as the dialect-aware reader produces them
// ---------------------------------------------------------------------------

/// Whether `view` is a `[…]` vector literal.
///
/// The **delimiter alone** separates a vector from the two things it could be
/// confused with: `#{1 2}` is a `Brace` node and `#(…)` is a `Paren` node, so
/// neither reaches the bracket test. An earlier revision also excluded
/// [`ReaderPrefix::HashLiteral`] here, on the theory that it was what kept
/// [`is_ordered_literal_sequence`] from accepting a set; mutation testing
/// showed that deleting it changed no test and no behaviour, because Clojure
/// has no `#[…]` reader syntax for it to reject. It was dead code and is gone.
#[must_use]
pub fn is_vector_literal(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Bracket)
}

/// Whether `view` is a literal collection whose element **order is defined** —
/// a `[…]` vector or a `'(…)` quoted list.
///
/// A `#{…}` set and a `{…}` map are deliberately excluded, and this is a
/// soundness requirement rather than a matter of taste. `(apply f #{1 2 3})`
/// passes the set's elements in the set's iteration order, which Clojure does
/// not specify; rewriting it to `(f 1 2 3)` would pick one order and call it
/// the meaning of the program. A map is worse still — `apply` would pass
/// `MapEntry` values, not the keys and values.
#[must_use]
pub fn is_ordered_literal_sequence(view: &ExpressionView) -> bool {
    if is_vector_literal(view) {
        return true;
    }
    // `'(1 2 3)`: a paren list carrying the hard quote. An unquoted `(1 2 3)`
    // is a *call* to `1`, not a literal, so the prefix is required.
    is_paren_list(view) && view.reader_prefixes.contains(&ReaderPrefix::Quote)
}

// ---------------------------------------------------------------------------
// The laziness vocabulary. Every entry below was checked against
// `clojure/core.clj`; see this package's README for the audit.
// ---------------------------------------------------------------------------

/// `clojure.core` functions and macros that return an **unrealized** sequence.
///
/// Everything here is either written with `lazy-seq` in `core.clj` or documented
/// as returning a lazy seq. The list is deliberately confined to `clojure.core`:
/// a user function returning a lazy seq is invisible to this rule, and guessing
/// from a name would report correct code.
///
/// `sort`, `sort-by`, `reverse`, `into`, `mapv`, `filterv`, `vec`, `set`,
/// `frequencies` and `group-by` are **absent on purpose** — every one of them is
/// eager, and several of them are the fix this rule suggests.
pub const LAZY_SEQUENCE_HEADS: &[&str] = &[
    "map",
    "map-indexed",
    "mapcat",
    "filter",
    "remove",
    "keep",
    "keep-indexed",
    "take",
    "take-while",
    "take-nth",
    "drop",
    "drop-while",
    "drop-last",
    "for",
    "line-seq",
    "re-seq",
    "resultset-seq",
    "xml-seq",
    "tree-seq",
    "iterate",
    "repeatedly",
    "repeat",
    "cycle",
    "concat",
    "interleave",
    "interpose",
    "partition",
    "partition-all",
    "partition-by",
    "distinct",
    "dedupe",
    "flatten",
    "lazy-seq",
    "lazy-cat",
    "sequence",
    "rest",
    "next",
];

/// `clojure.core` operations that force a sequence into memory and return the
/// realized result.
///
/// `with-open-returns-lazy-seq` uses these as a **barrier**, not as a
/// whole-subtree veto: `(map parse (doall (line-seq r)))` is still an
/// unrealized `map`, but it is unrealized over data that is *already in
/// memory*, so consuming it after the reader closes works and reporting it
/// would be a false positive. Pruning the search for the resource mention at
/// these heads is what expresses that, and it is strictly better than asking
/// whether one appears anywhere in the subtree — `(map #(reduce + %) (line-seq
/// r))` has a `reduce` in it and is still the defect.
///
/// The list doubles as the rule's suggested repair, which is why `doall`,
/// `into`, `vec` and `reduce` have to be in one list rather than two that can
/// drift apart.
///
/// `apply` is deliberately absent: `(apply concat (line-seq r))` is still lazy.
/// So is `reductions`, which is lazy despite the name's resemblance to
/// `reduce`.
pub const EAGER_REALIZER_HEADS: &[&str] = &[
    "doall",
    "dorun",
    "into",
    "vec",
    "set",
    "reduce",
    "transduce",
    "mapv",
    "filterv",
    "frequencies",
    "group-by",
    "sort",
    "sort-by",
    "reverse",
    "shuffle",
];

/// Forms whose *last child* is the value the form returns.
///
/// The threading macros belong here for the same reason the body forms do:
/// `(->> (line-seq r) (map parse))` returns whatever its last stage returns, so
/// `(->> (line-seq r) (map parse) (into []))` is eager and must not be reported
/// — which is the single most common correct spelling this rule has to stay
/// quiet on.
///
/// `doto` is deliberately absent: it returns its **first** argument, not its
/// last, so reading its tail would be reading the wrong form. `cond`, `condp`
/// and `case` are absent too, for the opposite reason — they have several
/// tails, and this package prefers a false negative to a guess.
pub const TAIL_POSITION_HEADS: &[&str] = &[
    "do",
    "let",
    "let*",
    "binding",
    "when",
    "when-not",
    "when-let",
    "when-some",
    "when-first",
    "loop",
    "->",
    "->>",
    "some->",
    "some->>",
    "as->",
    "with-open",
    "with-local-vars",
    "with-redefs",
];

/// The subset of [`TAIL_POSITION_HEADS`] through which a *value* reaches the
/// tail.
///
/// This is what decides how much of a form counts as the tail's context when
/// asking whether it touches a resource, and getting it wrong breaks the rule
/// in one direction or the other:
///
/// - **Threading macros and binding forms widen.** In `(->> (line-seq r) (map
///   parse))` the resource is named in the *first* stage and the lazy call is
///   the last, so asking only about the last would never see `r` and the rule
///   would report nothing at all. Same for `(let [ls (line-seq r)] (map parse
///   ls))`.
/// - **`do`, `when` and `when-not` narrow.** Their leading forms are executed
///   for effect and hand nothing to the tail, so `(do (log r) (map inc [1 2
///   3]))` must be judged on `(map inc [1 2 3])` alone — widening here would
///   report correct code because `r` happens to be mentioned nearby.
pub const VALUE_CARRYING_TAIL_HEADS: &[&str] = &[
    "let",
    "let*",
    "binding",
    "when-let",
    "when-some",
    "when-first",
    "loop",
    "->",
    "->>",
    "some->",
    "some->>",
    "as->",
    "with-open",
    "with-local-vars",
    "with-redefs",
];

/// Whether `view` is a `(...)` call to a `clojure.core` operation that returns
/// an unrealized sequence.
#[must_use]
pub fn is_lazy_sequence_call(view: &ExpressionView) -> bool {
    head_is(view, LAZY_SEQUENCE_HEADS)
}

/// The threading macros, whose stages may name an operation as a bare symbol.
pub const THREADING_HEADS: &[&str] = &["->", "->>", "some->", "some->>"];

/// Whether nothing below `node` is still unrealized.
///
/// Two spellings, because a threading macro lets a stage be a bare symbol:
/// `(doall …)` is a call, and `(->> xs doall (map f))` is the same realization
/// written without parentheses. Missing the second would report
/// `(->> (line-seq r) doall (map parse))`, which is correct code.
fn is_realization_barrier(node: &ExpressionView) -> bool {
    if head_is(node, EAGER_REALIZER_HEADS) {
        return true;
    }
    head_is(node, THREADING_HEADS)
        && node.children.iter().skip(1).any(|stage| {
            symbol_name(stage).is_some_and(|name| EAGER_REALIZER_HEADS.contains(&name.as_str()))
        })
}

/// Whether `view` reaches one of `names` **without crossing an eager
/// realizer**.
///
/// This is `mentions_any_symbol` with the barrier described on
/// [`EAGER_REALIZER_HEADS`]: a resource mentioned only underneath a `doall` is
/// already in memory by the time the lazy wrapper is returned, so it does not
/// make the wrapper a defect.
#[must_use]
pub fn reaches_symbol_unrealized(view: &ExpressionView, names: &[String]) -> bool {
    if names.is_empty() {
        return false;
    }
    let mut found = false;
    for_each_evaluated_subview_where(
        view,
        // The barrier node itself is still visited — it is a list, so it can
        // carry no name — and nothing under it is.
        |node| !is_realization_barrier(node),
        |node| {
            if found {
                return;
            }
            if let Some(name) = symbol_name(node) {
                found = names.contains(&name);
            }
        },
    );
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::Clojure).expect("parse")
    }

    fn first_form(source: &str) -> ExpressionView {
        tree(source).root_view().children.remove(0)
    }

    fn evaluated_heads(source: &str) -> Vec<String> {
        let parsed = tree(source);
        let mut heads = Vec::new();
        for_each_evaluated_subview(&parsed.root_view(), |view| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
        });
        heads
    }

    // The quote model's own five shapes, pinned here so that each rule's tests
    // are testing the rule rather than re-testing this.

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(def x 1)").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(evaluated_heads("(quote (def x 1))"), vec!["quote"]);
    }

    /// Clojure reads `,` as whitespace, so `` `(a ,x) `` is `` `(a x) `` — all
    /// data. This is the shape a single `i32` depth counter gets wrong.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data_because_a_comma_is_whitespace() {
        assert!(evaluated_heads("'(a ,(def x 1))").is_empty());
        assert!(evaluated_heads("`(a ,(def x 1))").is_empty());
    }

    #[test]
    fn a_tilde_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(evaluated_heads("`(a ~(def x 1))"), vec!["def"]);
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(def x 1)").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(def x 1)\")"), vec!["f"]);
    }

    /// A reader lambda's body is code, and so are a set literal's elements.
    /// `HashLiteral` must stay neutral in `after_prefixes` or every `#(…)` in
    /// the file goes silently unexamined.
    #[test]
    fn a_hash_literal_prefix_does_not_make_its_contents_data() {
        assert_eq!(evaluated_heads("#(inc (f %))"), vec!["inc", "f"]);
        assert_eq!(evaluated_heads("#{(f 1)}"), vec!["f"]);
    }

    // `case` test constants: the one implicit quote in `clojure.core`, and the
    // false positive the corpus audit found in clj-kondo's own analyzer.

    /// A test constant may be a *list* of constants, which reads exactly like a
    /// call. `(case tag (def defonce) …)` is two symbols, not a `def`.
    #[test]
    fn a_case_test_constant_list_is_data_and_is_not_visited() {
        assert_eq!(
            evaluated_heads("(case tag (def defonce) (handle tag))"),
            vec!["case", "handle"]
        );
    }

    /// The result expressions beside those constants are ordinary code, and a
    /// fix that silenced them too would be over-suppression.
    #[test]
    fn a_case_result_expression_is_still_code() {
        assert_eq!(
            evaluated_heads("(case x (a b) (f 1) (c d) (g 2))"),
            vec!["case", "f", "g"]
        );
    }

    /// An odd body has a trailing default, which is a *result* and therefore
    /// code. Reading it as a test constant would silence a real finding.
    #[test]
    fn a_case_default_clause_is_code_not_a_test_constant() {
        assert_eq!(
            evaluated_heads("(case x (a b) (f 1) (fallback 2))"),
            vec!["case", "f", "fallback"]
        );
    }

    /// The dispatch expression is evaluated.
    #[test]
    fn a_case_dispatch_expression_is_code() {
        assert_eq!(
            evaluated_heads("(case (kind x) (a b) (f 1))"),
            vec!["case", "kind", "f"]
        );
    }

    /// A single-symbol test constant carries no list to mistake for a call, but
    /// the index arithmetic still has to place the results correctly.
    #[test]
    fn a_case_with_single_symbol_constants_keeps_its_results() {
        assert_eq!(
            evaluated_heads("(case x :a (f 1) :b (g 2) (h 3))"),
            vec!["case", "f", "g", "h"]
        );
    }

    #[test]
    fn the_case_test_positions_are_every_other_child_before_any_default() {
        // (case e t r)          -> 4 children, even body, test at 2
        assert!(is_case_test_position(4, 2));
        assert!(!is_case_test_position(4, 3));
        // (case e t r default)  -> 5 children, odd body, 4 is the default
        assert!(is_case_test_position(5, 2));
        assert!(!is_case_test_position(5, 3));
        assert!(!is_case_test_position(5, 4));
        // the head and the dispatch expression are never test constants
        assert!(!is_case_test_position(5, 0));
        assert!(!is_case_test_position(5, 1));
        // out of range
        assert!(!is_case_test_position(4, 9));
        assert!(!is_case_test_position(0, 0));
    }

    /// `condp` evaluates its test expressions, so it must **not** be treated
    /// like `case`. Over-suppression is the default failure mode of a fix like
    /// this one.
    #[test]
    fn condp_is_not_case_and_its_tests_stay_code() {
        assert_eq!(
            evaluated_heads("(condp = x (f 1) (g 2))"),
            vec!["condp", "f", "g"]
        );
    }

    /// `comment` is a macro whose expansion is `nil`: its body is read and then
    /// thrown away, so nothing in it runs. clj-kondo's own test suite asserts
    /// its `:inline-def` linter is silent on `(defn foo [] (comment (def x
    /// 1)))`, and the corpus audit found this package was not.
    #[test]
    fn a_comment_body_is_never_evaluated() {
        assert_eq!(evaluated_heads("(comment (def x 1))"), vec!["comment"]);
        assert_eq!(
            evaluated_heads("(defn f [] (comment (def x 1)))"),
            vec!["defn", "comment"]
        );
        assert_eq!(
            evaluated_heads("(comment (apply f [1 2]) (get-in m [:k]))"),
            vec!["comment"]
        );
    }

    /// The negative control: a head that merely *ends* in `comment`, and one
    /// that merely starts with it, are ordinary calls.
    #[test]
    fn only_comment_itself_drops_its_body() {
        assert_eq!(
            evaluated_heads("(uncomment (def x 1))"),
            vec!["uncomment", "def"]
        );
        assert_eq!(
            evaluated_heads("(comment-out (def x 1))"),
            vec!["comment-out", "def"]
        );
    }

    #[test]
    fn a_span_inside_a_comment_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "(defn f [] (comment (def x 1)))",
            "def"
        ));
    }

    /// The three non-evaluating forms, and the fact that nothing else joins
    /// them. `if` and `do` hold code in every position.
    #[test]
    fn the_child_evaluation_table_names_exactly_three_forms() {
        assert_eq!(ChildEvaluation::of("quote"), ChildEvaluation::None);
        assert_eq!(ChildEvaluation::of("comment"), ChildEvaluation::None);
        assert_eq!(
            ChildEvaluation::of("case"),
            ChildEvaluation::CaseTestConstants
        );
        for ordinary in ["if", "do", "let", "condp", "cond", "defn", "when"] {
            assert_eq!(
                ChildEvaluation::of(ordinary),
                ChildEvaluation::Evaluated,
                "{ordinary} holds code"
            );
        }
    }

    #[test]
    fn a_span_inside_a_case_test_constant_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "(case tag (def defonce) (handle tag))",
            "def"
        ));
        assert!(!unevaluated_at_first_head(
            "(case tag (a b) (def x 1))",
            "def"
        ));
    }

    #[test]
    fn a_pruned_walk_stops_below_the_node_it_refuses_to_descend() {
        let parsed = tree("(a (stop (b)) (c))");
        let mut heads = Vec::new();
        for_each_evaluated_subview_where(
            &parsed.root_view(),
            |view| list_head(view).is_none_or(|head| head != "stop"),
            |view| {
                if let Some(head) = list_head(view) {
                    heads.push(head.to_owned());
                }
            },
        );
        assert_eq!(heads, vec!["a", "stop", "c"]);
    }

    fn unevaluated_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head("'(def x 1)", "def"));
        assert!(unevaluated_at_first_head("(quote (def x 1))", "def"));
        assert!(unevaluated_at_first_head("`(def ~n 1)", "def"));
    }

    #[test]
    fn a_span_in_plain_code_or_under_an_unquote_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head("(defn f [] (def x 1))", "def"));
        assert!(!unevaluated_at_first_head("`(a ~(def x 1))", "def"));
    }

    /// A span that lands in the whitespace *between* two top-level forms names
    /// no node at all. `root_child_containing` must decline it rather than
    /// answering about whichever form happens to start before it.
    ///
    /// Found by mutation testing: both containment checks — the top-level one
    /// and the per-child one — could be deleted with the suite green, because
    /// every caller passes a span it took from the tree.
    #[test]
    fn a_span_that_names_no_top_level_form_is_not_reported_as_data() {
        // "'(a)\n\n(b)" — the gap after the quoted form is inside no form.
        let parsed = tree("'(a)\n\n(b)");
        let gap = ByteSpan::new(
            parsed.root_view().children[0].span.end(),
            parsed.root_view().children[1].span.start(),
        );
        assert!(!is_unevaluated_at(&parsed, gap));
    }

    /// A span *inside* a top-level form but covering no single child is judged
    /// by the innermost node that **contains** it — not by whichever sibling
    /// happens to start before it.
    ///
    /// The distinction is only observable when the preceding sibling is quoted
    /// and the span is not inside it: in `(f '(a) b)` the gap between `'(a)`
    /// and `b` is evaluated context, but the last child *starting* before it is
    /// the quoted list. Descending into that child without checking
    /// containment reports the gap as data.
    ///
    /// Found by mutation testing, and the first two attempts at this test did
    /// not distinguish the mutant — a gap inside an already-quoted form
    /// answers "data" either way.
    #[test]
    fn a_span_covering_no_single_child_is_judged_by_its_container_not_its_predecessor() {
        let parsed = tree("(f '(a) b)");
        let form = &parsed.root_view().children[0];
        let gap = ByteSpan::new(form.children[1].span.end(), form.children[2].span.start());
        assert!(
            !is_unevaluated_at(&parsed, gap),
            "the gap after a quoted sibling is still evaluated context"
        );

        // And the same shape where the container itself is data.
        let parsed = tree("'(f (a) b)");
        let form = &parsed.root_view().children[0];
        let gap = ByteSpan::new(form.children[1].span.end(), form.children[2].span.start());
        assert!(is_unevaluated_at(&parsed, gap));
    }

    // --- literal spellings ---------------------------------------------------

    /// How the dialect-aware reader actually lays out Clojure metadata, pinned
    /// because two rules here read a positional child and would read the wrong
    /// one otherwise.
    ///
    /// `^:m [1 2]` is **two sibling nodes** — an atom `^:m` carrying
    /// [`ReaderPrefix::Metadata`], then the vector — and *not* a vector
    /// carrying the prefix. So `(defn ^:private f [] …)` has `^:private` at
    /// child 1 and the function's name at child 2, which is why
    /// [`skip_metadata_forms`] exists.
    #[test]
    fn clojure_metadata_is_a_sibling_node_not_a_prefix_on_its_target() {
        let parsed = tree("^:m [1 2]");
        let children = &parsed.root_view().children;
        assert_eq!(children.len(), 2);
        assert!(is_metadata_form(&children[0]));
        assert!(!is_vector_literal(&children[0]));
        assert!(!is_metadata_form(&children[1]));
        assert!(is_vector_literal(&children[1]));
    }

    #[test]
    fn a_vector_literal_is_a_bracket_without_a_hash() {
        assert!(is_vector_literal(&first_form("[1 2 3]")));
        assert!(!is_vector_literal(&first_form("#{1 2}")));
        assert!(!is_vector_literal(&first_form("{:a 1}")));
        assert!(!is_vector_literal(&first_form("(list 1 2)")));
        assert!(!is_vector_literal(&first_form("x")));
    }

    /// The soundness guard: a set and a map have no defined argument order, so
    /// neither is a literal sequence a call can be rewritten from.
    #[test]
    fn only_vectors_and_quoted_lists_are_ordered_literal_sequences() {
        assert!(is_ordered_literal_sequence(&first_form("[1 2 3]")));
        assert!(is_ordered_literal_sequence(&first_form("'(1 2 3)")));
        assert!(!is_ordered_literal_sequence(&first_form("#{1 2 3}")));
        assert!(!is_ordered_literal_sequence(&first_form("{:a 1}")));
        assert!(!is_ordered_literal_sequence(&first_form("#(+ % 1)")));
        // Unquoted, `(1 2 3)` is a call to `1`, not a literal.
        assert!(!is_ordered_literal_sequence(&first_form("(1 2 3)")));
        assert!(!is_ordered_literal_sequence(&first_form("coll")));
    }

    #[test]
    fn a_deref_prefix_is_dropped_before_the_name_is_read() {
        let parsed = tree("(println @result)");
        let argument = &parsed.root_view().children[0].children[1];
        assert_eq!(symbol_name(argument).as_deref(), Some("result"));
    }

    /// The barrier, in both directions. A resource behind a `doall` is already
    /// in memory; a `reduce` sitting in a *sibling* position — inside the
    /// mapping function — realizes nothing about the resource and must not
    /// silence the rule.
    #[test]
    fn a_realizer_blocks_the_path_to_a_name_without_hiding_a_sibling_path() {
        let names = vec!["r".to_owned()];
        assert!(reaches_symbol_unrealized(
            &first_form("(map parse (line-seq r))"),
            &names
        ));
        assert!(!reaches_symbol_unrealized(
            &first_form("(map parse (doall (line-seq r)))"),
            &names
        ));
        assert!(!reaches_symbol_unrealized(
            &first_form("(map parse (into [] (line-seq r)))"),
            &names
        ));
        assert!(
            reaches_symbol_unrealized(&first_form("(map #(reduce + %) (line-seq r))"), &names),
            "a reduce inside the mapping function realizes nothing about r"
        );
        assert!(!reaches_symbol_unrealized(
            &first_form("(map parse (line-seq other))"),
            &names
        ));
    }

    #[test]
    fn the_lazy_vocabulary_excludes_the_eager_operations_it_recommends() {
        assert!(is_lazy_sequence_call(&first_form("(map inc xs)")));
        assert!(is_lazy_sequence_call(&first_form("(line-seq r)")));
        assert!(is_lazy_sequence_call(&first_form("(for [x xs] x)")));
        for eager in [
            "(into [] xs)",
            "(mapv inc xs)",
            "(filterv odd? xs)",
            "(doall xs)",
            "(reduce + xs)",
            "(vec xs)",
            "(set xs)",
            "(sort xs)",
            "(reverse xs)",
            "(frequencies xs)",
            "(group-by :k xs)",
        ] {
            assert!(
                !is_lazy_sequence_call(&first_form(eager)),
                "{eager} is eager"
            );
        }
    }
}
