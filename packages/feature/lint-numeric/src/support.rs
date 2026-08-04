//! What the numeric rules share: which parts of a file are *code*, and how a
//! numeric token is classified.
//!
//! # Evaluation context
//!
//! The lint engine's dispatch walks into quoted data like any other subtree and
//! [`RuleContext`] carries no parent pointer, so a head-matched node cannot tell
//! on its own whether it is code. `'(+ 3.14 1.0d0)` is a list of four symbols,
//! not an arithmetic form. [`is_unevaluated_at`] answers that by descending from
//! the root along the single chain of nodes whose span contains the candidate's
//! — depth-many steps, not tree-many — and every rule here calls it only once it
//! already has a finding to report.
//!
//! The semantics are deliberately copied from
//! `paredit-feature-lint-condition-system`'s module of the same name rather than
//! depended on across a package boundary, and the five shapes that distinguish a
//! correct implementation from a plausible-looking wrong one are pinned in this
//! module's own tests:
//!
//! 1. `'(…)` is data.
//! 2. `(quote (…))` is data below its head.
//! 3. `` `(…) `` without an unquote is data.
//! 4. `` `(a ,(…)) `` is *code* from the comma down.
//! 5. `'(a ,(…))` is data — a comma inside a **hard** quote is a literal comma
//!    in a literal list, not an escape. This is the shape a single `i32` depth
//!    counter gets wrong, which is why `hard` is a `bool` that never clears and
//!    `quasi` is a separate counter.
//!
//! # Numeric classification
//!
//! [`FloatKind`] and [`integer_literal_value`] read a token the way the Common
//! Lisp reader does, which is *not* the way Rust's `str::parse` does: `1.` is a
//! decimal-radix **integer** in Common Lisp, `1/3` is an exact ratio, and a
//! float's format comes from its exponent marker (`1.0` is a single-float under
//! the default `*read-default-float-format*`, `1.0d0` is a double-float).
//!
//! Nothing here is called per visited node. That is deliberate: the
//! `clean/forms/*` benchmarks lint files with zero findings, so the per-file
//! cost of a rule that matches nothing is exactly what they measure.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head, symbol_is};

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
    /// `#'`, `#.`, `#+`, metadata and the rest are deliberately neutral: none of
    /// them turns code into data.
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
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// The reader-quote state in force at `target`.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the cost is the node's depth, not the file's size.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(truncate (float x))) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it, and
/// that is already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all.
fn quote_state_at(tree: &SyntaxTree, target: ByteSpan) -> QuoteState {
    let root = tree.root_view();
    let mut view: &ExpressionView = &root;
    let mut state = QuoteState::EVALUATED;

    loop {
        let quoting = is_quote_form(view);
        // A span that names no node is judged by the innermost node that
        // contains it, which is the honest answer for a span the caller
        // synthesized rather than took from the tree.
        let Some(child) = view
            .children
            .iter()
            .find(|child| span_contains(child.span, target))
        else {
            return state;
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
        if view.span == target {
            return state;
        }
    }
}

/// Whether the node at `target` is unevaluated data rather than code — under
/// *either* kind of quote.
///
/// The right question for the float-precision rules: `` `(+ 3.14 1.0d0) `` with
/// no unquote is a template whose numbers are never computed with, so reporting
/// it would be reporting a list of symbols.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    quote_state_at(tree, target).is_data()
}

/// Whether the node at `target` is inside a **hard** `'` quote, ignoring
/// quasiquote entirely.
///
/// This is the guard the `eq`/`eql` family wants, and it is deliberately weaker
/// than [`is_unevaluated_at`].
///
/// A `'(…)` list is data in the strongest sense: `'((eq function "f_eq") …)` is
/// a table of three-element rows, and nothing in it is ever a call. Every
/// `eql-string-comparison` finding measured over 5 388 Quicklisp and SBCL files
/// was exactly that shape — a HyperSpec index table and an opcode alist — and
/// every one of them was a false positive.
///
/// A `` `(…) `` template is the opposite case. Most quasiquoted `eq` forms in
/// SBCL's own sources are macro bodies that really do become code — `` `(eq
/// ,val 0) `` in `hashset.lisp`, and the same shape in `srctran.lisp` and
/// `ir1tran-lambda.lisp` — so suppressing those would trade this rule's false
/// positives for false *negatives* on genuine defects. A macro that expands to
/// `(eq x "s")` has the bug the rule is named for, and the expansion is where it
/// will bite.
///
/// Hence `hard` alone rather than [`QuoteState::is_data`]. The two-counter state
/// is what makes the distinction expressible at all: a single depth counter
/// cannot tell `'(a ,(eq x "s"))` — still data, the comma is a literal comma —
/// from `` `(a ,(eq x "s")) ``, which is code.
#[must_use]
pub fn is_hard_quoted_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    quote_state_at(tree, target).hard
}

// -- type-specifier position ---------------------------------------------------

/// The compound type specifiers that *contain* another type specifier, per
/// CLHS 4.2.3.
///
/// Climbing through these is what lets one anchor cover the shapes the corpus
/// actually writes: `(typecase x ((cons (eql :begin-file)) …))` puts the `eql`
/// two levels below the clause head, and `(declare (type (or function (eql 0))
/// f))` puts it two levels below the `type` declaration.
///
/// `and`, `or` and `not` are also perfectly ordinary macros, which is exactly
/// why climbing alone can never be the whole test — `(when (or (eql x) y) …)`
/// climbs to a `when`, which anchors nothing, and stays reported.
///
/// `member` and `satisfies` are deliberately absent: their arguments are objects
/// and a predicate name, not nested type specifiers.
const TYPE_SPECIFIER_COMBINATORS: [&str; 11] = [
    "and",
    "or",
    "not",
    "cons",
    "values",
    "array",
    "simple-array",
    "vector",
    "simple-vector",
    "function",
    "complex",
];

