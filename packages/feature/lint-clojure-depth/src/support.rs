//! What the Clojure-depth rules share: which parts of a file are *code*, where
//! a thread of control ends, and the four vocabularies the rules are written
//! against.
//!
//! # Evaluation context
//!
//! The quote machinery below (`QuoteState` — private — plus
//! `for_each_evaluated_subview_where` and [`is_unevaluated_at`]) is a
//! deliberate copy of `paredit-feature-lint-clojure-idiom`'s `support.rs`,
//! semantics included. It is copied rather than depended on because a feature
//! package must not depend on another feature package (§4.2), and it is copied
//! *exactly* because the two things a hand-rolled version keeps getting wrong
//! are subtle:
//!
//! - `'` and `` ` `` are not one counter. A comma inside `'(…)` is a comma
//!   character in a literal list — it does not escape back to code — so `hard`
//!   is a latch that never clears, while `quasi` counts up and down. A single
//!   `i32` depth would call `'(a ,x)` code.
//! - The verdict is read *at* the node, not one level up. A node one level
//!   inside a quote is still data even though it carries no reader prefix of
//!   its own, so checking a node's own `reader_prefixes` is not enough.
//!
//! Two forms hold unevaluated code with **no prefix at all** — `case`'s test
//! constants and `(comment …)`'s whole body. Both were found reporting false
//! positives on real Clojure by the sibling package's corpus audit, and both
//! matter here too: `(comment (go (>!! c v)))` is a scratch buffer, not a
//! defect.
//!
//! # Cost
//!
//! Every rule in this package calls [`is_unevaluated_at`], and every one calls
//! it **only after it already holds a candidate finding** — never as a
//! precondition on the head match. That ordering is the whole cost story:
//! [`is_unevaluated_at`] materializes the enclosing top-level form, so calling
//! it on every head match would cost that form's size once per candidate.
//!
//! `root_child_containing` binary-searches the top level with
//! [`SyntaxTree::root_child_span`], which is an index into a slice and a field
//! read. The obvious spelling — `select_path(&Path::root_child(i))?.span()` —
//! heap-allocates a `Vec<ChildIndex>` *per probe*, so the search alone would
//! cost `log2(forms)` allocations before the one materialization it needs.
//!
//! # A known gap in head normalization
//!
//! [`paredit_core_syntax::view_query::unqualified`] splits on `:`, the Common
//! Lisp package marker. Clojure's `/` namespace separator is *not* stripped,
//! so `async/go` does not normalize to `go` and neither the engine's head
//! index (`engine::head_index::head_key` returns a Clojure head verbatim) nor
//! [`symbol_in`] matches it.
//!
//! That is an engine-wide property, not something this package can fix
//! locally, and it costs this package more than it costs its siblings, because
//! `core.async` is conventionally aliased. It was measured rather than
//! guessed: over the corpus in this package's README the bare spellings `go`
//! and `go-loop` account for the large majority of call sites and the
//! alias-qualified spellings (`a/go`, `sp/go`, `cljs.core.async.macros/go`)
//! for the rest. The rules here therefore match the bare spelling; an
//! alias-qualified `go` block is a false negative, which is the direction this
//! package errs in throughout.

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
    /// turns code into data. Clojure's `@` deref also reads as
    /// [`ReaderPrefix::Function`] and is likewise neutral — `@x` is code. So is
    /// [`ReaderPrefix::HashLiteral`], which is how `#(…)` and `#{…}` are
    /// carried, and so is [`ReaderPrefix::ReaderConditional`]: every branch of
    /// a `#?(…)` is code the reader may select.
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
/// through `unqualified`, which is an `rsplit_once(':')` and therefore scans
/// the whole symbol, and this runs on **every list node** of every walk in this
/// package. A qualified spelling still ends in `quote`, so the pre-filter never
/// rejects something `symbol_is` would have accepted.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChildEvaluation {
    /// Ordinary code: every child is evaluated.
    Evaluated,
    /// Nothing below this form is evaluated — `(quote …)` and `(comment …)`.
    ///
    /// `comment` is a macro whose expansion is `nil`; its body is read, then
    /// discarded. A `(comment (go (>!! c v)))` scratch block interns nothing
    /// and runs nothing.
    None,
    /// `case`: the test constants are data, the result expressions are code.
    CaseTestConstants,
}

