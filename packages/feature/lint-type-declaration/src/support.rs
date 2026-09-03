//! What the declaration rules share: where a form's body starts, which of its
//! leading children are still the declaration section, and how much a literal's
//! type and a declared type can be said to disagree.
//!
//! Three things every rule here needs and neither the engine nor `core/syntax`
//! provides:
//!
//! - **Evaluation context.** A `(declare (fixnum x))` inside `'(…)` is a list of
//!   symbols. The lint engine's dispatch walks into quoted data like any other
//!   subtree and [`RuleContext`] carries no parent pointer, so a head-matched
//!   node cannot tell on its own whether it is code. [`is_unevaluated_at`]
//!   answers that by descending from the root along the single chain of nodes
//!   whose span contains the candidate's — depth-many steps, not tree-many — and
//!   is called only once a rule already has a finding to report.
//! - **Body geometry.** `(defun name (args) . body)` and `(let (bindings) .
//!   body)` put their body at different indices, and CLHS 3.4.11 lets the
//!   declaration section interleave declarations with one documentation string.
//!   [`body_start`] and [`declaration_section_end`] are that geometry.
//! - **A type lattice small enough to be right.** [`TypeExcludes`] answers only
//!   "can this type specifier definitely *not* contain this literal", and only
//!   for atomic specifiers whose relationship to the modelled literals is
//!   enumerable. Every compound specifier — `(or null string)` above all — is
//!   declined, because the whole value of these rules is that they do not fire
//!   on the widening idiom.
//!
//! Nothing here is called per visited node. That is deliberate: the
//! `clean/forms/*` benchmarks lint files with zero findings, so the per-file
//! cost of a rule that matches nothing is exactly what they measure.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext

use paredit_core_lint_engine::model::NormalizedHead;
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_is, unqualified,
};

// -- evaluation context ------------------------------------------------------

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down. A single depth counter cannot express that difference and gets
/// `'(a ,(f))` wrong in the direction that produces false positives.
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

/// The one child of `view` whose span contains `target`, by **binary search**.
///
/// Children are in source order and their spans do not overlap, so the first
/// child whose span ends after `target` starts is the only one that can contain
/// it. That has to be a search rather than a scan: the first level of the
/// descent is the file's *root*, whose children are its top-level forms, and a
/// linear `find` there costs one pass over every top-level form **per finding**.
///
/// That is not hypothetical. With a linear scan here, the `ignore` rule measured
/// 646ms at 500 definitions and 2442ms at 1000 — 3.8× the work for 2× the input
/// on a fixture where every definition reports. The rule's own analysis was
/// already linear; all of the growth was this lookup, reached once per finding
/// from `check`. Only the cost tests could see it, because every finding was
/// correct.
fn containing_child(view: &ExpressionView, target: ByteSpan) -> Option<&ExpressionView> {
    let index = view
        .children
        .partition_point(|child| child.span.end().get() <= target.start().get());
    view.children
        .get(index)
        .filter(|child| span_contains(child.span, target))
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the cost is the node's depth, not the file's size.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(let ((x 0)) (declare (string x)))) `` has a
/// quasiquoted ancestor and an evaluated target. Being inside a hard `'` does
/// settle it, and that is already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let root = tree.root_view();
    let mut view: &ExpressionView = &root;
    let mut state = QuoteState::EVALUATED;

    loop {
        let quoting = is_quote_form(view);
        let Some(child) = containing_child(view, target) else {
            return state.is_data();
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
        if view.span == target {
            return state.is_data();
        }
    }
}

/// Calls `visit` on every node of `root` reachable as evaluated code, with a say
/// in where the walk stops.
///
/// `descend` is asked about each visited node *before* its children are queued;
/// answering `false` visits that node and nothing under it. That is how the
/// `ignore` rule steps over a nested `(declare …)`: naming a variable in a
/// declaration is not *using* it, and counting it as one would make
/// `(declare (ignore x))` followed by `(declare (ignorable x))` report itself.
///
/// Quoted subtrees are still descended — `` `(a ,(f x)) `` has code inside data —
/// but their data nodes are never visited. A pruned node's subtree is skipped
/// including any quoted data in it, which is correct for finding *uses* and
/// would not be for finding data.
/// Where a visited node sits relative to its parent.
#[derive(Debug, Clone, Copy)]
pub struct Position<'a> {
    /// Whether the node is in **operator position** — index 0 of a `(…)` list.
    ///
    /// Common Lisp is a Lisp-2, so a symbol there names a function and a symbol
    /// anywhere else names a variable, and no rule about variables may conflate
    /// them. The root is never in operator position.
    pub is_operator: bool,
    /// The head symbol of the enclosing `(…)` list, when there is one.
    ///
    /// Lets a caller decide whether the *operator* of the call a node sits in is
    /// one whose evaluation semantics it knows, without walking back up.
    pub enclosing_head: Option<&'a str>,
}

impl Position<'_> {
    const ROOT: Self = Self {
        is_operator: false,
        enclosing_head: None,
    };
}