/// Forms whose argument at a fixed index is an **unevaluated** type specifier,
/// paired with the index that argument occupies.
///
/// Being unevaluated is the whole criterion, and it is what keeps `typep`,
/// `subtypep` and `coerce` off this list even though each is spelled with a
/// type. Those three are *functions*, so their type argument is an ordinary
/// evaluated form: `(typep x '(eql 5))` is a quoted specifier — a question this
/// module does not answer — but `(typep x (eql 5))` really is a one-argument
/// call to `eql`, and anchoring on the head would turn a true positive into
/// silence. `the`, `check-type` and the rest below are special operators or
/// macros that never evaluate the specifier at all.
const TYPE_ARGUMENT_ANCHORS: [(&str, usize); 2] = [("the", 1), ("check-type", 2)];

/// The `typecase` family, whose every clause is headed by a type specifier.
const TYPECASE_HEADS: [&str; 3] = ["typecase", "etypecase", "ctypecase"];

/// The declaration forms that carry a `(type SPEC var…)` / `(ftype SPEC name…)`
/// declaration specifier.
const DECLARATION_HEADS: [&str; 3] = ["declare", "declaim", "proclaim"];

/// The generic-function definers whose specialized lambda list may carry an
/// `(eql object)` specializer.
///
/// `:method` is the same geometry with the head replaced, as it appears inside
/// `defgeneric`.
const METHOD_HEADS: [&str; 2] = ["defmethod", ":method"];

/// The chain of `(parent, index-of-the-child-descended-into)` steps from the
/// root down to `target`, or `None` when no node has exactly that span.
///
/// Descends through the single child at each level whose span contains the
/// target, so the cost is the node's depth rather than the file's size — the
/// same descent [`quote_state_at`] makes.
fn ancestors_of(root: &ExpressionView, target: ByteSpan) -> Option<Vec<(&ExpressionView, usize)>> {
    let mut view = root;
    let mut chain: Vec<(&ExpressionView, usize)> = Vec::new();
    loop {
        let (index, child) = view
            .children
            .iter()
            .enumerate()
            .find(|(_, child)| span_contains(child.span, target))?;
        chain.push((view, index));
        if child.span == target {
            return Some(chain);
        }
        view = child;
    }
}

/// The lambda-list index of a `defmethod`-shaped form.
///
/// `(defmethod name qualifier* specialized-lambda-list . body)`: the qualifiers
/// are atoms, so the lambda list is the first *list* child past the name. The
/// search starts past the name rather than at it because `(setf documentation)`
/// is a perfectly ordinary method name and is itself a list.
fn method_lambda_list_index(view: &ExpressionView, name_index: usize) -> Option<usize> {
    view.children
        .iter()
        .enumerate()
        .skip(name_index + 1)
        .find(|(_, child)| is_paren_list(child))
        .map(|(index, _)| index)
}

/// Whether `(parent, index)` is a position that holds a type specifier.
///
/// Split out from [`is_eql_type_specifier_at`] so each anchor is one readable
/// clause; `grand` is the step above, which several anchors need because the
/// position alone does not settle them.
fn is_type_specifier_position(
    parent: &ExpressionView,
    index: usize,
    grand: Option<(&ExpressionView, usize)>,
) -> bool {
    // `(typep x SPEC)`, `(the SPEC form)`, `(check-type place SPEC)`, …
    if let Some(head) = list_head(parent) {
        if TYPE_ARGUMENT_ANCHORS
            .iter()
            .any(|(name, at)| *at == index && symbol_is(head, name))
        {
            return true;
        }
        // `(declare (type SPEC var…))` / `(declaim (ftype SPEC name…))`. The
        // `type` head is not enough on its own — a function may be named
        // `type` — so the enclosing declaration form has to agree.
        if index >= 1 && (symbol_is(head, "type") || symbol_is(head, "ftype")) {
            let inside_declaration = grand.is_some_and(|(outer, _)| {
                list_head(outer).is_some_and(|outer_head| {
                    DECLARATION_HEADS
                        .iter()
                        .any(|name| symbol_is(outer_head, name))
                })
            });
            if inside_declaration {
                return true;
            }
        }
    }

    // A slot option: `(slot-name … :type SPEC …)`, as `defclass`, `defstruct`
    // and `define-primitive-object` all spell it. A keyword can never be an
    // operator, so the token before the candidate settles this locally.
    if index >= 1 {
        let preceded_by_type_keyword = parent
            .children
            .get(index - 1)
            .and_then(atom_text)
            .is_some_and(|text| text.eq_ignore_ascii_case(":type"));
        if preceded_by_type_keyword {
            return true;
        }
    }

    // A `typecase`/`etypecase`/`ctypecase` clause head: the candidate is child 0
    // of a clause, and the clause is child 2-or-later of the `typecase`.
    if index == 0 {
        let in_typecase_clause = grand.is_some_and(|(outer, clause_index)| {
            clause_index >= 2
                && list_head(outer).is_some_and(|outer_head| {
                    TYPECASE_HEADS
                        .iter()
                        .any(|name| symbol_is(outer_head, name))
                })
        });
        if in_typecase_clause {
            return true;
        }
    }

    false
}

