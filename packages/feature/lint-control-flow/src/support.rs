//! What the non-local-exit rules share: which parts of a file are *code*, and
//! which lexical contour a `return-from`/`return`/`go` actually sits in.
//!
//! Three things none of these rules can do without, and none of which the
//! engine or `core/syntax` provides:
//!
//! - **Evaluation context.** `'(return-from f 1)` is a list of symbols, not an
//!   exit. The lint engine's dispatch walks into quoted data like any other
//!   subtree and [`RuleContext`] carries no parent pointer, so a head-matched
//!   node cannot tell on its own whether it is code. [`is_unevaluated_at`]
//!   answers that for a matched form, and [`for_each_evaluated_subview`] for
//!   the inside of one.
//!
//! - **Ancestors.** "Does a `block` named `foo` enclose this `return-from`?"
//!   is not a per-node question, and [`RuleContext`] carries no parent
//!   pointer either. [`with_lexical_chain`] answers it by materializing only
//!   the *one* enclosing top-level form and descending to the target through
//!   the single chain of nodes whose span contains it.
//!
//! - **What a standard operator does to a block or a tag.** [`block_scope`]
//!   is a closed table over the Common Lisp standard operators. A head that
//!   is not in it is [`BlockScope::Unknown`], and every rule here stops there
//!   rather than guessing: `(with-retry (return-from retry 1))` is a
//!   perfectly good program if `with-retry` expands to a `block`, and this
//!   analysis cannot see macro expansions. Preferring a false negative to a
//!   false positive is the whole design of that table.
//!
//! # Quote semantics
//!
//! [`QuoteState`] and [`for_each_evaluated_subview`] are copied from
//! `paredit-feature-lint-condition-system`'s `support.rs` (and its copy in
//! `paredit-feature-lint-testing`), tests included, deliberately as a copy
//! rather than as a dependency: a lint feature package depending on another
//! lint feature package would be a new feature→feature edge for a hundred
//! lines of traversal.
//!
//! The two counters are not one depth number. A comma inside `'(…)` is a comma
//! character in a literal list, so `hard` never clears; a comma inside `` `(…) ``
//! escapes back to code, so `quasi` counts up and down.
//!
//! # Cost
//!
//! Nothing here runs per visited node. Every rule in this package declares
//! [`HeadFilter::Heads`], so all of this is paid only once one of
//! `return-from`/`return`/`go`/`tagbody`/`block`/`multiple-value-bind`/
//! `dotimes`/`dolist` has already matched — which, in the `clean/forms/*`
//! benchmarks that lint files with no findings, is never.
//!
//! [`with_lexical_chain`] never calls `SyntaxTree::root_view`, which builds an
//! `ExpressionView` for *every node in the file* and would make a file of N
//! matched heads cost N×file. It binary-searches the root's children by span
//! instead and materializes the single top-level form the target is in.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext
//! [`HeadFilter::Heads`]: paredit_core_lint_engine::model::HeadFilter::Heads

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_in, unqualified,
};

// ---------------------------------------------------------------------------
// Evaluation context
// ---------------------------------------------------------------------------

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
    /// `#'`, `#.`, `#+`, metadata and the rest are deliberately neutral: none
    /// of them turns code into data.
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

/// The long-hand `(quote …)`, which the reader also produces for `'…` but which
/// hand-written code and macro output both spell out.
fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_in(head, &["quote"]))
}

/// Whether `outer` covers every byte of `inner`. Equal spans contain each
/// other, so a caller that means "strictly inside" compares the spans too.
#[must_use]
pub const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Calls `visit` on every node of `root` that is reachable as evaluated code,
/// in the same pre-order the lint engine's own walk produces.
///
/// Quoted subtrees are still *descended* — `` `(a ,(f)) `` has code inside data
/// — but their data nodes are never visited.
pub fn for_each_evaluated_subview(root: &ExpressionView, visit: impl FnMut(&ExpressionView)) {
    for_each_evaluated_subview_where(root, |_| true, visit);
}