impl ChildEvaluation {
    fn of(head: &str) -> Self {
        if is_quote_head(head) || is_comment_head(head) {
            Self::None
        } else if is_case_head(head) {
            Self::CaseTestConstants
        } else {
            Self::Evaluated
        }
    }

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
/// `case` compares the test position against its expression as literal data,
/// so `(case tag (go thread) (handle tag))` is a two-element list of symbols
/// and **not** a `go` block.
///
/// The layout is `(case e t1 r1 t2 r2 … default?)`. Children 0 and 1 are the
/// head and the expression; the body is everything after. An **odd** body has
/// a trailing default, which is a result expression and therefore code — so
/// only the paired region holds test constants, at every other index from 2.
const fn is_case_test_position(child_count: usize, index: usize) -> bool {
    if index < 2 || index >= child_count {
        return false;
    }
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
/// Quoted subtrees are still *descended* — `` `(a ~(f)) `` has code inside
/// data — but their data nodes are never visited.
///
/// This is the walk each rule's `build_*_report` uses. The rules themselves
/// never call it: the dispatcher already hands `check` every head-matched
/// node, and walking again there would cost the file a second time.
pub fn for_each_evaluated_subview(root: &ExpressionView, mut visit: impl FnMut(&ExpressionView)) {
    let mut stack = vec![(root, QuoteState::EVALUATED)];
    while let Some((view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() {
            visit(view);
        }
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

/// The one child of `view` whose span covers `target`, found without reading
/// the others.
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
/// The probe is [`SyntaxTree::root_child_span`] rather than
/// `select_path(&Path::root_child(i))`, because `Path::root_child` allocates a
/// `Vec` and the search makes `log2(forms)` probes. Only the single form the
/// search lands on is materialized.
fn root_child_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let mut low = 0;
    let mut high = tree.root_children().len();
    while low < high {
        let middle = low + (high - low) / 2;
        if tree.root_child_span(middle)?.start().get() <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let index = low.checked_sub(1)?;
    let selection = tree.select_path(&Path::root_child(index)).ok()?;
    span_contains(selection.span(), target).then(|| selection.view())
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends to `target` through the one child at each level whose span
/// contains it, so the cost is the enclosing top-level form's size, never the
/// file's.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor
/// being data does not settle it: `` `(a ~(f)) `` has a quasiquoted ancestor
/// and an evaluated target.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let Some(top_level) = root_child_containing(tree, target) else {
        return false;
    };
    let mut view: &ExpressionView = &top_level;
    let mut state = QuoteState::EVALUATED.after_prefixes(view);

    while view.span != target {
        let evaluation = list_head(view).map_or(ChildEvaluation::Evaluated, ChildEvaluation::of);
        let child_count = view.children.len();
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
// Symbols and literal spellings
// ---------------------------------------------------------------------------

/// An atom's symbol text, past any reader prefix, lowercased.
///
/// Deliberately **not** `unqualified`: Clojure symbols are case-sensitive at
/// the reader level, but every core name these rules match is lowercase
/// anyway, and lowercasing keeps one spelling for the head index (which
/// lowercases nothing for Clojure) and for these comparisons to meet at.
#[must_use]
pub fn normalized_symbol(text: &str) -> String {
    text.to_ascii_lowercase()
}

/// The symbol an atom names, in the normalized spelling, or `None` for a
/// non-atom.
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

/// Whether `view` is a `[…]` vector literal.
///
/// The **delimiter alone** separates a vector from the two things it could be
/// confused with: `#{1 2}` is a `Brace` node and `#(…)` is a `Paren` node, so
/// neither reaches the bracket test.
#[must_use]
pub fn is_vector_literal(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Bracket)
}

/// Whether `view` is the reader's anonymous-function literal, `#(…)`.
///
/// A `(…)` list carrying [`ReaderPrefix::HashLiteral`]. `#{…}` carries the
/// same prefix but is a `Brace` node, and `#?(…)` — which *is* a paren list —
/// carries [`ReaderPrefix::ReaderConditional`] instead, which is why this is
/// not "a paren list with a hash on it".
///
/// The distinction is load-bearing for
/// [`crate::parking_op_outside_go_machinery`]: a reader conditional is not a
/// function body, and treating `(go #?(:clj (<! c)))` as one would report
/// correct code.
#[must_use]
pub fn is_reader_lambda(view: &ExpressionView) -> bool {
    is_paren_list(view) && view.reader_prefixes.contains(&ReaderPrefix::HashLiteral)
}

/// Whether `view` is a literal collection whose element **order is defined** —
/// a `[…]` vector or a `'(…)` quoted list.
#[must_use]
pub fn is_quoted_list_literal(view: &ExpressionView) -> bool {
    is_paren_list(view) && view.reader_prefixes.contains(&ReaderPrefix::Quote)
}

// ---------------------------------------------------------------------------
// core.async, checked against `clojure/core.async`
// ---------------------------------------------------------------------------

/// The two macros that open an IOC "thread of control".
///
/// `go-loop` is `(defmacro go-loop [bindings & body] `(go (loop ~bindings
/// ~@body)))` — `async.clj`:553-556 — so anything true of a `go` body is true
/// of a `go-loop` body.
pub const GO_HEADS: &[&str] = &["go", "go-loop"];

/// core.async's **blocking** operations, the `!!` family.
///
/// `<!!`, `>!!` and `alts!!` are the three `defblockingop` definitions in
/// `async.clj` (lines 161, 200, 345); `alt!!` is the macro over `alts!!`
/// (line 422), whose own docstring says it is "not intended for use in (go
/// ...) blocks".
pub const BLOCKING_CHANNEL_OPS: &[&str] = &["<!!", ">!!", "alt!!", "alts!!"];

/// core.async's **parking** operations.
///
/// Each is a plain `defn` whose entire body is an assertion that it was
/// rewritten away: `(assert nil "<! used not in (go ...) block")`
/// (`async.clj`:174-178, 213-218, 358-382). They exist only as a marker for
/// the `go` macro's state-machine transform to find, so reaching one at
/// runtime is always a defect.
pub const PARKING_CHANNEL_OPS: &[&str] = &["<!", ">!", "alt!", "alts!"];

/// Forms whose body becomes a **separate function object**, and therefore a
/// separate thread of control.
///
/// This is the single fact both `go` rules turn on, and every entry is
/// justified by a macroexpansion that puts an `fn*` around the body — so the
/// `go` macro's state-machine transform, which rewrites only the body it is
/// handed, cannot reach inside:
///
/// - `fn`, `fn*`, and the reader's `#(…)` (see [`is_reader_lambda`]).
/// - `thread`, `io-thread`, `thread-call` — `` `(thread-call (^:once fn* []
///   ~@body) :mixed) `` (`async.clj`:531-536). A `<!!` here is *correct*: it
///   is running on a real thread.
/// - `future`, `future-call`, `bound-fn`, `bound-fn*`.
/// - `delay`, `lazy-seq`, `lazy-cat`, `for` — each wraps its body in a thunk.
/// - `dosync`/`sync` — `` `(. LockingTransaction (runInTransaction (fn []
///   ~@body))) `` in `core.clj`.
/// - `reify`, `proxy` — every child is a method body.
///
/// **`doseq` is deliberately absent**, and the asymmetry is the point.
/// `doseq` expands to `loop`/`recur` with no `fn*` anywhere (`core.clj`:3240-
/// 3290), which is exactly why `(go (doseq [x xs] (>! c x)))` is the idiomatic
/// spelling and `(go (for [x xs] (<! x)))` is broken. `letfn` is absent for
/// the opposite reason: its *body* is in the enclosing scope, so calling the
/// whole form a boundary would report a parking op that is fine.
pub const THREAD_BOUNDARY_HEADS: &[&str] = &[
    "bound-fn",
    "bound-fn*",
    "delay",
    "dosync",
    "fn",
    "fn*",
    "for",
    "future",
    "future-call",
    "io-thread",
    "lazy-cat",
    "lazy-seq",
    "proxy",
    "reify",
    "sync",
    "thread",
    "thread-call",
];

/// Whether `view` opens a new thread of control — see
/// [`THREAD_BOUNDARY_HEADS`], plus the reader's `#(…)`, which carries no head
/// of its own.
#[must_use]
pub fn is_thread_boundary(view: &ExpressionView) -> bool {
    is_reader_lambda(view) || head_is(view, THREAD_BOUNDARY_HEADS)
}

/// Whether `view` opens a nested IOC block.
#[must_use]
pub fn is_go_block(view: &ExpressionView) -> bool {
    head_is(view, GO_HEADS)
}

/// How a reader lambda names itself in a finding: it has no head of its own.
pub const READER_LAMBDA_NAME: &str = "#()";

/// Visits every evaluated `(head …)` call under `root`, telling the visitor
/// which [`is_thread_boundary`] form — if any — lies between `root` and that
/// call.
///
/// The visitor answers whether to descend, which is how both `go` rules stop
/// at a **nested** `go`: that block is its own head match and reports its own
/// findings, so descending would report the inner block's defects twice.
///
/// The boundary is carried rather than pruned on, because the two rules want
/// opposite sides of it — one reports what is *before* a boundary, the other
/// what is *after* it — and a pruning walk can only express one. It is carried
/// as the boundary's **name** rather than as a flag so that the second rule's
/// message can say which form the parking op ended up inside, which is the
/// whole of the repair.
///
/// Childless nodes are never queued: an atom can carry no head, so it can
/// satisfy no caller, and not queueing them halves the stack traffic over a
/// function body.
pub fn for_each_call_across_boundaries<'a>(
    root: &'a ExpressionView,
    mut visit: impl FnMut(&'a ExpressionView, &'a str, Option<&'a str>) -> bool,
) {
    let mut stack: Vec<(&'a ExpressionView, QuoteState, Option<&'a str>)> = Vec::with_capacity(32);
    stack.push((root, QuoteState::EVALUATED, None));
    while let Some((view, outer, crossed)) = stack.pop() {
        let state = outer.after_prefixes(view);
        let head = list_head(view);
        // `#(<! %)` is **one node**: the reader lambda and the call it
        // contains are the same `(…)` list, whose head is `<!`. So the
        // boundary applies at the node itself and not only to its children —
        // unlike `(fn [] (<! c))`, where the call is a child. Getting this
        // wrong reports `#(<! %)` inside a `go` as being in the go body.
        let here = if crossed.is_none() && is_reader_lambda(view) {
            Some(READER_LAMBDA_NAME)
        } else {
            crossed
        };
        if !state.is_data() {
            if let Some(head) = head {
                if !visit(view, head, here) {
                    continue;
                }
            }
        }
        let inner = here.or_else(|| head.filter(|h| symbol_in(h, THREAD_BOUNDARY_HEADS)));
        let evaluation = head.map_or(ChildEvaluation::Evaluated, ChildEvaluation::of);
        let child_count = view.children.len();
        for (index, child) in view.children.iter().enumerate().rev() {
            if child.children.is_empty() {
                continue;
            }
            let child_state = if evaluation.is_data_child(child_count, index) {
                state.quoted()
            } else {
                state
            };
            stack.push((child, child_state, inner));
        }
    }
}

// ---------------------------------------------------------------------------
// Collections that are not associative, checked against `RT.java`
// ---------------------------------------------------------------------------

/// `clojure.core` operations whose result is an `ISeq` (or `nil`), and
/// therefore neither `Associative`, nor `IPersistentSet`, nor a
/// `java.util.Map`/`Set`.
///
/// This is exactly the set of shapes `RT.contains` (`RT.java`:824-848) falls
/// through to its final `throw new IllegalArgumentException("contains? not
/// supported on type: …")`. `nil` is handled by the first branch and answers
/// `false`, so the uniform claim across the whole list is that `contains?`
/// over one of these *can never answer true*.
///
/// `shuffle` and `split-at` are absent on purpose: `shuffle` returns a vector
/// (`(RT/vector (.toArray al))`) and `split-at` a two-element vector of seqs,
/// so neither reaches that `throw`.
pub const SEQ_PRODUCER_HEADS: &[&str] = &[
    "concat",
    "cons",
    "cycle",
    "dedupe",
    "distinct",
    "drop",
    "drop-last",
    "drop-while",
    "filter",
    "flatten",
    "interleave",
    "interpose",
    "iterate",
    "keep",
    "keep-indexed",
    "keys",
    "lazy-cat",
    "lazy-seq",
    "line-seq",
    "list",
    "list*",
    "map",
    "map-indexed",
    "mapcat",
    "next",
    "partition",
    "partition-all",
    "partition-by",
    "range",
    "re-seq",
    "remove",
    "repeat",
    "repeatedly",
    "rest",
    "reverse",
    "rseq",
    "seq",
    "sequence",
    "sort",
    "sort-by",
    "subseq",
    "take",
    "take-last",
    "take-nth",
    "take-while",
    "tree-seq",
    "vals",
    "xml-seq",
];

// ---------------------------------------------------------------------------
// The reference types, checked against `clojure/core.clj`
// ---------------------------------------------------------------------------

/// One of Clojure's four mutable reference containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    Atom,
    Ref,
    Agent,
    Volatile,
}

impl ReferenceKind {
    /// The constructor's own name, for the message.
    #[must_use]
    pub const fn constructor(self) -> &'static str {
        match self {
            Self::Atom => "atom",
            Self::Ref => "ref",
            Self::Agent => "agent",
            Self::Volatile => "volatile!",
        }
    }

    /// The reference kind a constructor call produces.
    #[must_use]
    pub fn of_constructor(head: &str) -> Option<Self> {
        match head {
            "atom" => Some(Self::Atom),
            "ref" => Some(Self::Ref),
            "agent" => Some(Self::Agent),
            "volatile!" => Some(Self::Volatile),
            _ => None,
        }
    }

    /// The operators this kind accepts, as a hint in the message.
    #[must_use]
    pub const fn operators(self) -> &'static str {
        match self {
            Self::Atom => "swap!/reset!/compare-and-set!",
            Self::Ref => "alter/commute/ref-set (inside dosync)",
            Self::Agent => "send/send-off",
            Self::Volatile => "vswap!/vreset!",
        }
    }
}

/// The mutation operators, each paired with the one reference kind it accepts.
///
/// Every entry is a `clojure.core` function whose first argument is type-
/// checked by the host: `swap!` takes `clojure.lang.IAtom`, `alter`/`commute`/
/// `ref-set`/`ensure` take `clojure.lang.Ref`, `vswap!`/`vreset!` take
/// `clojure.lang.Volatile`. Applying one to the wrong kind is a
/// `ClassCastException`, not a silent wrong answer.
///
/// **`send`, `send-off` and `send-via` are deliberately absent.** They are the
/// agent operators, but `send` is also an extremely ordinary name for a user
/// function over a connection or a socket, and this rule cannot tell the two
/// apart — `(let [conn (atom nil)] (send conn "hi"))` is correct code under
/// any number of libraries. An agent is therefore only ever detected here as
/// the *target* of an atom, ref or volatile operator, never by an operator of
/// its own. That is a false negative, deliberately taken.
pub const REFERENCE_OPERATORS: &[(&str, ReferenceKind)] = &[
    ("compare-and-set!", ReferenceKind::Atom),
    ("reset!", ReferenceKind::Atom),
    ("reset-vals!", ReferenceKind::Atom),
    ("swap!", ReferenceKind::Atom),
    ("swap-vals!", ReferenceKind::Atom),
    ("alter", ReferenceKind::Ref),
    ("commute", ReferenceKind::Ref),
    ("ensure", ReferenceKind::Ref),
    ("ref-set", ReferenceKind::Ref),
    ("vreset!", ReferenceKind::Volatile),
    ("vswap!", ReferenceKind::Volatile),
];

/// The reference kind an operator requires of its first argument.
#[must_use]
pub fn operator_reference_kind(head: &str) -> Option<ReferenceKind> {
    REFERENCE_OPERATORS
        .iter()
        .find(|(name, _)| *name == head)
        .map(|(_, kind)| *kind)
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

    // --- the quote model -----------------------------------------------------

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(go (>!! c v))").is_empty());
        assert!(evaluated_heads("`(go (>!! c v))").is_empty());
    }

    /// Clojure reads `,` as whitespace, so `` `(a ,x) `` is `` `(a x) `` — all
    /// data. This is the shape a single `i32` depth counter gets wrong.
    #[test]
    fn a_comma_inside_a_quote_stays_data_because_a_comma_is_whitespace() {
        assert!(evaluated_heads("'(a ,(go (>!! c v)))").is_empty());
        assert!(evaluated_heads("`(a ,(go (>!! c v)))").is_empty());
    }

    #[test]
    fn a_tilde_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(evaluated_heads("`(a ~(go x))"), vec!["go"]);
    }