/// Whether the one-argument `(eql object)` at `target` is a CLHS 4.2.3 compound
/// **type specifier** rather than a call.
///
/// `(eql object)` is the one shape where a written arity of one is not merely
/// legal but required, and it is legal exactly where a type specifier is: as a
/// `defmethod` `eql` specializer, a `typecase` clause head, a `declare (type …)`
/// specifier, a `defclass`/`defstruct` slot `:type`, and inside the compound
/// specifiers that nest others. `(defmethod g ((x (eql 7))) …)` is not a
/// one-argument call to `eql` and never was.
///
/// The verdict needs the candidate's *ancestors*, which no head-matched node
/// carries, so this descends from the root exactly as [`quote_state_at`] does.
/// The two questions are independent and both are asked only once a finding
/// already exists, so ordinary code never reaches [`SyntaxTree::root_view`].
///
/// Deliberately anchored on **standard Common Lisp** only. SBCL's compiler
/// spells type specifiers in its own macros too — `defknown`, `deftransform`,
/// `(:constant …)` in `define-vop`, `constant-arg` — and those stay reported
/// rather than teach a general Common Lisp linter one implementation's
/// internals, which would silence a user macro that merely shares a name.
///
/// A quote is *not* consulted: `'(cons (eql or) …)` is a separate question with
/// a separate answer, and conflating the two would make each harder to read.
#[must_use]
pub fn is_eql_type_specifier_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let root = tree.root_view();
    let Some(chain) = ancestors_of(&root, target) else {
        return false;
    };

    // Climb past the compound specifiers that nest another specifier. Index 0
    // is the combinator's own head, so only a later child is a nested spec.
    let mut depth = chain.len();
    // A combinator's own head is child 0 and is an atom, so a nested specifier
    // is always a later child; `list_head` already declines a parent whose
    // child 0 is the candidate itself.
    while depth >= 1 {
        let (parent, _) = chain[depth - 1];
        let climbs = list_head(parent).is_some_and(|head| {
            TYPE_SPECIFIER_COMBINATORS
                .iter()
                .any(|name| symbol_is(head, name))
        });
        if !climbs {
            break;
        }
        depth -= 1;
    }
    if depth == 0 {
        return false;
    }

    let (parent, index) = chain[depth - 1];
    let grand = (depth >= 2).then(|| chain[depth - 2]);
    if is_type_specifier_position(parent, index, grand) {
        return true;
    }
    is_method_eql_specializer(&chain, depth, parent, index, grand)
}

/// Whether the candidate is a `defmethod` `eql` specializer.
///
/// The shape is `(defmethod name qualifier* (… (parameter (eql object)) …) …)`:
/// the candidate is the second element of a two-element parameter, that
/// parameter sits in the method's own lambda list, and — the part that a shape
/// check alone gets wrong — it sits in the lambda list's **required** section.
fn is_method_eql_specializer(
    chain: &[(&ExpressionView, usize)],
    depth: usize,
    parent: &ExpressionView,
    index: usize,
    grand: Option<(&ExpressionView, usize)>,
) -> bool {
    if index != 1 || parent.children.len() != 2 || !is_paren_list(parent) {
        return false;
    }
    let Some((lambda_list, parameter_index)) = grand else {
        return false;
    };
    let Some((definer, lambda_list_index)) = (depth >= 3).then(|| chain[depth - 3]) else {
        return false;
    };
    let Some(head) = list_head(definer) else {
        return false;
    };
    if !METHOD_HEADS.iter().any(|name| symbol_is(head, name)) {
        return false;
    }
    // `(:method …)` has no name of its own, so its lambda list may start one
    // child earlier than `defmethod`'s.
    let name_index = if symbol_is(head, ":method") { 0 } else { 1 };
    if !is_paren_list(lambda_list)
        || method_lambda_list_index(definer, name_index) != Some(lambda_list_index)
    {
        return false;
    }
    // Only a *required* parameter may be specialized. Past a lambda-list
    // keyword the very same `(name form)` shape is an `&optional`/`&key`
    // parameter with a **default value form**, and that form is live code:
    // `(defmethod g (a &key (k (eql y))) …)` really does call `eql` with one
    // argument, which is the defect this rule is named for.
    lambda_list
        .children
        .iter()
        .take(parameter_index)
        .all(|child| atom_text(child).is_none_or(|text| !text.starts_with('&')))
}

// -- key and binding positions -------------------------------------------------

/// The `case` family, whose clause heads are **key designators** rather than
/// operators.
///
/// `typecase` is deliberately absent: its clause heads are *type specifiers*,
/// which is a different question with a different answer.
const CASE_HEADS: [&str; 3] = ["case", "ecase", "ccase"];

/// Forms whose child at a fixed index is a variable list, a lambda list or a
/// binding spec — a position that *names* variables and calls nothing.
///
/// Every entry is standard Common Lisp and every index is fixed by the CLHS
/// syntax of the form, which is what makes the position decidable from the
/// shape alone. `defmethod` is deliberately absent: its lambda-list index moves
/// with its qualifiers, so it is not a fixed-index anchor at all.
const VARIABLE_LIST_ANCHORS: [(&str, usize); 21] = [
    // `(multiple-value-bind (a b) values-form …)` — a list of variable names.
    ("multiple-value-bind", 1),
    ("multiple-value-setq", 1),
    ("destructuring-bind", 1),
    ("with-slots", 1),
    // `(dolist (x list) …)` — a `(variable form…)` binding spec.
    ("dolist", 1),
    ("dotimes", 1),
    ("do-symbols", 1),
    ("do-external-symbols", 1),
    ("do-all-symbols", 1),
    ("with-open-file", 1),
    ("with-open-stream", 1),
    ("with-input-from-string", 1),
    ("with-output-to-string", 1),
    // A lambda list at the index the form's syntax fixes it at.
    ("lambda", 1),
    ("defun", 2),
    ("defmacro", 2),
    ("defgeneric", 2),
    ("deftype", 2),
    ("define-compiler-macro", 2),
    ("define-modify-macro", 2),
    ("define-setf-expander", 2),
];