/// [`for_each_evaluated_subview`], with a say in where the walk stops.
///
/// `descend` is asked about each visited node *before* its children are queued;
/// answering `false` visits that node and nothing under it. That is how
/// `block-name-shadows-outer-block` stops at the first nested `block` of the
/// same name (the one *inside* that is the inner block's own finding, not
/// this one's), and how `dotimes-dolist-index-var-mutated` stops at a form
/// that rebinds the iteration variable.
///
/// A pruned node's subtree is skipped *including* any quoted data in it, which
/// is correct for both of those and would not be if this were used to find data.
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
        let inside = if is_quote_form(view) {
            state.quoted()
        } else {
            state
        };
        for child in view.children.iter().rev() {
            stack.push((child, inside));
        }
    }
}

/// The one child of `view` whose span covers `target`, found without reading
/// the others.
///
/// A node's children are in document order and do not overlap, so the only
/// child that can contain `target` is the last one beginning at or before it —
/// which a binary search finds in `log₂ k` comparisons instead of `k`.
fn child_containing(view: &ExpressionView, target: ByteSpan) -> Option<&ExpressionView> {
    let after = view
        .children
        .partition_point(|child| child.span.start().get() <= target.start().get());
    let child = view.children.get(after.checked_sub(1)?)?;
    span_contains(child.span, target).then_some(child)
}

/// The top-level form containing `target`, materialized on its own.
///
/// The reason this is not `tree.root_view()` followed by a search: `root_view`
/// builds an `ExpressionView` — a `Vec` of children and a `Vec` of reader
/// prefixes — for *every node in the file*, so asking it about one node costs
/// the whole document, and these rules ask once per matched head. Selecting
/// the one root child instead costs a binary search over the top level — each
/// step a node-id lookup and a span read, neither of which allocates — plus
/// that one form's own subtree.
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

/// The nodes enclosing one matched form, and whether that form is data.
///
/// `nodes` runs outermost first — `nodes[0]` is the enclosing top-level form —
/// and its last element is the target itself, so the strict ancestors are
/// `nodes[..nodes.len() - 1]`.
#[derive(Debug)]
pub struct LexicalChain<'a> {
    pub nodes: Vec<&'a ExpressionView>,
    /// Whether the target is unevaluated data (inside `'(…)`, `(quote …)`, or
    /// an unescaped `` `(…) ``) rather than code.
    pub unevaluated: bool,
    /// Whether the target is inside a *hard* quote — `'(…)` or `(quote …)` —
    /// as opposed to a quasiquote template.
    ///
    /// The half of [`unevaluated`](Self::unevaluated) that a *rewrite* may act
    /// on. A `` `(…) `` template is a template of code and its contents really
    /// do get evaluated, so suppressing on the quasi half buys false negatives;
    /// a `'(…)` list is data in every expansion, so a rewrite of its contents
    /// is always a change to the program's data.
    pub hard_quoted: bool,
}

impl<'a> LexicalChain<'a> {
    /// The target's strict ancestors, innermost first — the order every scope
    /// question here is asked in.
    pub fn ancestors_inward(&self) -> std::iter::Rev<std::ops::Range<usize>> {
        (0..self.nodes.len().saturating_sub(1)).rev()
    }
}