    /// The one shape a single depth counter gets wrong, and the whole reason
    /// `hard` is a latch rather than a count.
    ///
    /// `'` and `` ` `` are different operators. A `~` inside a **hard** quote
    /// is not an escape back to code: the reader produces a literal
    /// `(clojure.core/unquote x)` form, which is still data, and `unquote`
    /// outside a syntax-quote is an error if it is ever evaluated. Modelling
    /// `'` as `quasi += 1` makes the `~` cancel it and calls the whole thing
    /// code.
    ///
    /// Found by mutation testing: replacing `hard = true` with `quasi += 1`
    /// left every other test in this package green.
    #[test]
    fn a_tilde_inside_a_hard_quote_does_not_escape_back_to_code() {
        assert!(evaluated_heads("'(a ~(go x))").is_empty());
        assert!(evaluated_heads("'(a ~@(go x))").is_empty());
        // …and nesting the other way round still escapes exactly once.
        assert_eq!(evaluated_heads("`(a ~(b ~c))"), vec!["b"]);
    }

    #[test]
    fn a_comment_body_is_never_evaluated() {
        assert_eq!(evaluated_heads("(comment (go (>!! c v)))"), vec!["comment"]);
    }

    #[test]
    fn a_case_test_constant_list_is_data_and_is_not_visited() {
        assert_eq!(
            evaluated_heads("(case tag (go thread) (handle tag))"),
            vec!["case", "handle"]
        );
    }