/// Forms whose child 1 is a **binding list**, each of whose own children binds
/// one name.
///
/// The distinction from [`VARIABLE_LIST_ANCHORS`] is a level of nesting: here
/// the candidate is one *binding*, so its parent is the binding list and its
/// grandparent is the form.
const BINDING_LIST_HEADS: [&str; 10] = [
    "let",
    "let*",
    "prog",
    "prog*",
    "do",
    "do*",
    "symbol-macrolet",
    "flet",
    "labels",
    "macrolet",
];

const fn is_case_head_index(index: usize) -> bool {
    // Child 0 is the `case` operator and child 1 is the keyform — which *is*
    // evaluated, so `(case (eql x) …)` stays reported. Only child 2 and later
    // are clauses.
    index >= 2
}
/// Whether the candidate occupies a `case`-family clause's **keys** position.
///
/// CLHS 5.3 gives `case` the syntax `(case keyform {(keys form*)}*)`, where
/// `keys` is a designator for a list of keys and an atom stands for the
/// singleton list containing it. So in `(case kind (eql x) …)` the `eql` is a
/// *symbol being compared against* and `x` is a body form — there is no call
/// there at all.
///
/// Two spellings, because the designator may be an atom or a list, and they sit
/// at different depths.
fn is_case_clause_key(
    parent: &ExpressionView,
    index: usize,
    grand: Option<(&ExpressionView, usize)>,
) -> bool {
    // `(case keyform (KEY body*) …)`: the candidate *is* the clause, so the
    // head this rule matched on is the clause's atom key designator.
    if is_case_head_index(index)
        && list_head(parent).is_some_and(|head| CASE_HEADS.iter().any(|name| symbol_is(head, name)))
    {
        return true;
    }
    // `(case keyform ((KEY …) body*) …)`: the candidate is the clause's key
    // *list*, which is child 0 of the clause.
    //
    // Requiring index 0 is the whole boundary this guard creates: a clause's
    // body forms are child 1 and later, and a genuine `(eql x)` there is
    // ordinary code that stays reported.
    index == 0
        && grand.is_some_and(|(outer, clause_index)| {
            is_case_head_index(clause_index)
                && list_head(outer)
                    .is_some_and(|head| CASE_HEADS.iter().any(|name| symbol_is(head, name)))
        })
}

/// Whether the candidate occupies a variable-binding position rather than a
/// call position.
fn is_binding_position(
    parent: &ExpressionView,
    index: usize,
    grand: Option<(&ExpressionView, usize)>,
) -> bool {
    // `(multiple-value-bind (equal certain) …)` binds variables named `equal`
    // and `certain`; the list is not a call and its length is not an arity.
    if let Some(head) = list_head(parent) {
        if VARIABLE_LIST_ANCHORS
            .iter()
            .any(|(name, at)| *at == index && symbol_is(head, name))
        {
            return true;
        }
    }
    // `(let ((eq …) …) …)`: the candidate is one binding, so its parent is the
    // binding list — child 1 of the form — and the form is one level further
    // up. An *initial value* form sits one level deeper still, so it keeps its
    // own verdict: in `(let ((a (eql x))) …)` the `(eql x)` is live code.
    grand.is_some_and(|(outer, binding_list_index)| {
        binding_list_index == 1
            && list_head(outer)
                .is_some_and(|head| BINDING_LIST_HEADS.iter().any(|name| symbol_is(head, name)))
    })
}

/// Whether the node at `target` sits in a position that names something rather
/// than calling it: a `case`-family clause **key**, or a variable-binding list.
///
/// Both are positions where the written shape `(eql x)` has no arity at all —
/// `eql` there is a symbol being matched or a variable being bound — so an
/// arity rule has nothing to say about them. Unlike the type-specifier
/// question, this one is not restricted to one operator or one argument count:
/// `(multiple-value-bind (equal less-or-equal greater-or-equal when-true
/// when-false) …)` is a five-variable binding list that this rule otherwise
/// reports as a four-argument `equal` call.
///
/// The verdict needs the candidate's *ancestors*, which no head-matched node
/// carries, so this descends from the root exactly as [`quote_state_at`] does,
/// and — like every other guard here — is asked only once a finding already
/// exists, so ordinary code never reaches [`SyntaxTree::root_view`].
///
/// Deliberately anchored on **standard Common Lisp** only, for the same reason
/// the quote guard is: teaching a general Common Lisp linter one
/// implementation's binding macros would silence a user macro that merely
/// shares a name.
#[must_use]
pub fn is_key_or_binding_position_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let root = tree.root_view();
    let Some(chain) = ancestors_of(&root, target) else {
        return false;
    };
    let Some(&(parent, index)) = chain.last() else {
        return false;
    };
    let grand = (chain.len() >= 2).then(|| chain[chain.len() - 2]);
    is_case_clause_key(parent, index, grand) || is_binding_position(parent, index, grand)
}

/// Which floating-point format a numeric token reads as.
///
/// The distinction is the exponent marker, per CLHS 2.3.1: `e`/`E` (and no
/// marker at all) means the value of `*read-default-float-format*`, which is
/// `single-float` unless a file rebinds it; `s`/`f` are short and single; `d`/`l`
/// are double and long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatKind {
    /// `1.0`, `1.0e3`, `1.0s0`, `1.0f0` — single-float under the default
    /// `*read-default-float-format*`.
    Single,
    /// `1.0d0`, `1.0L0` — explicitly wider than single.
    Double,
}