/// Calls `visit` on every node of `root` reachable as evaluated code, with a say
/// in where the walk stops and with each node's [`Position`].
pub fn for_each_evaluated_subview_where<'a>(
    root: &'a ExpressionView,
    mut descend: impl FnMut(&ExpressionView) -> bool,
    mut visit: impl FnMut(&'a ExpressionView, Position<'a>),
) {
    let mut stack = vec![(root, QuoteState::EVALUATED, Position::ROOT)];
    while let Some((view, outer, position)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() {
            visit(view, position);
            if !descend(view) {
                continue;
            }
        }
        let inside = if is_quote_form(view) {
            state.quoted()
        } else {
            state
        };
        let is_call = is_paren_list(view);
        let head = if is_call { list_head(view) } else { None };
        for (index, child) in view.children.iter().enumerate().rev() {
            stack.push((
                child,
                inside,
                Position {
                    is_operator: is_call && index == 0,
                    enclosing_head: head,
                },
            ));
        }
    }
}

// -- symbols -----------------------------------------------------------------

/// An atom's symbol text, past any reader prefix, lowercased and stripped of its
/// package qualifier — the spelling every comparison here is written in.
#[must_use]
pub fn normalized_symbol(text: &str) -> String {
    unqualified(text).to_ascii_lowercase()
}

/// The symbol an atom names, in the normalized spelling.
#[must_use]
pub fn symbol_name(view: &ExpressionView) -> Option<String> {
    atom_symbol_text(view)
        .filter(|text| !text.is_empty())
        .map(normalized_symbol)
}

/// Whether an atom is a lambda-list keyword such as `&rest` or `&optional`.
///
/// Compared on the *unqualified* text, because `&rest` is read as a symbol in
/// whatever package is current and a qualified spelling is legal if unusual.
#[must_use]
pub fn is_lambda_list_keyword(name: &str) -> bool {
    name.starts_with('&')
}

/// Whether this node is a form the reader folded away behind `#+` or `#-`.
///
/// Under this repository's dialect-aware parse a reader conditional and the form
/// it guards are read as a **single atom**, and `atom_text` returns that atom's
/// text with the `#+`/`#-` prefix still attached. So `#+sbcl (declare (fixnum
/// x))` is not a list with head `declare`; it is one atom whose text begins
/// `#+`. Every rule here that reasons about the *position* of a body form has to
/// decline such a body outright: it cannot see whether the hidden form is a
/// declaration, and guessing either way is a false positive in one direction and
/// a missed finding in the other.
///
/// Several rules in sibling packages have been written with guards that were
/// unreachable because they expected `#+sbcl` to be a separate node. It is not.
#[must_use]
pub fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

// -- body geometry -----------------------------------------------------------

/// A `(declare …)` form.
#[must_use]
pub fn is_declare(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, "declare"))
}

/// A `(declaim …)` form.
#[must_use]
pub fn is_declaim(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, "declaim"))
}

/// A string literal, which in a declaration section is a documentation string.
#[must_use]
pub fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

/// Where the body of a declaration-accepting form begins, by head.
///
/// Only forms whose prefix length is *fixed* are listed. `defmethod` is absent
/// on purpose: its qualifiers sit between the name and the specialized lambda
/// list, so its body starts at a position that has to be searched for rather
/// than counted to, and every rule here that needs it uses
/// [`defmethod_body_start`].
///
/// `handler-case`, `restart-case` and friends are absent because CLHS does not
/// admit a declaration at the position that looks like their body.
#[must_use]
pub fn body_start(head: &str) -> Option<usize> {
    let index = match head {
        // (defun name lambda-list . body) — the name may itself be a list, as in
        // `(defun (setf foo) …)`, so this counts to a fixed index rather than
        // looking for the first list.
        "defun" | "defmacro" | "define-compiler-macro" | "deftype" => 3,
        // (lambda lambda-list . body)
        "lambda" => 2,
        // (let bindings . body)
        "let" | "let*" | "flet" | "labels" | "macrolet" | "symbol-macrolet" | "prog" | "prog*" => 2,
        // (locally . body)
        "locally" => 1,
        // (multiple-value-bind vars form . body)
        "multiple-value-bind" | "destructuring-bind" | "with-slots" | "with-accessors" => 3,
        // (do bindings (end-test . result) . body)
        "do" | "do*" => 3,
        // (dolist (var list . result) . body)
        "dolist" | "dotimes" => 2,
        _ => return None,
    };
    Some(index)
}

/// Every head [`body_start`] answers for, as the head list a rule registers.
///
/// Kept beside `body_start` so that a head added to one and not the other is a
/// visible inconsistency rather than a rule that silently stops being reached.
pub const DECLARATION_BODY_HEADS: [&str; 21] = [
    "defun",
    "defmacro",
    "define-compiler-macro",
    "deftype",
    "lambda",
    "let",
    "let*",
    "flet",
    "labels",
    "macrolet",
    "symbol-macrolet",
    "prog",
    "prog*",
    "locally",
    "multiple-value-bind",
    "destructuring-bind",
    "with-slots",
    "with-accessors",
    "do",
    "do*",
    "dolist",
];