    #[test]
    fn the_case_test_positions_are_every_other_child_before_any_default() {
        assert!(is_case_test_position(4, 2));
        assert!(!is_case_test_position(4, 3));
        assert!(is_case_test_position(5, 2));
        assert!(!is_case_test_position(5, 3));
        assert!(!is_case_test_position(5, 4));
        assert!(!is_case_test_position(5, 0));
        assert!(!is_case_test_position(5, 1));
        assert!(!is_case_test_position(4, 9));
        assert!(!is_case_test_position(0, 0));
    }

    #[test]
    fn the_child_evaluation_table_names_exactly_three_forms() {
        assert_eq!(ChildEvaluation::of("quote"), ChildEvaluation::None);
        assert_eq!(ChildEvaluation::of("comment"), ChildEvaluation::None);
        assert_eq!(
            ChildEvaluation::of("case"),
            ChildEvaluation::CaseTestConstants
        );
        for ordinary in ["if", "do", "let", "condp", "cond", "go", "binding"] {
            assert_eq!(
                ChildEvaluation::of(ordinary),
                ChildEvaluation::Evaluated,
                "{ordinary} holds code"
            );
        }
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
        assert!(unevaluated_at_first_head("'(go x)", "go"));
        assert!(unevaluated_at_first_head("(quote (go x))", "go"));
        assert!(unevaluated_at_first_head("(comment (go x))", "go"));
    }