/// The float format `text` reads as, or `None` when it is not a float token at
/// all.
///
/// Excluded on purpose, because each is *exact* and so cannot lose precision:
/// an integer (`42`), a decimal-radix integer (`1.` — a trailing point is
/// radix notation in Common Lisp, not a float), and a ratio (`1/3`).
///
/// The first byte is checked before anything else, so a symbol — which is what
/// almost every operand of almost every arithmetic form is — is rejected after
/// one comparison.
#[must_use]
pub fn float_kind(text: &str) -> Option<FloatKind> {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    // The cheap gate: an operand that cannot start a number is not one.
    if !body.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return None;
    }
    // A ratio is exact, whatever else it looks like.
    if body.contains('/') {
        return None;
    }

    let mut digits = false;
    let mut point = false;
    let mut marker: Option<char> = None;
    for (index, character) in body.char_indices() {
        match character {
            '0'..='9' => digits = true,
            '.' if !point && marker.is_none() => point = true,
            'e' | 's' | 'f' | 'd' | 'l' | 'E' | 'S' | 'F' | 'D' | 'L'
                if digits && marker.is_none() && index > 0 =>
            {
                marker = Some(character.to_ascii_lowercase());
            }
            '+' | '-' if marker.is_some() => {}
            _ => return None,
        }
    }
    if !digits {
        return None;
    }
    // `1.` is an integer in Common Lisp, so a trailing point is not a float.
    if !(point && !body.ends_with('.')) && marker.is_none() {
        return None;
    }
    match marker {
        Some('d' | 'l') => Some(FloatKind::Double),
        _ => Some(FloatKind::Single),
    }
}

/// The digits of a float token with its exponent marker normalized to `e`, so
/// that Rust's `str::parse` reads the same value the Common Lisp reader does.
///
/// `1.5d0` and `1.5` both become a string Rust parses as `1.5`; the format is
/// [`float_kind`]'s business, not this function's.
fn normalized_for_parse(text: &str) -> String {
    text.chars()
        .map(|c| match c.to_ascii_lowercase() {
            's' | 'f' | 'd' | 'l' => 'e',
            other => other,
        })
        .collect()
}

/// Whether widening this single-float token to a double-float would change its
/// value away from what the same digits mean as a double.
///
/// This is the discriminator that keeps `mixed-float-precision-arithmetic` off
/// correct code. `1.5`, `2.0`, `0.25` and `0.5` are exactly representable in
/// both formats, so `(* 1.5 x 1.0d0)` computes with the value that was written
/// and there is nothing to report. `3.14` is not: as a single-float it is
/// 3.1400001049041748, and *that* is the value a double-float result carries —
/// verified against SBCL, which prints `(* 3.14 1.0d0)` as
/// `3.140000104904175d0`.
///
/// A token Rust cannot parse answers `false`. Refusing to report what cannot be
/// read is the safe direction: this function only ever *adds* a finding.
#[must_use]
pub fn single_float_widens_lossily(text: &str) -> bool {
    let normalized = normalized_for_parse(text);
    let (Ok(narrow), Ok(wide)) = (normalized.parse::<f32>(), normalized.parse::<f64>()) else {
        return false;
    };
    // Not a tolerance comparison: the question is exactly whether the two
    // formats denote the same real number, and `f64::from(narrow)` is exact.
    f64::from(narrow) != wide
}

/// Whether a float token denotes a number binary floating point can hold
/// *exactly* — a dyadic rational, `m / 2^k`.
///
/// A decimal literal is `M × 10^(e-n)` for mantissa digits `M`, `n` fractional
/// digits and exponent `e`. Writing `k = n - e`, the value is `M / 10^k`, which
/// reduces to a power-of-two denominator exactly when `5^k` divides `M`. So
/// `0.5` (M=5, k=1) and `0.125` (M=125, k=3) are exact, and `0.1` (M=1, k=1) and
/// `0.3` (M=3, k=1) are not.
///
/// This is the guard that keeps `epsilon-less-float-loop-bound` off correct
/// code. A loop stepping by an exact 0.5 accumulates 0.5, 1.0, 1.5, … with no
/// drift at all, so an equality test against it is not the classic footgun —
/// verified against SBCL, where a `do` loop stepping by 0.5 to a limit of 2
/// terminates and one stepping by 0.1 does not.
///
/// Undecidable input answers `true` (exact, so no finding): a mantissa too long
/// for `i64` and an exponent large enough to overflow `5^k` both take the
/// false-negative direction on purpose.
#[must_use]
pub fn is_exact_binary_float(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);

    // Split the exponent off, if the token carries one.
    let (mantissa, exponent) = match body.char_indices().find(|(index, c)| {
        *index > 0 && matches!(c.to_ascii_lowercase(), 'e' | 's' | 'f' | 'd' | 'l')
    }) {
        Some((at, _)) => {
            let (head, tail) = body.split_at(at);
            match tail[1..].parse::<i32>() {
                Ok(value) => (head, value),
                // An exponent that will not parse is not one this can reason
                // about.
                Err(_) => return true,
            }
        }
        None => (body, 0),
    };

    let fraction_digits = match mantissa.find('.') {
        Some(at) => i32::try_from(mantissa.len() - at - 1).unwrap_or(0),
        None => 0,
    };
    let Ok(digits) = mantissa.replace('.', "").parse::<i64>() else {
        return true;
    };

    let k = fraction_digits - exponent;
    if k <= 0 {
        // An integer times a power of ten: exactly representable.
        return true;
    }
    // 5^28 overflows an i64, and a literal needing that many fractional digits
    // is past anything this can decide.
    let Some(power) = 5i64.checked_pow(u32::try_from(k).unwrap_or(u32::MAX)) else {
        return true;
    };
    digits % power == 0
}