/// The head list every rule about a *body* registers, as the engine's head
/// index wants it: [`DECLARATION_BODY_HEADS`] plus `defmethod`, whose body start
/// is searched for rather than counted to.
///
/// A single constant so that the head index and [`body_start_of`] cannot drift
/// apart. A head in the index that `body_start_of` does not answer for is a
/// wasted dispatch; a head `body_start_of` answers for that the index omits is a
/// rule that silently never sees those forms, and no domain test can catch it.
pub const DECLARATION_BODY_RULE_HEADS: [NormalizedHead; 22] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("define-compiler-macro"),
    NormalizedHead::new("deftype"),
    NormalizedHead::new("lambda"),
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
    NormalizedHead::new("flet"),
    NormalizedHead::new("labels"),
    NormalizedHead::new("macrolet"),
    NormalizedHead::new("symbol-macrolet"),
    NormalizedHead::new("prog"),
    NormalizedHead::new("prog*"),
    NormalizedHead::new("locally"),
    NormalizedHead::new("multiple-value-bind"),
    NormalizedHead::new("destructuring-bind"),
    NormalizedHead::new("with-slots"),
    NormalizedHead::new("with-accessors"),
    NormalizedHead::new("do"),
    NormalizedHead::new("do*"),
    NormalizedHead::new("dolist"),
    NormalizedHead::new("defmethod"),
];

/// Every rule in this package models Common Lisp's declaration system and
/// nothing else.
///
/// Stated explicitly rather than left to the [`RuleDialectScope`] trait default,
/// which is also Common Lisp: a rule that *defaulted* to it and one that
/// *declares* it are indistinguishable at the call site, and the package's
/// engine tests assert the declaration.
pub const COMMON_LISP_ONLY: RuleDialectScope = RuleDialectScope::new(&[Dialect::CommonLisp]);

/// `defmethod`'s body start, which has to be searched for.
///
/// `(defmethod name qualifier* specialized-lambda-list . body)`: the qualifiers
/// are non-list objects, so the lambda list is the first `(…)` at or after index
/// 2 and the body starts just past it. A `defmethod` whose lambda list cannot be
/// found is `None` rather than a guess.
#[must_use]
pub fn defmethod_body_start(view: &ExpressionView) -> Option<usize> {
    view.children
        .iter()
        .enumerate()
        .skip(2)
        .find(|(_, child)| is_paren_list(child))
        .map(|(index, _)| index + 1)
}

/// The body start of any form this module models, `defmethod` included.
#[must_use]
pub fn body_start_of(view: &ExpressionView) -> Option<usize> {
    let head = list_head(view)?;
    if symbol_is(head, "defmethod") {
        return defmethod_body_start(view);
    }
    body_start(&normalized_symbol(head))
}

/// The index just past a form's declaration section.
///
/// CLHS 3.4.11 lets a body begin with any number of `declare` expressions and at
/// most one documentation string, in either order. A trailing string is *not* a
/// documentation string — `(defun f (x) (declare (fixnum x)) "result")` returns
/// that string — so a string is only counted into the section when some form
/// follows it.
#[must_use]
pub fn declaration_section_end(view: &ExpressionView, start: usize) -> usize {
    let mut index = start;
    let mut seen_doc = false;
    while let Some(child) = view.children.get(index) {
        if is_declare(child) {
            index += 1;
        } else if !seen_doc && is_string_literal(child) && index + 1 < view.children.len() {
            seen_doc = true;
            index += 1;
        } else {
            break;
        }
    }
    index
}

/// Whether a node's value is produced by `#.` at read time.
///
/// `#.(format nil "…")` is a *string* by the time the compiler sees the form, so
/// it is a documentation string — but statically it is a call to `format`, and
/// nothing here can tell that it will evaluate to a string rather than to a
/// declaration or an ordinary body form.
#[must_use]
pub fn carries_read_eval(view: &ExpressionView) -> bool {
    view.reader_prefixes.contains(&ReaderPrefix::ReadEval)
}

/// Whether a form's shape is too opaque for any positional judgement about its
/// body.
///
/// Both conditions were found by auditing SBCL's own sources, and neither was
/// anticipated:
///
/// - **A reader conditional anywhere among the form's children, the head
///   included.** `sbcl/src/compiler/globaldb.lisp` opens a definition with
///   `(#+sb-xc-host cl:defmacro #-sb-xc-host sb-xc:defmacro define-info-type
///   (…) (declare …) …)`. The head is a folded `#+` atom that still normalizes
///   to `defmacro`, so the head index dispatches — and every subsequent index is
///   shifted by the two conditional atoms, which put the lambda list where the
///   first body form is counted and made a correctly placed `declare` look
///   displaced. Scanning only from `start` misses this entirely, which is why
///   the scan begins at zero.
/// - **A `#.` read-time evaluation in the declaration section.**
///   `sbcl/src/code/save.lisp` writes its documentation string as `#.(format
///   nil "…" #+elf "…" #-elf "")`. That is a docstring at read time and a
///   `format` call statically, so the two `declare` forms after it read as
///   coming *after* a body form.
///
/// Declining both costs findings in exactly the files most saturated with
/// declarations. It is still the right trade: each was a false positive on
/// working, shipped code.
#[must_use]
pub fn form_shape_is_opaque(view: &ExpressionView, start: usize) -> bool {
    if view.children.iter().any(is_reader_conditional) {
        return true;
    }
    let section_end = declaration_section_end(view, start);
    view.children
        .iter()
        .take(section_end.saturating_add(1))
        .skip(start)
        .any(carries_read_eval)
}