    #[test]
    fn a_span_in_plain_code_or_under_an_unquote_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head("(defn f [] (go x))", "go"));
        assert!(!unevaluated_at_first_head("`(a ~(go x))", "go"));
    }

    /// A span in the whitespace *between* two top-level forms names no node.
    /// `root_child_containing` must decline it rather than answering about
    /// whichever form happens to start before it.
    #[test]
    fn a_span_that_names_no_top_level_form_is_not_reported_as_data() {
        let parsed = tree("'(a)\n\n(b)");
        let gap = ByteSpan::new(
            parsed.root_view().children[0].span.end(),
            parsed.root_view().children[1].span.start(),
        );
        assert!(!is_unevaluated_at(&parsed, gap));
    }

    /// A span *inside* a top-level form but covering no single child is judged
    /// by the innermost node that **contains** it, not by whichever sibling
    /// happens to start before it.
    #[test]
    fn a_span_covering_no_single_child_is_judged_by_its_container() {
        let parsed = tree("(f '(a) b)");
        let form = &parsed.root_view().children[0];
        let gap = ByteSpan::new(form.children[1].span.end(), form.children[2].span.start());
        assert!(!is_unevaluated_at(&parsed, gap));

        let parsed = tree("'(f (a) b)");
        let form = &parsed.root_view().children[0];
        let gap = ByteSpan::new(form.children[1].span.end(), form.children[2].span.start());
        assert!(is_unevaluated_at(&parsed, gap));
    }

    /// The binary search must land on the *last* form beginning at or before
    /// the target, over a document with enough forms for the search to make
    /// more than one probe.
    #[test]
    fn the_top_level_search_finds_the_right_form_among_many() {
        let source: String = (0..32).map(|i| format!("(f{i} '(go x))\n")).collect();
        let parsed = tree(&source);
        for form in &parsed.root_view().children {
            let quoted = &form.children[1];
            assert!(is_unevaluated_at(&parsed, quoted.children[0].span));
        }
    }

    // --- literal spellings ---------------------------------------------------

    #[test]
    fn a_reader_lambda_is_a_paren_list_with_a_hash_and_nothing_else_is() {
        assert!(is_reader_lambda(&first_form("#(+ % 1)")));
        assert!(!is_reader_lambda(&first_form("#{1 2}")));
        assert!(!is_reader_lambda(&first_form("(fn [x] x)")));
        assert!(!is_reader_lambda(&first_form("[1 2]")));
    }

    /// The distinction that keeps `(go #?(:clj (<! c)))` quiet: a reader
    /// conditional is a paren list with a hash in the source and is **not** a
    /// function body.
    #[test]
    fn a_reader_conditional_is_not_a_reader_lambda() {
        let form = first_form("#?(:clj (inc 1) :cljs (inc 2))");
        assert!(!is_reader_lambda(&form));
        assert!(!is_thread_boundary(&form));
    }

    #[test]
    fn a_vector_literal_is_a_bracket_without_a_hash() {
        assert!(is_vector_literal(&first_form("[1 2 3]")));
        assert!(!is_vector_literal(&first_form("#{1 2}")));
        assert!(!is_vector_literal(&first_form("{:a 1}")));
        assert!(!is_vector_literal(&first_form("(list 1 2)")));
    }

    #[test]
    fn a_quoted_list_literal_needs_the_quote() {
        assert!(is_quoted_list_literal(&first_form("'(1 2 3)")));
        // Unquoted, `(1 2 3)` is a call to `1`, not a literal.
        assert!(!is_quoted_list_literal(&first_form("(1 2 3)")));
        assert!(!is_quoted_list_literal(&first_form("[1 2 3]")));
    }

    // --- thread boundaries ---------------------------------------------------

    /// The asymmetry the two `go` rules rest on: `for` builds a lazy sequence
    /// through an `fn*` and `doseq` does not.
    #[test]
    fn for_is_a_thread_boundary_and_doseq_is_not() {
        assert!(is_thread_boundary(&first_form("(for [x xs] x)")));
        assert!(!is_thread_boundary(&first_form("(doseq [x xs] x)")));
    }

    #[test]
    fn the_function_producing_forms_are_boundaries() {
        for source in [
            "(fn [x] x)",
            "(fn* [x] x)",
            "#(inc %)",
            "(thread (f))",
            "(future (f))",
            "(delay (f))",
            "(lazy-seq (f))",
            "(dosync (f))",
            "(reify P (m [_] 1))",
        ] {
            assert!(is_thread_boundary(&first_form(source)), "{source}");
        }
    }

    #[test]
    fn ordinary_control_forms_are_not_boundaries() {
        for source in [
            "(let [x 1] x)",
            "(when x (f))",
            "(loop [x 1] x)",
            "(try (f) (catch Exception e nil))",
            "(letfn [(g [] 1)] (g))",
            "(doseq [x xs] x)",
            "(locking o (f))",
        ] {
            assert!(!is_thread_boundary(&first_form(source)), "{source}");
        }
    }

    fn calls_with_boundary(source: &str) -> Vec<(String, Option<String>)> {
        let form = first_form(source);
        let mut found = Vec::new();
        for_each_call_across_boundaries(&form, |_, head, crossed| {
            found.push((head.to_owned(), crossed.map(ToOwned::to_owned)));
            true
        });
        found
    }

    fn call(head: &str, boundary: Option<&str>) -> (String, Option<String>) {
        (head.to_owned(), boundary.map(ToOwned::to_owned))
    }

    #[test]
    fn the_boundary_is_none_until_a_boundary_form_and_names_it_after() {
        assert_eq!(
            calls_with_boundary("(go (a) (fn [] (b)))"),
            vec![
                call("go", None),
                call("a", None),
                call("fn", None),
                call("b", Some("fn")),
            ]
        );
    }

    /// The **outermost** boundary is the one reported: a `fn` nested in a
    /// `thread` is still the `thread` that took the parking op out of the
    /// state machine, and naming the innermost would send a reader to the
    /// wrong form.
    #[test]
    fn the_outermost_boundary_is_the_one_carried() {
        assert_eq!(
            calls_with_boundary("(go (thread (fn [] (b))))"),
            vec![
                call("go", None),
                call("thread", None),
                call("fn", Some("thread")),
                call("b", Some("thread")),
            ]
        );
    }

    #[test]
    fn a_reader_lambda_is_a_boundary_at_its_own_node_having_no_head_of_its_own() {
        assert_eq!(
            calls_with_boundary("(go (map #(f %) xs))"),
            vec![
                call("go", None),
                call("map", None),
                call("f", Some(READER_LAMBDA_NAME)),
            ]
        );
    }

    #[test]
    fn declining_to_descend_stops_the_walk_below_that_node() {
        let form = first_form("(go (inner (deep)) (after))");
        let mut found = Vec::new();
        for_each_call_across_boundaries(&form, |_, head, _| {
            found.push(head.to_owned());
            head != "inner"
        });
        assert_eq!(found, vec!["go", "inner", "after"]);
    }

    #[test]
    fn quoted_calls_are_never_visited_by_the_boundary_walk() {
        assert_eq!(
            calls_with_boundary("(go '(a) (b))"),
            vec![call("go", None), call("b", None)]
        );
    }

    // --- reference types -----------------------------------------------------

    #[test]
    fn every_operator_names_a_kind_and_every_kind_names_a_constructor() {
        for (operator, kind) in REFERENCE_OPERATORS {
            assert_eq!(operator_reference_kind(operator), Some(*kind));
            assert!(!kind.operators().is_empty());
            assert_eq!(
                ReferenceKind::of_constructor(kind.constructor()),
                Some(*kind)
            );
        }
        assert_eq!(operator_reference_kind("deref"), None);
        assert_eq!(operator_reference_kind("send"), None);
        assert_eq!(operator_reference_kind("send-off"), None);
        assert_eq!(ReferenceKind::of_constructor("delay"), None);
    }

    #[test]
    fn the_four_constructors_map_to_the_four_kinds() {
        assert_eq!(
            ReferenceKind::of_constructor("atom"),
            Some(ReferenceKind::Atom)
        );
        assert_eq!(
            ReferenceKind::of_constructor("ref"),
            Some(ReferenceKind::Ref)
        );
        assert_eq!(
            ReferenceKind::of_constructor("agent"),
            Some(ReferenceKind::Agent)
        );
        assert_eq!(
            ReferenceKind::of_constructor("volatile!"),
            Some(ReferenceKind::Volatile)
        );
    }

    // --- vocabularies stay disjoint where they must --------------------------

    /// A blocking op and its parking twin differ by one character, and the
    /// whole of both `go` rules is that they are different operators.
    #[test]
    fn the_blocking_and_parking_vocabularies_are_disjoint() {
        for blocking in BLOCKING_CHANNEL_OPS {
            assert!(
                !PARKING_CHANNEL_OPS.contains(blocking),
                "{blocking} is in both lists"
            );
        }
        assert_eq!(BLOCKING_CHANNEL_OPS.len(), PARKING_CHANNEL_OPS.len());
    }

    /// Every list this package matches heads against must be sorted and free
    /// of duplicates, because a duplicate silently doubles a denominator and
    /// an unsorted list is how one gets added twice.
    #[test]
    fn every_head_vocabulary_is_sorted_and_free_of_duplicates() {
        for (name, heads) in [
            ("BLOCKING_CHANNEL_OPS", BLOCKING_CHANNEL_OPS),
            ("GO_HEADS", GO_HEADS),
            ("PARKING_CHANNEL_OPS", PARKING_CHANNEL_OPS),
            ("SEQ_PRODUCER_HEADS", SEQ_PRODUCER_HEADS),
            ("THREAD_BOUNDARY_HEADS", THREAD_BOUNDARY_HEADS),
        ] {
            let mut sorted = heads.to_vec();
            sorted.sort_unstable();
            assert_eq!(sorted, heads, "{name} is not sorted");
            sorted.dedup();
            assert_eq!(sorted.len(), heads.len(), "{name} has a duplicate");
        }
    }
}