/// The value of a plain decimal integer token, or `None` for anything else.
///
/// Deliberately narrow. A ratio, a float, a radix prefix (`#x10`), a
/// decimal-radix trailing point (`1.`) and a bignum too large for `i64` all
/// answer `None`, because the rules that consult this want to *compute* with the
/// value and a guess would be worse than a skipped form.
#[must_use]
pub fn integer_literal_value(text: &str) -> Option<i64> {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i64>().ok()
}

/// Runs one rule end to end through the real lint engine and returns, per
/// finding, its message and the source that applying its fix produces.
///
/// A domain test cannot see the thing that actually broke these rules on real
/// code, because the domain never applies a fix: a replacement region that
/// starts one byte early deletes the form's reader prefix, and every assertion
/// about spans and counts still passes. Only the spliced source shows it. So
/// this returns the rewritten text, not just the finding.
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
    use paredit_core_syntax::view_query::for_each_subview;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    /// Finds the first node with `head` — deliberately through the *unfiltered*
    /// walk, since the point is to locate the node even when it is data — and
    /// asks the span-directed lookup about it.
    fn unevaluated_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    // -- the five quote shapes ----------------------------------------------

    #[test]
    fn shape_1_a_hard_quoted_list_is_data() {
        assert!(unevaluated_at_first_head("'(+ 3.14 1.0d0)", "+"));
    }

    #[test]
    fn shape_2_a_long_hand_quote_form_is_data_below_its_head() {
        assert!(unevaluated_at_first_head("(quote (+ 3.14 1.0d0))", "+"));
    }

    #[test]
    fn shape_3_a_backquote_without_an_unquote_is_data() {
        assert!(unevaluated_at_first_head("`(+ 3.14 1.0d0)", "+"));
    }

    #[test]
    fn shape_4_an_unquote_inside_a_backquote_is_code_again() {
        assert!(!unevaluated_at_first_head("`(a ,(+ 3.14 1.0d0))", "+"));
    }

    /// The shape a single depth counter gets wrong: the comma is a literal
    /// comma inside a literal list, so everything under it stays data.
    #[test]
    fn shape_5_a_comma_inside_a_hard_quote_stays_data() {
        assert!(unevaluated_at_first_head("'(a ,(+ 3.14 1.0d0))", "+"));
    }

    #[test]
    fn plain_code_is_evaluated() {
        assert!(!unevaluated_at_first_head(
            "(defun f (x) (+ 3.14 x 1.0d0))",
            "+"
        ));
    }

    /// A node one level *inside* a quote is still data, which is the other half
    /// of what a "check the immediate parent only" implementation misses.
    #[test]
    fn a_node_nested_deep_inside_a_hard_quote_is_still_data() {
        assert!(unevaluated_at_first_head(
            "'(a (b (c (+ 3.14 1.0d0))))",
            "+"
        ));
    }

    // -- the hard-quote-only guard the eq/eql family uses --------------------

    fn hard_quoted_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_hard_quoted_at(&parsed, span.expect("the head must occur in the source"))
    }

    /// The three shapes where the two guards agree that the node is data.
    #[test]
    fn a_hard_quote_is_hard_quoted_in_every_spelling() {
        assert!(hard_quoted_at_first_head("'(eq x \"s\")", "eq"));
        assert!(hard_quoted_at_first_head("(quote (eq x \"s\"))", "eq"));
        assert!(hard_quoted_at_first_head("'(a (b (eq x \"s\")))", "eq"));
    }

    /// The shape a single depth counter gets wrong: a comma inside a hard quote
    /// is a literal comma, so the node stays hard-quoted.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_hard_quoted() {
        assert!(hard_quoted_at_first_head("'(a ,(eq x \"s\"))", "eq"));
    }

    #[test]
    fn plain_code_is_not_hard_quoted() {
        assert!(!hard_quoted_at_first_head(
            "(defun f (x) (eq x \"s\"))",
            "eq"
        ));
    }

    /// **The divergence.** A quasiquoted template is `is_unevaluated_at` data
    /// but is *not* hard-quoted, because `` `(eq ,val 0) `` becomes code. This
    /// is the assertion that fails if someone "simplifies" the eq family back
    /// onto `is_unevaluated_at`, and the false negatives that would follow are
    /// genuine defects in SBCL-shaped macro bodies.
    #[test]
    fn a_quasiquote_is_unevaluated_but_is_not_hard_quoted() {
        for source in ["`(eq x \"s\")", "`(a ,(eq x \"s\"))", "``(eq x \"s\")"] {
            assert!(!hard_quoted_at_first_head(source, "eq"), "{source}");
        }
        // The other guard disagrees on the template without an unquote, which
        // is exactly why the eq family may not reuse it.
        assert!(unevaluated_at_first_head("`(eq x \"s\")", "eq"));
    }

    /// A hard quote *inside* a quasiquote is still hard.
    #[test]
    fn a_hard_quote_nested_in_a_quasiquote_is_hard_quoted() {
        assert!(hard_quoted_at_first_head("`(a ,'(eq x \"s\"))", "eq"));
    }

    // -- key and binding positions -------------------------------------------

    /// The span of the `occurrence`-th node headed `head`, through the
    /// *unfiltered* walk, so a node nested inside another match is reachable.
    fn nth_head_span(parsed: &SyntaxTree, head: &str, occurrence: usize) -> ByteSpan {
        let mut spans = Vec::new();
        for_each_subview(&parsed.root_view(), |view| {
            if list_head(view).is_some_and(|found| found.eq_ignore_ascii_case(head)) {
                spans.push(view.span);
            }
        });
        spans[occurrence]
    }

    fn key_or_binding(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        is_key_or_binding_position_at(&parsed, nth_head_span(&parsed, head, 0))
    }

    fn key_or_binding_nth(source: &str, head: &str, occurrence: usize) -> bool {
        let parsed = tree(source);
        is_key_or_binding_position_at(&parsed, nth_head_span(&parsed, head, occurrence))
    }

    /// A `case` clause key is a set of *object* keys, not a type, so the
    /// `typecase` anchor must not extend to it.
    ///
    /// The rule declines this shape through [`is_key_or_binding_position_at`]
    /// instead, so this asserts the type predicate specifically — otherwise the
    /// key guard would mask a regression in the type guard.
    #[test]
    fn a_case_clause_key_is_not_a_type_specifier_position() {
        let parsed = tree("(case x ((eql 5) 1))");
        assert!(!is_eql_type_specifier_at(
            &parsed,
            nth_head_span(&parsed, "eql", 0)
        ));
    }

    /// CLHS 5.3: each `case` clause is `(keys form*)`, and an atom `keys`
    /// stands for the singleton list containing it. So the `eql` here is a
    /// symbol being compared against, not an operator.
    #[test]
    fn a_case_clause_with_an_atom_key_is_a_key_position() {
        assert!(key_or_binding("(case kind (eql x))", "eql"));
        assert!(key_or_binding("(ecase kind (eq x))", "eq"));
        assert!(key_or_binding("(ccase kind (equalp x))", "equalp"));
        // The operator's spelling is not part of the question.
        assert!(key_or_binding("(CASE kind (EQL x))", "eql"));
    }

    /// The list spelling of the same designator, which sits one level deeper.
    #[test]
    fn a_case_clause_key_list_is_a_key_position() {
        assert!(key_or_binding("(case kind ((eql equal) x))", "eql"));
        assert!(key_or_binding("(ecase std-fn ((equal) x))", "equal"));
    }

    /// **The boundary this guard creates.** A clause's *body* is child 1 and
    /// later, and a genuine misarity call there is exactly what the rule is
    /// for. Only the key — child 0, or the clause itself — is silenced.
    #[test]
    fn a_case_clause_body_is_not_a_key_position() {
        // A body form under an ordinary atom key.
        assert!(!key_or_binding("(case kind (some-key (eq x)))", "eq"));
        // A body form under a key *list*.
        assert!(!key_or_binding("(case kind ((a b) (eq x)))", "eq"));
        // …and under the `otherwise` and `t` clauses.
        assert!(!key_or_binding("(case kind (otherwise (eq x)))", "eq"));
        assert!(!key_or_binding("(case kind (t (eq x)))", "eq"));
    }

    /// The sharpest form of the boundary: the same operator as key and as body
    /// in one clause. The outer node is the clause, the inner one is code.
    #[test]
    fn a_key_named_eql_does_not_silence_an_eql_call_in_its_own_body() {
        let source = "(case kind (eql (eql x)))";
        assert!(key_or_binding_nth(source, "eql", 0), "the clause is a key");
        assert!(
            !key_or_binding_nth(source, "eql", 1),
            "the body call is code"
        );
    }

    /// The `keyform` is child 1 and *is* evaluated, so it is a real call.
    #[test]
    fn the_case_keyform_is_not_a_key_position() {
        assert!(!key_or_binding("(case (eql x) (a b))", "eql"));
    }

    /// `typecase` clause heads are *type specifiers*, a separate question with
    /// a separate answer. This guard must not claim them.
    #[test]
    fn a_typecase_clause_head_is_not_claimed_by_this_guard() {
        assert!(!key_or_binding("(typecase x ((eql 5) y))", "eql"));
        assert!(!key_or_binding("(etypecase x ((eql 5) y))", "eql"));
    }

    /// A variable list is not a call and its length is not an arity.
    #[test]
    fn a_variable_list_is_a_binding_position() {
        assert!(key_or_binding(
            "(multiple-value-bind (equal certain) (type= a b) certain)",
            "equal"
        ));
        // Five variables, which the rule otherwise reports as four arguments.
        assert!(key_or_binding(
            "(multiple-value-bind (equal less greater when-true when-false) (f) equal)",
            "equal"
        ));
        assert!(key_or_binding("(destructuring-bind (eq a) form eq)", "eq"));
        assert!(key_or_binding("(dolist (equal items) equal)", "equal"));
        assert!(key_or_binding("(defun f (equal x) x)", "equal"));
        assert!(key_or_binding("(lambda (equal x) x)", "equal"));
    }

    /// The shape SBCL's `constraint.lisp` writes: a `let` binding a variable
    /// literally named `eq`.
    #[test]
    fn a_let_binding_is_a_binding_position() {
        assert!(key_or_binding(
            "(let (mark (eq (lambda-var-eq-constraints leaf))) eq)",
            "eq"
        ));
        assert!(key_or_binding("(let* ((equal (f))) equal)", "equal"));
        assert!(key_or_binding("(do ((eql 0)) (done) x)", "eql"));
    }

    /// A binding's *initial value* form is one level deeper than the binding,
    /// so it keeps its own verdict — it is live code.
    #[test]
    fn a_binding_initial_value_form_is_not_a_binding_position() {
        assert!(!key_or_binding("(let ((a (eql x))) a)", "eql"));
        assert!(!key_or_binding("(let* ((a (eq x))) a)", "eq"));
        // …and so is a `do` step form and a `do` end test.
        assert!(!key_or_binding("(do ((i 0 (eql i))) (done) x)", "eql"));
        assert!(!key_or_binding("(do ((i 0)) ((eql i)) x)", "eql"));
    }

    /// A form's *body* is not its binding list.
    #[test]
    fn a_body_form_is_not_a_binding_position() {
        assert!(!key_or_binding("(let ((a 1)) (eq a))", "eq"));
        assert!(!key_or_binding(
            "(multiple-value-bind (a b) (f) (eq a))",
            "eq"
        ));
        assert!(!key_or_binding("(flet ((g (x) (eql x))) (g 1))", "eql"));
        assert!(!key_or_binding("(defun f (x) (eq x))", "eq"));
    }

    /// An ordinary call is neither, however deeply nested.
    #[test]
    fn an_ordinary_call_is_neither() {
        assert!(!key_or_binding("(eq x)", "eq"));
        assert!(!key_or_binding("(when (eql a) t)", "eql"));
        assert!(!key_or_binding("(list (equal a))", "equal"));
    }

    // -- float classification -----------------------------------------------

    #[test]
    fn reads_the_single_float_spellings() {
        for text in ["1.0", "0.1", "-3.25", "1.0e10", "3.0s-2", "1.5f0", "+2.5"] {
            assert_eq!(float_kind(text), Some(FloatKind::Single), "{text}");
        }
    }

    #[test]
    fn reads_the_double_and_long_float_spellings() {
        for text in ["1.0d0", "1d0", "-2.5d-3", "1.0L0", "3.14D0"] {
            assert_eq!(float_kind(text), Some(FloatKind::Double), "{text}");
        }
    }

    /// Every exact spelling. None of these can lose precision, so none of them
    /// is a float as far as these rules are concerned.
    #[test]
    fn does_not_read_an_exact_number_as_a_float() {
        for text in ["1", "-42", "1/3", "0", "1."] {
            assert_eq!(float_kind(text), None, "{text}");
        }
    }

    #[test]
    fn does_not_read_a_symbol_or_a_string_as_a_float() {
        for text in ["x", "point.five", "", "+", "-", "\"1.0\"", "1.0.0", "d0"] {
            assert_eq!(float_kind(text), None, "{text}");
        }
    }

    // -- the widening discriminator, checked against SBCL's answers ----------

    /// SBCL: `(coerce 3.14 'double-float)` => 3.140000104904175d0, which is not
    /// 3.14d0. Same for 0.1 and 1.1.
    #[test]
    fn a_single_float_that_cannot_be_written_exactly_widens_lossily() {
        for text in ["3.14", "0.1", "1.1", "0.2", "1.0e-1"] {
            assert!(single_float_widens_lossily(text), "{text}");
        }
    }

    /// SBCL: `(* 1.5 1.0d0)` => 1.5d0 exactly. A rule that reported these would
    /// be reporting correct code.
    #[test]
    fn a_single_float_that_is_exact_in_both_formats_widens_losslessly() {
        for text in ["1.5", "2.0", "0.25", "0.5", "100.0", "0.0", "-4.75"] {
            assert!(!single_float_widens_lossily(text), "{text}");
        }
    }

    #[test]
    fn an_unparseable_token_is_never_called_lossy() {
        assert!(!single_float_widens_lossily("x"));
        assert!(!single_float_widens_lossily(""));
    }

    // -- exact binary representability --------------------------------------

    /// SBCL: a `do` loop stepping by each of these to a limit of 2 terminates.
    #[test]
    fn a_dyadic_decimal_is_exact() {
        for text in [
            "0.5", "0.25", "0.75", "0.125", "1.0", "2.0", "-0.5", "0.0", "1.5d0", "1.0e0",
        ] {
            assert!(is_exact_binary_float(text), "{text}");
        }
    }

    /// SBCL: a `do` loop stepping by each of these to a limit of 2 never
    /// terminates.
    #[test]
    fn a_decimal_needing_a_factor_of_five_is_not_exact() {
        for text in ["0.1", "0.2", "0.3", "3.14", "0.01", "1.1", "0.1d0"] {
            assert!(!is_exact_binary_float(text), "{text}");
        }
    }

    /// A positive exponent cancels fractional digits: 1.5e1 is 15.
    #[test]
    fn an_exponent_shifts_the_fractional_digit_count() {
        assert!(is_exact_binary_float("1.5e1"));
        assert!(is_exact_binary_float("0.1e1"));
        assert!(!is_exact_binary_float("0.1e-1"));
    }

    /// Undecidable input takes the false-negative direction, so a rule built on
    /// this never reports what it could not read.
    #[test]
    fn an_undecidable_token_is_called_exact() {
        assert!(is_exact_binary_float("0.99999999999999999999999999999999"));
        assert!(is_exact_binary_float("1.0e999999999999"));
        assert!(is_exact_binary_float("x"));
    }

    // -- integer literals ----------------------------------------------------

    #[test]
    fn reads_a_plain_decimal_integer() {
        assert_eq!(integer_literal_value("42"), Some(42));
        assert_eq!(integer_literal_value("-7"), Some(-7));
        assert_eq!(integer_literal_value("+3"), Some(3));
        assert_eq!(integer_literal_value("0"), Some(0));
    }

    #[test]
    fn refuses_everything_that_is_not_a_plain_decimal_integer() {
        for text in [
            "1.0",
            "1/3",
            "#x10",
            "1.",
            "x",
            "",
            "-",
            "99999999999999999999",
        ] {
            assert_eq!(integer_literal_value(text), None, "{text}");
        }
    }
}