// -- declaration specifiers --------------------------------------------------

/// The declaration identifiers that are *not* type specifiers, so that
/// `(fixnum x)` can be read as the abbreviated type declaration it is while
/// `(ignore x)` is not.
///
/// CLHS 3.3.1 lists these; `declaration` and `values` are included because both
/// are declaration identifiers a program may write and neither declares a
/// variable's type.
const NON_TYPE_DECLARATION_IDENTIFIERS: [&str; 11] = [
    "ignore",
    "ignorable",
    "special",
    "dynamic-extent",
    "optimize",
    "inline",
    "notinline",
    "ftype",
    "type",
    "declaration",
    "values",
];

/// One `(identifier …)` specifier inside a `declare` or `declaim`.
#[derive(Debug, Clone, Copy)]
pub struct Specifier<'a> {
    /// The normalized declaration identifier, e.g. `ignore` or `fixnum`.
    pub identifier: &'a ExpressionView,
    /// The whole specifier list.
    pub form: &'a ExpressionView,
}

/// Every `(identifier …)` specifier of a `declare`/`declaim` form.
///
/// The head itself is skipped, and any specifier that is not a `(…)` list is
/// dropped: `(declare optimize)` is malformed, and a malformed declaration is a
/// different rule's subject.
pub fn specifiers(view: &ExpressionView) -> impl Iterator<Item = Specifier<'_>> {
    view.children.iter().skip(1).filter_map(|form| {
        if !is_paren_list(form) {
            return None;
        }
        let identifier = form.children.first()?;
        Some(Specifier { identifier, form })
    })
}

/// The variables a `(ignore …)` or `(ignorable …)` specifier names.
///
/// `(ignore (function f))` names a *function*, not a variable, and is skipped:
/// only plain symbols are returned.
pub fn ignored_variables(view: &ExpressionView, identifier: &str) -> Vec<String> {
    let mut names = Vec::new();
    for specifier in specifiers(view) {
        if symbol_name(specifier.identifier).as_deref() != Some(identifier) {
            continue;
        }
        names.extend(
            specifier
                .form
                .children
                .iter()
                .skip(1)
                .filter_map(symbol_name),
        );
    }
    names
}

/// A type declaration found in a `declare`: the declared type and the variables
/// it applies to.
#[derive(Debug, Clone, Copy)]
pub struct TypeDeclaration<'a> {
    /// The type specifier as written.
    pub type_spec: &'a ExpressionView,
    /// The specifier list the declaration came from, for the finding's span.
    pub form: &'a ExpressionView,
    /// Index of the first variable name within [`TypeDeclaration::form`].
    first_variable: usize,
}

impl<'a> TypeDeclaration<'a> {
    /// The variables this declaration applies to.
    pub fn variables(&self) -> impl Iterator<Item = String> + use<'a> {
        self.form
            .children
            .iter()
            .skip(self.first_variable)
            .filter_map(symbol_name)
    }
}

/// Every variable type declaration in one `declare` form.
///
/// Both spellings: the long `(type fixnum x y)` and the abbreviated `(fixnum x
/// y)`. An abbreviated specifier whose identifier is one of
/// [`NON_TYPE_DECLARATION_IDENTIFIERS`] is not a type declaration, and neither
/// is one whose identifier is a `(…)` list — `((integer 0 10) x)` is not legal
/// abbreviated syntax, only `(type (integer 0 10) x)` is.
#[must_use]
pub fn type_declarations(view: &ExpressionView) -> Vec<TypeDeclaration<'_>> {
    let mut found = Vec::new();
    for specifier in specifiers(view) {
        let Some(identifier) = symbol_name(specifier.identifier) else {
            continue;
        };
        if identifier == "type" {
            if let Some(type_spec) = specifier.form.children.get(1) {
                found.push(TypeDeclaration {
                    type_spec,
                    form: specifier.form,
                    first_variable: 2,
                });
            }
            continue;
        }
        if NON_TYPE_DECLARATION_IDENTIFIERS.contains(&identifier.as_str()) {
            continue;
        }
        found.push(TypeDeclaration {
            type_spec: specifier.identifier,
            form: specifier.form,
            first_variable: 1,
        });
    }
    found
}

// -- the literal lattice -----------------------------------------------------