/// Runs `f` with the chain of nodes from the enclosing top-level form down to
/// the node at `target`.
///
/// `None` when `target` lies inside no top-level form at all, which a span
/// taken from the tree never does.
///
/// Borrowed rather than returned because the chain borrows the one top-level
/// `ExpressionView` this materializes, which is a local.
pub fn with_lexical_chain<R>(
    tree: &SyntaxTree,
    target: ByteSpan,
    f: impl FnOnce(&LexicalChain<'_>) -> R,
) -> Option<R> {
    let top_level = root_child_containing(tree, target)?;
    let mut view: &ExpressionView = &top_level;
    // The root carries no reader prefix and is not a `(quote …)` form, so the
    // state entering the top-level form is whatever that form's own prefixes
    // say.
    let mut state = QuoteState::EVALUATED.after_prefixes(view);
    let mut nodes: Vec<&ExpressionView> = vec![view];

    while view.span != target {
        let quoting = is_quote_form(view);
        // A span that names no node is judged by the innermost node that
        // contains it, which is the honest answer for a span the caller
        // synthesized rather than took from the tree.
        let Some(child) = child_containing(view, target) else {
            break;
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
        nodes.push(child);
    }

    Some(f(&LexicalChain {
        nodes,
        unevaluated: state.is_data(),
        hard_quoted: state.hard,
    }))
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(return-from f 1)) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it, and
/// that is already modelled by `hard` never clearing.
///
/// Called at most once per matched head, never per visited node.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    with_lexical_chain(tree, target, |chain| chain.unevaluated).unwrap_or(false)
}

/// Whether the node at `target` sits inside a hard quote — `'(…)` or
/// `(quote …)` — and is therefore literal data in every expansion.
///
/// The guard a *rewriting* rule wants, where [`is_unevaluated_at`] is the guard
/// a *reporting* rule wants. The difference is the quasiquote: `` `(progn ,x) ``
/// is a template whose `progn` really is emitted as code, so a rule that
/// suppressed itself there would go quiet on the macro bodies it exists to
/// read; `'(progn a b)` is a three-element list and rewriting it edits the
/// program's data.
///
/// Costs one binary search over the top level plus one descent through the
/// enclosing top-level form — never [`SyntaxTree::root_view`] — and is meant to
/// be called only once a rule already holds a finding, so a file with no
/// findings never pays for it.
#[must_use]
pub fn is_hard_quoted_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    with_lexical_chain(tree, target, |chain| chain.hard_quoted).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

/// An atom's symbol text, past any reader prefix, lowercased and stripped of
/// its package qualifier — the spelling every comparison here is written in.
#[must_use]
pub fn normalized_symbol(view: &ExpressionView) -> Option<String> {
    atom_symbol_text(view)
        .filter(|text| !text.is_empty())
        .map(|text| unqualified(text).to_ascii_lowercase())
}

/// A string literal, which the reader keeps as one atom including its quotes.
///
/// This is what keeps every rule here out of string contents: `"(go done)"` is
/// this atom and has no children, so no walk can reach a form inside it.
#[must_use]
pub fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

/// A reader-conditional atom (`#+feature`/`#-feature`), which reads together
/// with the form that follows it and so is neither a tag nor a body form on its
/// own.
#[must_use]
pub fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// A plain symbol used as a name: an atom with no reader prefix, not a string,
/// not a number, and not a keyword.
///
/// The reader-prefix test is what keeps `'foo` out: `(block 'foo …)` names no
/// block this analysis can read, and `(return-from 'foo)` is malformed rather
/// than a reference to a block called `foo`.
#[must_use]
pub fn plain_name(view: &ExpressionView) -> Option<String> {
    if !view.reader_prefixes.is_empty() || is_string_literal(view) {
        return None;
    }
    let name = normalized_symbol(view)?;
    if name.starts_with(':') || name.parse::<f64>().is_ok() {
        return None;
    }
    Some(name)
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

/// A `tagbody` tag: a symbol or an integer, in the spelling tags are compared
/// in.
///
/// CLHS 5.3 `tagbody`: "the tags are … integers or symbols", and `go` matches
/// its argument against them with `eql`. Integers are therefore compared
/// numerically — `007` and `7` are one tag — and symbols case-insensitively,
/// since the reader upcases them. A string, a float, or a list is not a tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tag {
    Integer(i64),
    Symbol(String),
}

impl Tag {
    /// The tag an atom names, or `None` for a node that is not a tag at all.
    #[must_use]
    pub fn read(view: &ExpressionView) -> Option<Self> {
        if !view.reader_prefixes.is_empty() || is_string_literal(view) {
            return None;
        }
        if !view.children.is_empty() {
            return None;
        }
        Self::from_text(atom_symbol_text(view).filter(|text| !text.is_empty())?)
    }

    /// The tag an atom *mentions*, reader prefixes and all.
    ///
    /// [`Self::read`] refuses a prefixed atom because `(go 'retry)` names no
    /// tag and `(tagbody 'retry)` labels nothing. This is the other question —
    /// "does this name appear here at all" — which `tagbody-unreachable-tag`
    /// asks to decide whether a macro might be turning the name into a `go`,
    /// and for which `'retry` counts.
    #[must_use]
    pub fn mention(view: &ExpressionView) -> Option<Self> {
        if is_string_literal(view) || !view.children.is_empty() {
            return None;
        }
        Self::from_text(atom_symbol_text(view).filter(|text| !text.is_empty())?)
    }

    /// The tag a piece of symbol text names, past the syntactic questions.
    fn from_text(text: &str) -> Option<Self> {
        if let Ok(value) = text.parse::<i64>() {
            return Some(Self::Integer(value));
        }
        // A float is neither an integer nor a symbol, and `eql` on floats is
        // not something a lint should reason about.
        if text.parse::<f64>().is_ok() {
            return None;
        }
        Some(Self::Symbol(unqualified(text).to_ascii_lowercase()))
    }

    /// How the tag reads in a report.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::Integer(value) => value.to_string(),
            Self::Symbol(name) => name.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// The standard operator table
// ---------------------------------------------------------------------------

/// Standard Common Lisp operators that establish no block of their own, and
/// whose body this analysis may therefore walk *through* on its way outward.
///
/// Only standard operators are listed. A conforming program may not redefine
/// any of them (CLHS 11.1.2.1.2), so what each one does to a lexical block is
/// fixed and knowable; a name outside this table is a user function or — the
/// case that matters — a user macro, which may expand to anything at all.
///
/// Function names are here for the same reason macro names are: `(list 1
/// (return-from f 2))` puts a `return-from` under a `list` call, and refusing
/// to walk through `list` would mean refusing to diagnose it. Sorted, because
/// [`is_transparent_operator`] binary-searches it; `the_table_is_sorted` is the
/// test that keeps it that way.
const TRANSPARENT_OPERATORS: &[&str] = &[
    "*",
    "+",
    "-",
    "/",
    "/=",
    "1+",
    "1-",
    "<",
    "<=",
    "=",
    ">",
    ">=",
    "abs",
    "acons",
    "adjoin",
    "and",
    "append",
    "apply",
    "aref",
    "assert",
    "assoc",
    "atom",
    "boundp",
    "butlast",
    "car",
    "case",
    "catch",
    "ccase",
    "cdr",
    "cerror",
    "char=",
    "check-type",
    "coerce",
    "concatenate",
    "cond",
    "cons",
    "consp",
    "copy-list",
    "copy-seq",
    "count",
    "ctypecase",
    "decf",
    "declaim",
    "declare",
    "defclass",
    "defconstant",
    "define-condition",
    "define-modify-macro",
    "define-symbol-macro",
    "defpackage",
    "defparameter",
    "defstruct",
    "deftype",
    "defvar",
    "delete",
    "destructuring-bind",
    "ecase",
    "elt",
    "endp",
    "eq",
    "eql",
    "equal",
    "equalp",
    "error",
    "etypecase",
    "eval-when",
    "every",
    "expt",
    "find",
    "find-if",
    "first",
    "floor",
    "format",
    "funcall",
    "function",
    "gethash",
    "handler-bind",
    "handler-case",
    "identity",
    "if",
    "ignore-errors",
    "in-package",
    "incf",
    "intern",
    "lambda",
    "last",
    "length",
    "let",
    "let*",
    "list",
    "list*",
    "listp",
    "locally",
    "make-array",
    "make-hash-table",
    "make-instance",
    "make-string",
    "map",
    "mapc",
    "mapcan",
    "mapcar",
    "maphash",
    "max",
    "member",
    "min",
    "mod",
    "multiple-value-bind",
    "multiple-value-call",
    "multiple-value-list",
    "multiple-value-prog1",
    "multiple-value-setq",
    "not",
    "notany",
    "notevery",
    "nreverse",
    "nth",
    "nth-value",
    "nthcdr",
    "null",
    "numberp",
    "or",
    "pop",
    "position",
    "prin1",
    "princ",
    "print",
    "prog1",
    "prog2",
    "progn",
    "progv",
    "psetf",
    "psetq",
    "push",
    "pushnew",
    "quote",
    "reduce",
    "rem",
    "remove",
    "rest",
    "restart-bind",
    "restart-case",
    "return",
    "return-from",
    "reverse",
    "rotatef",
    "round",
    "second",
    "setf",
    "setq",
    "shiftf",
    "signal",
    "some",
    "sort",
    "sqrt",
    "string",
    "string-equal",
    "string=",
    "subseq",
    "svref",
    "symbol-macrolet",
    "symbol-name",
    "symbol-value",
    "tagbody",
    "terpri",
    "the",
    "third",
    "throw",
    "truncate",
    "typecase",
    "typep",
    "unless",
    "unwind-protect",
    "values",
    "values-list",
    "vector",
    "warn",
    "when",
    "with-accessors",
    "with-compilation-unit",
    "with-condition-restarts",
    "with-hash-table-iterator",
    "with-input-from-string",
    "with-open-file",
    "with-open-stream",
    "with-output-to-string",
    "with-package-iterator",
    "with-simple-restart",
    "with-slots",
    "with-standard-io-syntax",
    "write",
    "write-string",
    "zerop",
];

/// The standard iteration operators that establish the implicit `nil` block.
///
/// CLHS says so in as many words for each: `do`/`do*` ("an implicit block named
/// nil"), `dolist`, `dotimes`, `prog`/`prog*`, and the three `do-…-symbols`
/// macros. `loop` also does, but its block may be *renamed* by a `named`
/// clause, so it is handled separately in [`block_scope`].
///
/// `mapcar`, `handler-case`, `case` and friends are deliberately absent: none
/// of them establishes a block named `nil`, and putting one here would silence
/// a real `(return …)` defect.
const NIL_BLOCK_OPERATORS: [&str; 9] = [
    "do",
    "do*",
    "do-all-symbols",
    "do-external-symbols",
    "do-symbols",
    "dolist",
    "dotimes",
    "prog",
    "prog*",
];

/// The definition forms whose body is an implicit block named after the thing
/// being defined (CLHS 3.1.2.1.2.2 / `defun` / `defmacro` / `defmethod`).
const NAMED_DEFINITION_OPERATORS: [&str; 4] =
    ["defun", "defmacro", "defmethod", "define-compiler-macro"];

/// The local-function forms, each of whose bindings' bodies is an implicit
/// block named after that binding.
const LOCAL_FUNCTION_OPERATORS: [&str; 3] = ["flet", "labels", "macrolet"];

/// Whether `head` is a standard operator this analysis may walk through.
#[must_use]
pub fn is_transparent_operator(head: &str) -> bool {
    TRANSPARENT_OPERATORS.binary_search(&head).is_ok()
}

/// What one enclosing form does to the lexical blocks visible inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockScope {
    /// A standard operator that establishes no block.
    Transparent,
    /// Establishes exactly one block, with this name. The implicit `nil` block
    /// is `Named("nil")`, because `(return x)` is `(return-from nil x)`.
    Named(String),
    /// `flet`/`labels`/`macrolet`: one block per local function name.
    LocalFunctions(Vec<String>),
    /// Not a standard operator, so what it establishes cannot be known. Every
    /// caller stops its walk here rather than guessing.
    Unknown,
}

/// The name a `defun`/`defmethod`/`defmacro` defines, including the
/// `(setf foo)` spelling, whose block name is `foo` (CLHS `defun`).
fn definition_block_name(view: &ExpressionView) -> Option<String> {
    let name = view.children.get(1)?;
    if let Some(plain) = plain_name(name) {
        return Some(plain);
    }
    if is_paren_list(name) && list_head(name).is_some_and(|head| symbol_in(head, &["setf"])) {
        return name.children.get(1).and_then(plain_name);
    }
    None
}

/// The block a `loop` establishes.
///
/// `(loop named outer …)` names its block `outer` *instead of* `nil` (CLHS
/// 6.1.1.4), so a `(return …)` inside such a loop exits nothing it can see.
/// Getting this wrong in either direction is a false positive: reading every
/// loop as `nil` misses that case, and reading none as `nil` flags every
/// ordinary `(loop … (return x))`.
fn loop_block_name(view: &ExpressionView) -> String {
    let named = view
        .children
        .get(1)
        .and_then(plain_name)
        .is_some_and(|first| first == "named");
    if named {
        if let Some(name) = view.children.get(2).and_then(plain_name) {
            return name;
        }
    }
    "nil".to_owned()
}

/// What the node at `index` of `chain` does to the blocks visible at the
/// target.
///
/// Takes the whole chain because one case is not answerable from the node
/// alone: `(flet ((helper () …)) …)` puts the binding `(helper () …)` in the
/// chain, whose head `helper` is not an operator at all — it is a local
/// function name, and its body *is* a block named `helper`. That is only
/// visible from the grandparent.
#[must_use]
pub fn block_scope(chain: &[&ExpressionView], index: usize) -> BlockScope {
    let Some(view) = chain.get(index) else {
        return BlockScope::Unknown;
    };
    // A list whose head is itself a list — a `cond` clause, a `let` binding
    // list, an `flet` binding list — establishes nothing on its own. Whether
    // walking through it is safe is settled by its parent, which is in the
    // chain too and is classified on its own terms.
    let Some(head) = list_head(view) else {
        return BlockScope::Transparent;
    };
    let head = unqualified(head).to_ascii_lowercase();

    if head == "block" {
        return view
            .children
            .get(1)
            .and_then(plain_name)
            .map_or(BlockScope::Transparent, BlockScope::Named);
    }
    if head == "loop" {
        return BlockScope::Named(loop_block_name(view));
    }
    if NIL_BLOCK_OPERATORS.contains(&head.as_str()) {
        return BlockScope::Named("nil".to_owned());
    }
    if NAMED_DEFINITION_OPERATORS.contains(&head.as_str()) {
        return definition_block_name(view).map_or(BlockScope::Transparent, BlockScope::Named);
    }
    if LOCAL_FUNCTION_OPERATORS.contains(&head.as_str()) {
        let names = view
            .children
            .get(1)
            .map(|bindings| {
                bindings
                    .children
                    .iter()
                    .filter_map(|binding| binding.children.first().and_then(plain_name))
                    .collect()
            })
            .unwrap_or_default();
        return BlockScope::LocalFunctions(names);
    }
    if is_transparent_operator(&head) {
        return BlockScope::Transparent;
    }
    if is_local_function_definition(chain, index) {
        return BlockScope::Named(head);
    }
    BlockScope::Unknown
}

/// Whether `chain[index]` is one of the bindings of an enclosing
/// `flet`/`labels`/`macrolet` — the shape whose head is a *definition* name
/// rather than a call.
fn is_local_function_definition(chain: &[&ExpressionView], index: usize) -> bool {
    let Some(bindings) = index.checked_sub(1).and_then(|at| chain.get(at)) else {
        return false;
    };
    let Some(form) = index.checked_sub(2).and_then(|at| chain.get(at)) else {
        return false;
    };
    let is_local_form =
        list_head(form).is_some_and(|head| symbol_in(head, &LOCAL_FUNCTION_OPERATORS));
    is_local_form
        && form
            .children
            .get(1)
            .is_some_and(|list| list.span == bindings.span)
}

/// Where the body of an implicit-`tagbody` form starts, for the forms whose
/// body is one.
///
/// CLHS: `tagbody`'s own body starts right after the head; `prog`/`prog*` after
/// the binding list; `do`/`do*` after the binding list and the end-test clause;
/// `dolist`/`dotimes` and the three `do-…-symbols` macros after their spec.
/// `loop` is absent on purpose — a `loop` body is a sequence of clauses, not a
/// tagbody, and no `go` may target anything in it.
#[must_use]
pub fn tagbody_body_start(head: &str) -> Option<usize> {
    let head = unqualified(head).to_ascii_lowercase();
    match head.as_str() {
        "tagbody" => Some(1),
        "prog" | "prog*" => Some(2),
        "do" | "do*" => Some(3),
        "dolist" | "dotimes" | "do-symbols" | "do-external-symbols" | "do-all-symbols" => Some(2),
        _ => None,
    }
}

/// The tags a form's own implicit tagbody establishes, in source order.
///
/// Only the form's *own* body: a tag inside a nested `tagbody` belongs to that
/// one, and `go` resolves to the innermost enclosing tagbody that has the tag.
#[must_use]
pub fn direct_tags(view: &ExpressionView) -> Vec<(Tag, ByteSpan)> {
    let Some(start) = list_head(view).and_then(tagbody_body_start) else {
        return Vec::new();
    };
    view.children
        .iter()
        .skip(start)
        .filter(|child| child.children.is_empty() && !is_reader_conditional(child))
        .filter_map(|child| Tag::read(child).map(|tag| (tag, child.span)))
        .collect()
}

/// Runs one rule end to end through the real lint engine and returns, per
/// finding, its message and the source that applying its fix produces.
///
/// A domain test cannot see either of the two things that actually broke these
/// rules on real code. It cannot catch a wrong [`HeadFilter::Heads`] list — a
/// rule that declares the wrong head compiles, passes every domain test, and is
/// never invoked. And it cannot catch a fix whose *span* is wrong, because the
/// domain never applies one: a replacement that starts one byte early deletes
/// the form's reader prefix, and only the spliced source shows it. So this
/// returns the rewritten text, not just the finding.
///
/// [`HeadFilter::Heads`]: paredit_core_lint_engine::model::HeadFilter::Heads
#[cfg(test)]
#[must_use]
pub fn run_rule_fixed(
    entries: &'static [paredit_core_lint_engine::rule::RuleEntry],
    source: &str,
) -> Vec<(String, String)> {
    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::RuleCatalog;
    use paredit_core_syntax::dialect::Dialect;

    let catalog = RuleCatalog::new(entries);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    collect_lint_outcomes(
        catalog,
        &index,
        std::path::Path::new("app.lisp"),
        Dialect::CommonLisp,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("lint pass")
    .into_iter()
    .map(|outcome| {
        let (finding, fix) = outcome.into_parts();
        let mut fixed = source.to_owned();
        if let Some(fix) = fix {
            // Highest offset first, so an earlier edit's span stays valid.
            let mut edits: Vec<_> = fix.replacements().collect();
            edits.sort_by_key(|edit| std::cmp::Reverse(edit.span().start().get()));
            for edit in edits {
                fixed.replace_range(
                    edit.span().start().get()..edit.span().end().get(),
                    edit.text(),
                );
            }
        }
        (finding.message, fixed)
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn parse(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    /// The span of the first `(head …)` form in the file, found with the same
    /// evaluated-blind walk the engine's dispatch uses.
    fn span_of(tree: &SyntaxTree, head: &str) -> ByteSpan {
        let root = tree.root_view();
        let mut found = None;
        paredit_core_syntax::view_query::for_each_subview(&root, |view| {
            if found.is_none() && list_head(view).is_some_and(|name| symbol_in(name, &[head])) {
                found = Some(view.span);
            }
        });
        found.expect("a form with that head")
    }

    #[test]
    fn the_table_is_sorted() {
        let mut sorted = TRANSPARENT_OPERATORS.to_vec();
        sorted.sort_unstable();
        assert_eq!(TRANSPARENT_OPERATORS, sorted.as_slice());
    }

    #[test]
    fn the_table_has_no_duplicates() {
        let mut sorted = TRANSPARENT_OPERATORS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), TRANSPARENT_OPERATORS.len());
    }

    /// The tables must not overlap: a head classified as transparent would
    /// never reach the block-establishing branches at all.
    #[test]
    fn no_block_establishing_operator_is_also_transparent() {
        for head in NIL_BLOCK_OPERATORS
            .iter()
            .chain(NAMED_DEFINITION_OPERATORS.iter())
            .chain(LOCAL_FUNCTION_OPERATORS.iter())
            .chain(["block", "loop"].iter())
        {
            assert!(
                !is_transparent_operator(head),
                "{head} is in two tables at once"
            );
        }
    }

    // -- quote state --------------------------------------------------------

    #[test]
    fn bare_code_is_evaluated() {
        let tree = parse("(defun f () (block b 1))");
        assert!(!is_unevaluated_at(&tree, span_of(&tree, "block")));
    }

    #[test]
    fn a_hard_quoted_form_is_data() {
        let tree = parse("'(block b 1)");
        assert!(is_unevaluated_at(&tree, span_of(&tree, "block")));
    }

    #[test]
    fn a_long_hand_quote_form_is_data() {
        let tree = parse("(quote (block b 1))");
        assert!(is_unevaluated_at(&tree, span_of(&tree, "block")));
    }

    /// A comma inside a hard quote is a literal comma, not an escape back to
    /// code — the shape a single depth counter reads wrongly.
    #[test]
    fn a_comma_inside_a_hard_quote_is_still_data() {
        let tree = parse("'(a ,(block b 1))");
        assert!(is_unevaluated_at(&tree, span_of(&tree, "block")));
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_is_code() {
        let tree = parse("`(a ,(block b 1))");
        assert!(!is_unevaluated_at(&tree, span_of(&tree, "block")));
    }

    #[test]
    fn a_quasiquoted_template_is_data() {
        let tree = parse("`(block b 1)");
        assert!(is_unevaluated_at(&tree, span_of(&tree, "block")));
    }

    // -- the chain ----------------------------------------------------------

    #[test]
    fn the_chain_runs_from_the_top_level_form_to_the_target() {
        let tree = parse("(defun f ()\n  (dolist (x l)\n    (return-from f x)))");
        let target = span_of(&tree, "return-from");
        let heads = with_lexical_chain(&tree, target, |chain| {
            chain
                .nodes
                .iter()
                .map(|node| list_head(node).unwrap_or("?").to_owned())
                .collect::<Vec<_>>()
        })
        .expect("a chain");
        assert_eq!(heads, vec!["defun", "dolist", "return-from"]);
    }

    /// Only the enclosing top-level form is materialized, so a target in the
    /// last of many forms still sees a three-node chain and not the file.
    #[test]
    fn the_chain_covers_only_the_enclosing_top_level_form() {
        let source = "(defun a () 1)\n(defun b () 2)\n(defun c () (block z (return-from z 1)))\n";
        let tree = parse(source);
        let target = span_of(&tree, "return-from");
        let heads = with_lexical_chain(&tree, target, |chain| {
            chain
                .nodes
                .iter()
                .map(|node| list_head(node).unwrap_or("?").to_owned())
                .collect::<Vec<_>>()
        })
        .expect("a chain");
        assert_eq!(heads, vec!["defun", "block", "return-from"]);
    }

    // -- block scope --------------------------------------------------------

    fn scopes(source: &str, head: &str) -> Vec<BlockScope> {
        let tree = parse(source);
        let target = span_of(&tree, head);
        with_lexical_chain(&tree, target, |chain| {
            chain
                .ancestors_inward()
                .map(|index| block_scope(&chain.nodes, index))
                .collect()
        })
        .expect("a chain")
    }

    #[test]
    fn a_defun_establishes_a_block_named_after_it() {
        assert_eq!(
            scopes("(defun f () (return-from f 1))", "return-from"),
            vec![BlockScope::Named("f".to_owned())]
        );
    }

    #[test]
    fn a_setf_defun_establishes_a_block_named_after_the_accessor() {
        assert_eq!(
            scopes("(defun (setf f) (v) (return-from f 1))", "return-from"),
            vec![BlockScope::Named("f".to_owned())]
        );
    }

    #[test]
    fn every_iteration_macro_establishes_the_nil_block() {
        for source in [
            "(dolist (x l) (return 1))",
            "(dotimes (i 3) (return 1))",
            "(do () (t) (return 1))",
            "(prog () (return 1))",
            "(loop (return 1))",
            "(do-symbols (s) (return 1))",
        ] {
            assert_eq!(
                scopes(source, "return")[0],
                BlockScope::Named("nil".to_owned()),
                "{source}"
            );
        }
    }

    /// CLHS 6.1.1.4: a `named` clause *replaces* the loop's `nil` block.
    #[test]
    fn a_named_loop_does_not_establish_the_nil_block() {
        assert_eq!(
            scopes("(loop named outer do (return 1))", "return")[0],
            BlockScope::Named("outer".to_owned())
        );
    }

    #[test]
    fn an_flet_binding_body_is_a_block_named_after_the_binding() {
        let scopes = scopes(
            "(flet ((helper () (return-from helper 1))) 1)",
            "return-from",
        );
        assert_eq!(scopes[0], BlockScope::Named("helper".to_owned()));
        assert_eq!(scopes[1], BlockScope::Transparent);
        assert_eq!(
            scopes[2],
            BlockScope::LocalFunctions(vec!["helper".to_owned()])
        );
    }

    #[test]
    fn a_user_macro_is_unknown() {
        assert_eq!(
            scopes("(with-retry (return-from retry 1))", "return-from"),
            vec![BlockScope::Unknown]
        );
    }

    #[test]
    fn a_lambda_establishes_no_block() {
        assert_eq!(
            scopes("(lambda () (return-from f 1))", "return-from"),
            vec![BlockScope::Transparent]
        );
    }

    // -- tags ---------------------------------------------------------------

    #[test]
    fn direct_tags_reads_symbols_and_integers_from_the_body() {
        let tree = parse("(tagbody start (foo) 10 (bar) \"doc\" (baz))");
        let root = tree.root_view();
        let tags: Vec<String> = direct_tags(&root.children[0])
            .into_iter()
            .map(|(tag, _)| tag.display())
            .collect();
        assert_eq!(tags, vec!["start", "10"]);
    }

    #[test]
    fn a_do_body_starts_after_its_end_test() {
        let tree = parse("(do ((i 0)) ((= i 3)) top (foo))");
        let root = tree.root_view();
        let tags: Vec<String> = direct_tags(&root.children[0])
            .into_iter()
            .map(|(tag, _)| tag.display())
            .collect();
        assert_eq!(tags, vec!["top"]);
    }

    #[test]
    fn integer_tags_compare_numerically() {
        let tree = parse("(tagbody 007)");
        let root = tree.root_view();
        assert_eq!(direct_tags(&root.children[0])[0].0, Tag::Integer(7));
    }
}