/// The kinds of value this package is willing to read off a literal.
///
/// Deliberately coarse and deliberately incomplete. Every rule that consults it
/// reports only when a literal's kind is known *and* the declared type provably
/// excludes it, so an unmodelled expression costs a missed finding and never a
/// false one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    Integer,
    Float,
    Ratio,
    Str,
    Character,
    /// `nil`, which is both the empty list and a symbol.
    Null,
    /// `t`.
    True,
    Keyword,
    /// A non-empty list: `(list 1 2)`, `(cons 1 2)`, `'(1 2)`.
    Cons,
}

/// Whether a token is an integer in the CL reader's sense.
///
/// `123`, `-4`, and `7.` are all integers; a trailing decimal point is a decimal
/// radix marker, not a float.
fn is_integer_token(text: &str) -> bool {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    let digits = digits.strip_suffix('.').unwrap_or(digits);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether a token is a float: digits, a decimal point with at least one digit
/// after it, and an optional exponent.
///
/// The "digit after the point" requirement is what keeps `7.` an integer.
fn is_float_token(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let Some((whole, rest)) = body.split_once('.') else {
        return false;
    };
    if !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let fraction = match rest.split_once(['e', 'E', 's', 'S', 'f', 'F', 'd', 'D', 'l', 'L']) {
        Some((fraction, exponent)) => {
            let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
            if exponent.is_empty() || !exponent.bytes().all(|byte| byte.is_ascii_digit()) {
                return false;
            }
            fraction
        }
        None => rest,
    };
    !fraction.is_empty() && fraction.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether a token is a ratio: `1/2`, `-3/4`.
fn is_ratio_token(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let Some((numerator, denominator)) = body.split_once('/') else {
        return false;
    };
    !numerator.is_empty()
        && !denominator.is_empty()
        && numerator.bytes().all(|byte| byte.is_ascii_digit())
        && denominator.bytes().all(|byte| byte.is_ascii_digit())
}

/// What kind of value an expression obviously is, when that is obvious.
///
/// `None` for everything else, which is most things: a variable reference, a
/// function call whose return type is not modelled, an arithmetic form. Callers
/// treat `None` as "say nothing".
///
/// A node carrying a reader-conditional prefix is `None` by construction: its
/// text begins `#+` and matches no literal shape.
#[must_use]
pub fn literal_kind(view: &ExpressionView) -> Option<LiteralKind> {
    // A quoted expression: `'foo`, `'(1 2)`, `'()`.
    if view.reader_prefixes.contains(&ReaderPrefix::Quote) {
        return quoted_literal_kind(view);
    }
    if let Some(text) = atom_text(view) {
        return atom_literal_kind(text);
    }
    if is_quote_form(view) {
        return view.children.get(1).and_then(quoted_literal_kind);
    }
    let head = list_head(view)?;
    let head = normalized_symbol(head);
    match head.as_str() {
        // `(list)` is nil; `(list x …)` is always a cons.
        "list" => Some(if view.children.len() > 1 {
            LiteralKind::Cons
        } else {
            LiteralKind::Null
        }),
        // `(cons a b)` and `(list* a … b)` with at least one argument are conses.
        "cons" => Some(LiteralKind::Cons),
        "list*" if view.children.len() > 2 => Some(LiteralKind::Cons),
        _ => None,
    }
}

/// The kind of a *quoted* expression, where a list is literal data rather than a
/// call.
fn quoted_literal_kind(view: &ExpressionView) -> Option<LiteralKind> {
    if let Some(text) = atom_text(view) {
        // `'nil` and `'t` read as the same objects `nil` and `t` do; any other
        // quoted atom is a symbol, which this lattice does not model beyond
        // keywords.
        let stripped = text.trim_start_matches('\'');
        return atom_literal_kind(stripped);
    }
    if !is_paren_list(view) {
        return None;
    }
    Some(if view.children.is_empty() {
        LiteralKind::Null
    } else {
        LiteralKind::Cons
    })
}

/// The kind of a bare atom's text.
fn atom_literal_kind(text: &str) -> Option<LiteralKind> {
    if text.starts_with('"') {
        return Some(LiteralKind::Str);
    }
    if text.starts_with("#\\") {
        return Some(LiteralKind::Character);
    }
    if text.starts_with('#') {
        // `#(1 2)`, `#+sbcl …`, `#'f`, `#b1010` — none of them modelled.
        return None;
    }
    let normalized = normalized_symbol(text);
    if normalized == "nil" || normalized == "()" {
        return Some(LiteralKind::Null);
    }
    if normalized == "t" {
        return Some(LiteralKind::True);
    }
    if text.starts_with(':') && text.len() > 1 {
        return Some(LiteralKind::Keyword);
    }
    if is_integer_token(text) {
        return Some(LiteralKind::Integer);
    }
    if is_float_token(text) {
        return Some(LiteralKind::Float);
    }
    if is_ratio_token(text) {
        return Some(LiteralKind::Ratio);
    }
    None
}

/// The exact set of [`LiteralKind`]s an atomic type specifier admits, for the
/// specifiers whose relationship to this lattice is fully enumerable.
///
/// `None` means "not modelled", and no rule may conclude anything from it. That
/// is the case for every compound specifier — `(or null string)`, `(integer 0
/// 10)`, `(satisfies p)` — and for the wide specifiers `t`, `atom`, `sequence`,
/// `array` and `vector`, whose membership questions are exactly the ones a
/// linter gets wrong. A string *is* a vector and *is* a sequence, and a rule
/// that forgot it would fire on correct code.
fn admitted_kinds(type_name: &str) -> Option<&'static [LiteralKind]> {
    use LiteralKind::{Character, Cons, Float, Integer, Keyword, Null, Ratio, Str, True};
    Some(match type_name {
        "null" => &[Null],
        "boolean" => &[Null, True],
        "keyword" => &[Keyword],
        "symbol" => &[Null, True, Keyword],
        "cons" => &[Cons],
        "list" => &[Null, Cons],
        "string" | "simple-string" | "base-string" | "simple-base-string" => &[Str],
        "character" | "base-char" | "standard-char" | "extended-char" => &[Character],
        "fixnum" | "bignum" | "integer" | "signed-byte" | "unsigned-byte" | "bit" => &[Integer],
        "float" | "single-float" | "double-float" | "short-float" | "long-float" => &[Float],
        "ratio" => &[Ratio],
        "rational" => &[Integer, Ratio],
        "real" | "number" => &[Integer, Float, Ratio],
        "hash-table" | "function" | "package" | "stream" | "pathname" | "readtable"
        | "random-state" | "restart" | "condition" => &[],
        _ => return None,
    })
}

/// Whether `type_spec` provably cannot contain a value of `kind`.
///
/// The one question this package's type reasoning is allowed to ask. `false` for
/// every specifier that is not modelled, so an unfamiliar type is silence.
#[must_use]
pub fn type_excludes(type_spec: &ExpressionView, kind: LiteralKind) -> bool {
    // Any compound specifier is declined outright. `(or null string)` is the
    // idiom these rules exist not to fire on.
    let Some(name) = symbol_name(type_spec) else {
        return false;
    };
    admitted_kinds(&name).is_some_and(|admitted| !admitted.contains(&kind))
}

/// Whether `type_spec` is a modelled specifier at all — used to state in a
/// message that the type was understood rather than merely unrecognised.
#[must_use]
pub fn is_modelled_type(type_spec: &ExpressionView) -> bool {
    symbol_name(type_spec).is_some_and(|name| admitted_kinds(&name).is_some())
}

/// A human-readable rendering of a modelled type specifier.
#[must_use]
pub fn type_text(type_spec: &ExpressionView) -> String {
    symbol_name(type_spec).unwrap_or_else(|| "the declared type".to_owned())
}

/// How a literal kind reads in a finding's message.
#[must_use]
pub const fn kind_text(kind: LiteralKind) -> &'static str {
    match kind {
        LiteralKind::Integer => "an integer",
        LiteralKind::Float => "a float",
        LiteralKind::Ratio => "a ratio",
        LiteralKind::Str => "a string",
        LiteralKind::Character => "a character",
        LiteralKind::Null => "NIL",
        LiteralKind::True => "T",
        LiteralKind::Keyword => "a keyword",
        LiteralKind::Cons => "a non-empty list",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    fn first_form(source: &str) -> (SyntaxTree, ByteSpan) {
        let parsed = tree(source);
        let span = parsed.root_view().children[0].span;
        (parsed, span)
    }

    #[test]
    fn a_declaration_in_plain_code_reads_as_evaluated() {
        let (parsed, span) = first_form("(let ((x 0)) (declare (string x)) x)");
        assert!(!is_unevaluated_at(&parsed, span));
    }

    #[test]
    fn a_declaration_inside_a_quote_reads_as_data() {
        let (parsed, span) = first_form("'(let ((x 0)) (declare (string x)) x)");
        assert!(is_unevaluated_at(&parsed, span));
    }

    #[test]
    fn a_declaration_inside_a_backquote_reads_as_data() {
        let (parsed, span) = first_form("`(let ((x 0)) (declare (string x)) x)");
        assert!(is_unevaluated_at(&parsed, span));
    }

    /// The two-counter model's reason for existing: a comma inside a hard quote
    /// is a comma character, not an escape back to code.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        let parsed = tree("'(a ,(let ((x 0)) (declare (string x)) x))");
        let mut span = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|head| head == "let") {
                span = Some(view.span);
            }
        });
        assert!(is_unevaluated_at(&parsed, span.expect("the let")));
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        let parsed = tree("`(a ,(let ((x 0)) (declare (string x)) x))");
        let mut span = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|head| head == "let") {
                span = Some(view.span);
            }
        });
        assert!(!is_unevaluated_at(&parsed, span.expect("the let")));
    }

    // -- body geometry -------------------------------------------------------

    #[test]
    fn a_defuns_body_starts_after_the_lambda_list() {
        assert_eq!(body_start("defun"), Some(3));
        assert_eq!(body_start("let"), Some(2));
        assert_eq!(body_start("locally"), Some(1));
        assert_eq!(body_start("if"), None);
    }

    #[test]
    fn every_listed_head_has_a_body_start() {
        for head in DECLARATION_BODY_HEADS {
            assert!(body_start(head).is_some(), "{head} has no body start");
        }
    }

    #[test]
    fn a_defmethods_body_starts_past_its_qualifiers() {
        let parsed = tree("(defmethod area :before ((s square)) (print s))");
        let form = &parsed.root_view().children[0];
        assert_eq!(defmethod_body_start(form), Some(4));

        let parsed = tree("(defmethod area ((s square)) (print s))");
        let form = &parsed.root_view().children[0];
        assert_eq!(defmethod_body_start(form), Some(3));
    }

    #[test]
    fn the_declaration_section_covers_declarations_and_one_docstring() {
        let parsed = tree("(defun f (x) \"doc\" (declare (fixnum x)) (+ x 1))");
        let form = &parsed.root_view().children[0];
        assert_eq!(declaration_section_end(form, 3), 5);
    }

    /// A string with nothing after it is the return value, not a docstring.
    #[test]
    fn a_trailing_string_is_not_a_docstring() {
        let parsed = tree("(defun f (x) (declare (fixnum x)) \"result\")");
        let form = &parsed.root_view().children[0];
        assert_eq!(declaration_section_end(form, 3), 4);
    }

    /// The prompt's trap, verified as a property of the parse rather than
    /// assumed: `#+sbcl (declare …)` is one atom whose text carries the prefix.
    #[test]
    fn a_reader_conditional_is_a_single_atom_carrying_its_prefix() {
        let parsed = tree("(defun f (x) #+sbcl (declare (fixnum x)) (+ x 1))");
        let form = &parsed.root_view().children[0];
        let guarded = &form.children[3];
        assert!(
            is_reader_conditional(guarded),
            "expected one atom beginning #+, got {:?}",
            atom_text(guarded)
        );
        assert!(
            !is_declare(guarded),
            "a reader-conditional declare must not read as a declare list"
        );
        assert!(form_shape_is_opaque(form, 3));
    }

    /// The two shapes the SBCL corpus audit turned up, as properties of
    /// [`form_shape_is_opaque`] rather than only of the rules that consult it.
    #[test]
    fn a_reader_conditional_head_makes_the_whole_form_opaque() {
        let parsed = tree(
            "(#+sb-xc-host cl:defmacro #-sb-xc-host sb-xc:defmacro m (a) (declare (fixnum a)) a)",
        );
        let form = &parsed.root_view().children[0];
        assert!(
            form_shape_is_opaque(form, 3),
            "a folded #+ head shifts every later index"
        );
    }

    #[test]
    fn a_read_time_evaluated_docstring_makes_the_body_opaque() {
        let parsed = tree("(defun f (x) #.(format nil \"doc\") (declare (fixnum x)) x)");
        let form = &parsed.root_view().children[0];
        assert!(carries_read_eval(&form.children[3]));
        assert!(form_shape_is_opaque(form, 3));
    }

    /// The guard must not swallow ordinary forms: a `defun` with a plain
    /// docstring and no reader syntax is transparent.
    #[test]
    fn an_ordinary_form_is_not_opaque() {
        let parsed = tree("(defun f (x) \"doc\" (declare (fixnum x)) x)");
        let form = &parsed.root_view().children[0];
        assert!(!form_shape_is_opaque(form, 3));
    }

    // -- specifiers ----------------------------------------------------------

    #[test]
    fn ignore_specifiers_name_their_variables() {
        let parsed = tree("(declare (ignore a b) (ignorable c))");
        let form = &parsed.root_view().children[0];
        assert_eq!(ignored_variables(form, "ignore"), vec!["a", "b"]);
        assert_eq!(ignored_variables(form, "ignorable"), vec!["c"]);
    }

    #[test]
    fn an_ignored_function_is_not_an_ignored_variable() {
        let parsed = tree("(declare (ignore (function f) x))");
        let form = &parsed.root_view().children[0];
        assert_eq!(ignored_variables(form, "ignore"), vec!["x"]);
    }

    #[test]
    fn both_type_declaration_spellings_are_read() {
        let parsed = tree("(declare (fixnum a b) (type string c))");
        let form = &parsed.root_view().children[0];
        let declarations = type_declarations(form);
        assert_eq!(declarations.len(), 2);
        assert_eq!(type_text(declarations[0].type_spec), "fixnum");
        assert_eq!(
            declarations[0].variables().collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(type_text(declarations[1].type_spec), "string");
        assert_eq!(declarations[1].variables().collect::<Vec<_>>(), vec!["c"]);
    }

    #[test]
    fn a_non_type_declaration_identifier_is_not_a_type() {
        let parsed = tree("(declare (ignore x) (special *y*) (optimize (speed 3)) (inline f))");
        let form = &parsed.root_view().children[0];
        assert!(type_declarations(form).is_empty());
    }

    // -- literals ------------------------------------------------------------

    fn kind_of(source: &str) -> Option<LiteralKind> {
        let parsed = tree(source);
        literal_kind(&parsed.root_view().children[0])
    }

    #[test]
    fn literals_read_as_their_kinds() {
        assert_eq!(kind_of("0"), Some(LiteralKind::Integer));
        assert_eq!(kind_of("-42"), Some(LiteralKind::Integer));
        assert_eq!(kind_of("7."), Some(LiteralKind::Integer));
        assert_eq!(kind_of("1.5"), Some(LiteralKind::Float));
        assert_eq!(kind_of("1.0e10"), Some(LiteralKind::Float));
        assert_eq!(kind_of("1/2"), Some(LiteralKind::Ratio));
        assert_eq!(kind_of("\"hi\""), Some(LiteralKind::Str));
        assert_eq!(kind_of("#\\a"), Some(LiteralKind::Character));
        assert_eq!(kind_of("nil"), Some(LiteralKind::Null));
        assert_eq!(kind_of("t"), Some(LiteralKind::True));
        assert_eq!(kind_of(":key"), Some(LiteralKind::Keyword));
    }

    #[test]
    fn an_unmodelled_expression_has_no_kind() {
        assert_eq!(kind_of("x"), None);
        assert_eq!(kind_of("(f x)"), None);
        assert_eq!(kind_of("(+ 1 2)"), None);
        assert_eq!(kind_of("#(1 2)"), None);
        assert_eq!(kind_of("#'car"), None);
    }

    #[test]
    fn list_building_calls_read_as_conses() {
        assert_eq!(kind_of("(list 1 2)"), Some(LiteralKind::Cons));
        assert_eq!(kind_of("(list)"), Some(LiteralKind::Null));
        assert_eq!(kind_of("(cons 1 2)"), Some(LiteralKind::Cons));
    }

    #[test]
    fn quoted_data_reads_as_its_kind() {
        assert_eq!(kind_of("'(1 2)"), Some(LiteralKind::Cons));
        assert_eq!(kind_of("'()"), Some(LiteralKind::Null));
        assert_eq!(kind_of("'nil"), Some(LiteralKind::Null));
    }

    // -- the lattice ---------------------------------------------------------

    fn excludes(type_source: &str, kind: LiteralKind) -> bool {
        let parsed = tree(type_source);
        type_excludes(&parsed.root_view().children[0], kind)
    }

    #[test]
    fn a_type_excludes_a_kind_it_cannot_contain() {
        assert!(excludes("string", LiteralKind::Integer));
        assert!(excludes("fixnum", LiteralKind::Str));
        assert!(excludes("fixnum", LiteralKind::Null));
        assert!(excludes("null", LiteralKind::Cons));
        assert!(excludes("cons", LiteralKind::Null));
        assert!(excludes("integer", LiteralKind::Float));
    }

    #[test]
    fn a_type_does_not_exclude_a_kind_it_contains() {
        assert!(!excludes("fixnum", LiteralKind::Integer));
        assert!(!excludes("string", LiteralKind::Str));
        assert!(!excludes("list", LiteralKind::Null));
        assert!(!excludes("list", LiteralKind::Cons));
        assert!(!excludes("symbol", LiteralKind::Null));
        assert!(!excludes("number", LiteralKind::Float));
        assert!(!excludes("real", LiteralKind::Ratio));
    }

    /// The whole point of declining compound specifiers: the widening idiom
    /// `(or null hash-table)` around a `nil` initform is correct code.
    #[test]
    fn a_compound_specifier_excludes_nothing() {
        assert!(!excludes("(or null string)", LiteralKind::Null));
        assert!(!excludes("(or null hash-table)", LiteralKind::Null));
        assert!(!excludes("(integer 0 10)", LiteralKind::Str));
        assert!(!excludes("(satisfies evenp)", LiteralKind::Str));
        assert!(!excludes(
            "(simple-array double-float (*))",
            LiteralKind::Null
        ));
    }

    /// A string is a vector and a sequence, and an unmodelled wide type must
    /// stay unmodelled rather than be guessed at.
    #[test]
    fn wide_types_are_not_modelled() {
        for wide in ["t", "atom", "sequence", "array", "vector", "simple-vector"] {
            let parsed = tree(wide);
            assert!(
                !is_modelled_type(&parsed.root_view().children[0]),
                "{wide} must not be modelled"
            );
            for kind in [
                LiteralKind::Str,
                LiteralKind::Null,
                LiteralKind::Cons,
                LiteralKind::Integer,
            ] {
                assert!(!excludes(wide, kind), "{wide} must exclude nothing");
            }
        }
    }

    #[test]
    fn an_unknown_user_type_excludes_nothing() {
        assert!(!excludes("my-widget", LiteralKind::Str));
        assert!(!excludes("point", LiteralKind::Null));
    }

    #[test]
    fn a_lambda_list_keyword_is_recognised_by_its_ampersand() {
        assert!(is_lambda_list_keyword("&rest"));
        assert!(is_lambda_list_keyword("&optional"));
        assert!(!is_lambda_list_keyword("x"));
    }
}
