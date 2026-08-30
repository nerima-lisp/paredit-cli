//! `lfe-illegal-guard-call` detection: a module-qualified call inside a `when`
//! guard that Erlang's guard sublanguage does not permit.
//!
//! # This is a compile error, not a style preference
//!
//! Erlang restricts guards to a fixed set of BIFs and operators. LFE inherits
//! that restriction wholesale: `lfe_lint.erl:116` defines
//! `format_error(illegal_guard) -> <<"illegal guard expression">>`, and
//! `check_gexpr/4` rejects anything outside the permitted set.
//!
//! The relevant clauses are `lfe_lint.erl:1812-1822`:
//!
//! ```text
//! check_gexpr([call,?Q(erlang),?Q(Fun)|As], Env, L, St0) ->
//!     ...
//!         false -> illegal_guard_error(L, St1)
//!     end;
//! check_gexpr([call|_], _, L, St) ->              %Other calls not allowed
//!     illegal_guard_error(L, St);
//! ```
//!
//! Verified against LFE 2.2.0 on Erlang/OTP 27.3.4.15 rather than read off the
//! source. Compiling this module:
//!
//! ```text
//! (defun b ((x) (when (call 'lists 'member x '(1 2))) 'bad) ((_) 'other))
//! (defun c ((x) (when (lists:member x '(1 2)))        'bad) ((_) 'other))
//! (defun d ((x) (when (call 'erlang 'is_atom x))      'good) ((_) 'other))
//! ```
//!
//! produced, from `lfec`:
//!
//! ```text
//! p1.lfe:14: illegal guard expression
//! p1.lfe:19: illegal guard expression
//! ```
//!
//! — `b` and `c` rejected, `d` accepted. Further probes pinned the boundary
//! exactly:
//!
//! | guard expression                    | `lfec` verdict |
//! |-------------------------------------|----------------|
//! | `(erlang:is_atom x)`                | accepted       |
//! | `(erlang:list_to_atom x)`           | **illegal**    |
//! | `(call 'erlang 'list_to_atom x)`    | **illegal**    |
//! | `(erlang:is_record x)` (arity 1)    | **illegal**    |
//! | `(: erlang is_atom x)`              | accepted       |
//! | `(: lists member x '(1 2))`         | **illegal**    |
//!
//! So the permitted case is narrow: module `erlang`, a function in the guard
//! BIF set, *at a permitted arity*.
//!
//! # Why this rule is restricted to module-qualified calls
//!
//! `lfe_lint` also rejects an unqualified call to a user function in a guard,
//! and that is a much more common defect. This rule does **not** report it,
//! because it cannot be told apart from a macro call without an environment.
//!
//! LFE expands macros before linting, so a macro whose expansion is a legal
//! guard test is legal in a guard. `clj.lfe` ships many of exactly that shape
//! and uses them in its own guards — `clj.lfe:567` is
//! `([`#(,key ,_)] (when (atom? key)) 'true)`, where `atom?` is a macro
//! expanding to `is_atom`. This was verified rather than assumed: compiling
//!
//! ```text
//! (defmacro atom? (x) `(is_atom ,x))
//! (defun a ((x) (when (atom? x)) 'macro-ok) ((_) 'other))
//! ```
//!
//! produced **no diagnostic at all**. A structural rule that flagged every
//! head in a guard outside the BIF set would report every such macro use, and
//! `binding_table()` is empty for LFE, so there is no sound way to tell which
//! names are macros.
//!
//! Module-qualified calls have no such ambiguity:
//!
//! - `call` is an LFE **core form** (`lfe_internal.erl:is_core_form(call)`),
//!   and redefining one is itself an error (`redefine_core_form`), so
//!   `(call …)` cannot be a user macro.
//! - `mod:fun` and `(: mod fun …)` are resolved as remote calls, not looked up
//!   in the macro environment.
//!
//! So every finding this rule reports is a call the compiler will reject.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};

use crate::support::{QuoteState, atom_text, head_symbol, is_paren_list};

/// LFE only. Erlang's guard restriction is an LFE-specific inheritance;
/// `mod:fun` is a keyword in Clojure, a symbol in Scheme, and `when` is an
/// ordinary conditional macro in Common Lisp and Emacs Lisp, so widening the
/// scope would report on code that has nothing to do with Erlang guards.
pub const DIALECTS: [Dialect; 1] = [Dialect::Lfe];

/// Every `{name, arity}` Erlang permits in a guard, for OTP 27.3.4.15.
///
/// Not transcribed from documentation. LFE delegates the question to Erlang at
/// `lfe_internal.erl:43`:
///
/// ```text
/// is_guard_bif(Op, Ar) ->
///     erl_internal:guard_bif(Op, Ar)
///         orelse erl_internal:arith_op(Op, Ar)
///         orelse erl_internal:bool_op(Op, Ar)
///         orelse erl_internal:comp_op(Op, Ar).
/// ```
///
/// so this table was produced by *running* that predicate over
/// `erlang:module_info(exports)` plus every operator spelling, under the same
/// OTP the probes used. It reported `GUARD_SET_COUNT 67`, and
/// `tests::the_guard_table_matches_the_enumerated_size` pins that count so a
/// hand edit cannot silently drop an entry.
///
/// Sorted by name then arity, which [`is_guard_bif`] relies on.
pub const GUARD_BIFS: [(&str, usize); 67] = [
    ("*", 2),
    ("+", 1),
    ("+", 2),
    ("-", 1),
    ("-", 2),
    ("/", 2),
    ("/=", 2),
    ("<", 2),
    ("=/=", 2),
    ("=:=", 2),
    ("=<", 2),
    ("==", 2),
    (">", 2),
    (">=", 2),
    ("abs", 1),
    ("and", 2),
    ("band", 2),
    ("binary_part", 2),
    ("binary_part", 3),
    ("bit_size", 1),
    ("bnot", 1),
    ("bor", 2),
    ("bsl", 2),
    ("bsr", 2),
    ("bxor", 2),
    ("byte_size", 1),
    ("ceil", 1),
    ("div", 2),
    ("element", 2),
    ("float", 1),
    ("floor", 1),
    ("hd", 1),
    ("is_atom", 1),
    ("is_binary", 1),
    ("is_bitstring", 1),
    ("is_boolean", 1),
    ("is_float", 1),
    ("is_function", 1),
    ("is_function", 2),
    ("is_integer", 1),
    ("is_list", 1),
    ("is_map", 1),
    ("is_map_key", 2),
    ("is_number", 1),
    ("is_pid", 1),
    ("is_port", 1),
    ("is_record", 2),
    ("is_record", 3),
    ("is_reference", 1),
    ("is_tuple", 1),
    ("length", 1),
    ("map_get", 2),
    ("map_size", 1),
    ("max", 2),
    ("min", 2),
    ("node", 0),
    ("node", 1),
    ("not", 1),
    ("or", 2),
    ("rem", 2),
    ("round", 1),
    ("self", 0),
    ("size", 1),
    ("tl", 1),
    ("trunc", 1),
    ("tuple_size", 1),
    ("xor", 2),
];

/// The only module whose functions may be called, qualified, from a guard.
pub const GUARD_MODULE: &str = "erlang";

/// Whether `name/arity` is callable from a guard as `erlang:name`.
#[must_use]
pub fn is_guard_bif(name: &str, arity: usize) -> bool {
    GUARD_BIFS
        .binary_search_by(|(entry, entry_arity)| entry.cmp(&name).then(entry_arity.cmp(&arity)))
        .is_ok()
}

/// How a qualified call was spelled, which the message quotes back so the
/// report points at the syntax the author actually wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallSyntax {
    /// `(call 'mod 'fun …)` — the `call` core form.
    CallForm,
    /// `(mod:fun …)` — a colon in the head atom.
    Colon,
    /// `(: mod fun …)` — LFE 1.x remote-call syntax, still accepted by 2.2.0.
    ColonForm,
}

impl CallSyntax {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::CallForm => "(call 'mod 'fun …)",
            Self::Colon => "mod:fun",
            Self::ColonForm => "(: mod fun …)",
        }
    }
}

/// Why a qualified call in a guard is not permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Any module other than `erlang`.
    NotErlangModule,
    /// `erlang:` but not a guard BIF at all.
    NotAGuardBif,
    /// `erlang:` and a guard BIF name, but at an arity guards do not permit.
    WrongArity,
    /// The module or function is not a literal, so it cannot be `erlang:` at
    /// compile time — `(call M F x)` with variables.
    NotLiteral,
}

/// One illegal qualified call inside a guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IllegalCall {
    /// The whole offending call, which is what gets reported.
    pub span: ByteSpan,
    pub module: String,
    pub function: String,
    pub arity: usize,
    pub syntax: CallSyntax,
    pub reason: Reason,
}

/// An atom's text with a single leading `'` removed.
///
/// The reader hands back `'erlang` as an `Atom` whose `text` still carries the
/// quote, with a `Quote` reader prefix alongside. Both spellings of a
/// qualified call name their module as a literal, but only the `call` form
/// quotes it.
fn literal_atom(view: &ExpressionView, quoted: bool) -> Option<&str> {
    let text = atom_text(view)?;
    if quoted {
        let has_quote = view
            .reader_prefixes
            .iter()
            .any(|prefix| matches!(prefix, paredit_core_syntax::sexpr::ReaderPrefix::Quote));
        if !has_quote {
            return None;
        }
        text.strip_prefix('\'')
    } else {
        view.reader_prefixes.is_empty().then_some(text)
    }
}

/// Splits `mod:fun` into its two halves, or `None` if the head is not a
/// module-qualified name at all.
///
/// The colon test alone is not enough, and getting this wrong is not
/// hypothetical: **`=:=` contains a colon**. So do `=:=`'s relatives across
/// the Lisp family. Splitting on the first colon reads `=:=` as module `=`
/// calling function `=`, which is a false positive on the single most common
/// comparison operator in LFE guards — and one that would have fired on
/// essentially every guarded clause in the corpus. This package's own unit
/// tests caught it before the corpus sweep did.
///
/// The test is therefore that both halves look like unquoted Erlang atoms.
/// A bar-quoted atom (`|foo:bar|`) starts with `|`, which is not a lowercase
/// letter, so it is rejected too — the colon inside the bars is part of the
/// name rather than a qualifier.
fn split_qualified(head: &str) -> Option<(&str, &str)> {
    let (module, function) = head.split_once(':')?;
    (is_unquoted_atom_name(module) && is_unquoted_atom_name(function)).then_some((module, function))
}

/// Whether `text` spells an unquoted atom, the way a module or function name
/// in a remote call has to.
///
/// Starts with a lowercase letter, then letters, digits, `_`, `@`, `.` or `-`.
/// The `-` is there because LFE atoms permit it and LFE code uses it freely
/// (`rebar3-lfe`), unlike Erlang's own unquoted atom grammar; `.` because
/// dotted module names are legal.
fn is_unquoted_atom_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    bytes.all(|byte| {
        byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'@' || byte == b'.' || byte == b'-'
    })
}

/// Classifies a call whose module and function are known.
fn verdict(module: &str, function: &str, arity: usize) -> Option<Reason> {
    if module != GUARD_MODULE {
        return Some(Reason::NotErlangModule);
    }
    if is_guard_bif(function, arity) {
        return None;
    }
    // The name is a guard BIF at *some* arity, so the defect is the arity
    // rather than the choice of function; saying so makes the report
    // actionable.
    let known_name = GUARD_BIFS.iter().any(|(entry, _)| *entry == function);
    Some(if known_name {
        Reason::WrongArity
    } else {
        Reason::NotAGuardBif
    })
}

/// Examines one node for being an illegal qualified call.
///
/// Returns `None` for anything that is not a qualified call at all, which is
/// the overwhelming majority of guard nodes.
#[must_use]
fn examine_call(view: &ExpressionView) -> Option<IllegalCall> {
    let head = head_symbol(view)?;
    let children = &view.children;

    // `(call 'mod 'fun …)` and `(: mod fun …)` share a shape: the module and
    // function are the next two elements and the arity is what remains.
    if head == "call" || head == ":" {
        let quoted = head == "call";
        let syntax = if quoted {
            CallSyntax::CallForm
        } else {
            CallSyntax::ColonForm
        };
        let arity = children.len().saturating_sub(3);
        let (Some(module), Some(function)) = (
            children.get(1).and_then(|view| literal_atom(view, quoted)),
            children.get(2).and_then(|view| literal_atom(view, quoted)),
        ) else {
            // `(call M F x)`: not a literal `'erlang`, so `check_gexpr`'s
            // catch-all `[call|_]` clause rejects it.
            return Some(IllegalCall {
                span: view.span,
                module: "?".to_owned(),
                function: "?".to_owned(),
                arity,
                syntax,
                reason: Reason::NotLiteral,
            });
        };
        let reason = verdict(module, function, arity)?;
        return Some(IllegalCall {
            span: view.span,
            module: module.to_owned(),
            function: function.to_owned(),
            arity,
            syntax,
            reason,
        });
    }

    // `(mod:fun …)`. The reader keeps the colon inside the head atom, so this
    // is a text test rather than a structural one.
    let (module, function) = split_qualified(head)?;
    let arity = children.len().saturating_sub(1);
    let reason = verdict(module, function, arity)?;
    Some(IllegalCall {
        span: view.span,
        module: module.to_owned(),
        function: function.to_owned(),
        arity,
        syntax: CallSyntax::Colon,
        reason,
    })
}

/// Every illegal qualified call inside one `(when …)` guard.
///
/// `guard` is the `when` form itself, as the dispatcher hands it over.
///
/// Walks only the guard's own subtree — guards are small — so this never
/// materializes the document. Quoted data is not descended into: a `'(…)` in a
/// guard is a literal list, and `mod:fun` inside it is a symbol rather than a
/// call.
#[must_use]
pub fn collect_in_guard(dialect: Dialect, guard: &ExpressionView) -> Vec<IllegalCall> {
    if !DIALECTS.contains(&dialect) {
        return Vec::new();
    }
    if head_symbol(guard) != Some("when") {
        return Vec::new();
    }
    let mut found = Vec::new();
    // The guard tests are everything after the `when` head.
    let mut stack: Vec<(&ExpressionView, QuoteState)> = guard.children[1..]
        .iter()
        .map(|view| {
            let state = QuoteState::EVALUATED.after_prefixes(view);
            (view, state)
        })
        .collect();
    while let Some((view, state)) = stack.pop() {
        if state.is_pruned() {
            continue;
        }
        if !state.is_data() && is_paren_list(view) {
            if let Some(item) = examine_call(view) {
                found.push(item);
            }
        }
        let inner = state.after_quoting_head(view);
        for child in &view.children {
            stack.push((child, inner.after_prefixes(child)));
        }
    }
    found.sort_by_key(|item| item.span.start().get());
    found
}

/// How many qualified calls this rule adjudicated in one guard: the
/// denominator for a zero-finding sweep.
///
/// Counting only illegal ones would make the denominator equal the numerator,
/// so a clean corpus would report "0 findings over 0 candidates" — the
/// false-clean a denominator exists to rule out. This counts *every*
/// module-qualified call in a guard, legal or not, which is the question the
/// rule actually adjudicates.
#[must_use]
pub fn candidate_count_in_guard(dialect: Dialect, guard: &ExpressionView) -> usize {
    if !DIALECTS.contains(&dialect) || head_symbol(guard) != Some("when") {
        return 0;
    }
    let mut count = 0;
    let mut stack: Vec<(&ExpressionView, QuoteState)> = guard.children[1..]
        .iter()
        .map(|view| {
            let state = QuoteState::EVALUATED.after_prefixes(view);
            (view, state)
        })
        .collect();
    while let Some((view, state)) = stack.pop() {
        if state.is_pruned() {
            continue;
        }
        if !state.is_data() && is_paren_list(view) && is_qualified_call(view) {
            count += 1;
        }
        let inner = state.after_quoting_head(view);
        for child in &view.children {
            stack.push((child, inner.after_prefixes(child)));
        }
    }
    count
}

/// Whether a node is a module-qualified call in any of the three spellings,
/// regardless of whether it is permitted.
fn is_qualified_call(view: &ExpressionView) -> bool {
    let Some(head) = head_symbol(view) else {
        return false;
    };
    if head == "call" || head == ":" {
        return true;
    }
    split_qualified(head).is_some()
}

/// Every illegal qualified guard call in a whole document.
///
/// Only used by this package's own corpus tests and cost measurements; the
/// engine drives the rule one `when` form at a time through the head index.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<IllegalCall> {
    walk_guards(dialect, tree, collect_in_guard)
}

/// The document-wide denominator, for the same reason.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    walk_guards(dialect, tree, |dialect, guard| {
        vec![candidate_count_in_guard(dialect, guard)]
    })
    .into_iter()
    .sum()
}

/// Applies `per_guard` to every `(when …)` form in the document.
fn walk_guards<T>(
    dialect: Dialect,
    tree: &SyntaxTree,
    per_guard: impl Fn(Dialect, &ExpressionView) -> Vec<T>,
) -> Vec<T> {
    if !DIALECTS.contains(&dialect) {
        return Vec::new();
    }
    let root = tree.root_view();
    let mut out = Vec::new();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if head_symbol(view) == Some("when") {
            out.extend(per_guard(dialect, view));
        }
        stack.extend(view.children.iter());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guards(source: &str) -> Vec<IllegalCall> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Lfe).expect("parse");
        let mut found = collect(Dialect::Lfe, &tree);
        found.sort_by_key(|item| item.span.start().get());
        found
    }

    fn reasons(source: &str) -> Vec<Reason> {
        guards(source).into_iter().map(|item| item.reason).collect()
    }

    /// The table came from running `erl_internal` under OTP 27.3.4.15, which
    /// reported `GUARD_SET_COUNT 67`. Pinning the length means a hand edit
    /// that drops an entry fails rather than silently narrowing the rule into
    /// false positives.
    #[test]
    fn the_guard_table_matches_the_enumerated_size() {
        assert_eq!(GUARD_BIFS.len(), 67);
    }

    /// `is_guard_bif` binary-searches, so the table has to be sorted.
    #[test]
    fn the_guard_table_is_sorted() {
        let mut sorted = GUARD_BIFS;
        sorted.sort_unstable();
        assert_eq!(sorted, GUARD_BIFS, "GUARD_BIFS must stay sorted");
    }

    #[test]
    fn the_guard_bif_lookup_respects_arity() {
        assert!(is_guard_bif("is_atom", 1));
        assert!(!is_guard_bif("is_atom", 2));
        assert!(is_guard_bif("is_record", 2));
        assert!(is_guard_bif("is_record", 3));
        assert!(!is_guard_bif("is_record", 1));
        assert!(is_guard_bif("node", 0));
        assert!(is_guard_bif("node", 1));
        assert!(!is_guard_bif("list_to_atom", 1));
    }

    // -- the three spellings, each verified against lfec ------------------

    #[test]
    fn a_non_erlang_module_is_illegal_in_every_spelling() {
        assert_eq!(
            reasons("(when (lists:member x '(1 2)))"),
            vec![Reason::NotErlangModule]
        );
        assert_eq!(
            reasons("(when (call 'lists 'member x '(1 2)))"),
            vec![Reason::NotErlangModule]
        );
        assert_eq!(
            reasons("(when (: lists member x '(1 2)))"),
            vec![Reason::NotErlangModule]
        );
    }

    #[test]
    fn an_erlang_guard_bif_is_permitted_in_every_spelling() {
        assert!(guards("(when (erlang:is_atom x))").is_empty());
        assert!(guards("(when (call 'erlang 'is_atom x))").is_empty());
        assert!(guards("(when (: erlang is_atom x))").is_empty());
    }

    #[test]
    fn an_erlang_non_guard_bif_is_illegal() {
        assert_eq!(
            reasons("(when (erlang:list_to_atom x))"),
            vec![Reason::NotAGuardBif]
        );
        assert_eq!(
            reasons("(when (call 'erlang 'list_to_atom x))"),
            vec![Reason::NotAGuardBif]
        );
    }

    /// `lfec` rejected `(erlang:is_record x)` at arity 1 although
    /// `is_record/2` and `/3` are both guard BIFs.
    #[test]
    fn a_guard_bif_at_the_wrong_arity_is_illegal() {
        assert_eq!(
            reasons("(when (erlang:is_record x))"),
            vec![Reason::WrongArity]
        );
        assert!(guards("(when (erlang:is_record x 'rec))").is_empty());
    }

    /// `(call M F x)` cannot be `'erlang` at compile time, and `check_gexpr`'s
    /// catch-all `[call|_]` clause rejects it.
    #[test]
    fn a_call_with_a_non_literal_module_is_illegal() {
        assert_eq!(reasons("(when (call mod fun x))"), vec![Reason::NotLiteral]);
    }

    // -- the macro false-positive this rule exists to avoid ---------------

    /// The whole reason this rule is restricted to qualified calls. `atom?` is
    /// a `clj.lfe` macro expanding to `is_atom`, and `lfec` compiles it in a
    /// guard with no diagnostic.
    #[test]
    fn an_unqualified_call_is_never_reported() {
        assert!(guards("(when (atom? x))").is_empty());
        assert!(guards("(when (my-own-predicate x))").is_empty());
        assert!(guards("(when (is_atom x))").is_empty());
    }

    // -- nesting and quoting ----------------------------------------------

    #[test]
    fn a_call_nested_inside_a_boolean_operator_is_reached() {
        assert_eq!(
            reasons("(when (andalso (is_atom x) (lists:member x '(1 2))))"),
            vec![Reason::NotErlangModule]
        );
        assert_eq!(
            reasons("(when (orelse (lists:member x y) (dict:is_key k d)))"),
            vec![Reason::NotErlangModule, Reason::NotErlangModule]
        );
    }

    /// A `mod:fun` inside quoted data is a symbol, not a call.
    #[test]
    fn a_qualified_name_inside_quoted_data_is_not_a_call() {
        assert!(guards("(when (=:= x '(lists:member a b)))").is_empty());
        assert!(guards("(when (=:= x (quote (lists:member a b))))").is_empty());
    }

    /// But an unquote escapes back to code, so a call there is real. LFE
    /// genuinely uses `,` for unquote, unlike Carp.
    #[test]
    fn an_unquoted_call_inside_a_template_is_still_code() {
        assert_eq!(
            reasons("(when (=:= x `(a ,(lists:member p q))))"),
            vec![Reason::NotErlangModule]
        );
    }

    /// A hard quote never clears, so a `,` inside `'(…)` stays data.
    #[test]
    fn an_unquote_inside_a_hard_quote_stays_data() {
        assert!(guards("(when (=:= x '(a ,(lists:member p q))))").is_empty());
    }

    // -- shapes that must not be read as qualified calls -------------------

    #[test]
    fn a_head_without_a_module_part_is_not_a_qualified_call() {
        assert!(guards("(when (:foo x))").is_empty());
        assert!(guards("(when (foo: x))").is_empty());
    }

    /// **`=:=` contains a colon.** Splitting a head on its first colon without
    /// checking that both halves are atom names reads it as module `=` calling
    /// function `=`, which false-positives on the commonest comparison
    /// operator in LFE guards. This was a real defect in this rule, caught by
    /// the quoting tests before the corpus sweep ran.
    #[test]
    fn a_comparison_operator_containing_a_colon_is_not_a_qualified_call() {
        assert!(guards("(when (=:= x 'one))").is_empty());
        assert!(guards("(when (=/= x 'one))").is_empty());
        assert!(guards("(when (andalso (=:= a b) (=:= c d)))").is_empty());
        // And it must not be counted as a candidate either, or the
        // denominator would claim the rule adjudicated something it did not.
        let tree =
            SyntaxTree::parse_with_dialect("(when (=:= x 'one))", Dialect::Lfe).expect("parse");
        assert_eq!(candidate_count(Dialect::Lfe, &tree), 0);
    }

    /// A bar-quoted atom carries its colon inside the name.
    #[test]
    fn a_bar_quoted_atom_is_not_a_qualified_call() {
        assert!(guards("(when (|foo:bar| x))").is_empty());
    }

    /// An uppercase or otherwise non-atom module part is not an unquoted
    /// remote call, so it is left alone rather than guessed at.
    #[test]
    fn a_non_atom_module_part_is_not_a_qualified_call() {
        assert!(guards("(when (Mod:fun x))").is_empty());
        assert!(guards("(when (_x:fun y))").is_empty());
    }

    /// The names LFE code really uses have `_`, `-` and digits in them, and
    /// those must still be recognized or the rule would quietly stop working
    /// on most modules.
    #[test]
    fn ordinary_module_spellings_are_recognized() {
        assert_eq!(
            reasons("(when (lfe_io:format x))"),
            vec![Reason::NotErlangModule]
        );
        assert_eq!(
            reasons("(when (rebar3-lfe:run x))"),
            vec![Reason::NotErlangModule]
        );
        assert_eq!(reasons("(when (mod2:go x))"), vec![Reason::NotErlangModule]);
    }

    /// LFE really does treat `[…]` and `(…)` as the same list, so
    /// `[lists:member x y]` in a guard *is* an illegal call and this is a
    /// deliberate false negative rather than a correctness claim.
    ///
    /// [`head_symbol`] is paren-only because `defsyntax` rules spell their
    /// patterns with brackets — `([('else . b)] (begin . b))` — and reading a
    /// head off one would invent operators out of pattern syntax. The
    /// syntax-template gate now covers that case, but the recall lost here is
    /// small (brackets are rare in LFE call position) and the direction is the
    /// safe one.
    ///
    /// Pinned so that relaxing `head_symbol` to accept any list fails a test
    /// rather than silently changing what the rule reports. Mutation-testing
    /// found the paren-only test killed nothing until this existed.
    #[test]
    fn a_bracket_list_is_deliberately_not_read_as_a_call() {
        assert!(guards("(when [lists:member x y])").is_empty());
        assert!(guards("(when (=:= x [lists:member]))").is_empty());
        // And it must not inflate the denominator either.
        let tree = SyntaxTree::parse_with_dialect("(when [lists:member x y])", Dialect::Lfe)
            .expect("parse");
        assert_eq!(candidate_count(Dialect::Lfe, &tree), 0);
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        for dialect in [Dialect::CommonLisp, Dialect::Clojure, Dialect::Scheme] {
            let tree = SyntaxTree::parse_with_dialect("(when (lists:member x y))", dialect)
                .expect("parse");
            assert!(collect(dialect, &tree).is_empty());
        }
    }

    // -- the denominator ---------------------------------------------------

    /// The denominator must be able to exceed the numerator, or it is not a
    /// denominator. A guard full of *legal* qualified calls has candidates and
    /// no findings.
    #[test]
    fn correct_code_has_candidates_and_no_findings() {
        let source = "(when (andalso (erlang:is_atom x) (erlang:is_list y) (: erlang is_tuple z)))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Lfe).expect("parse");
        assert_eq!(candidate_count(Dialect::Lfe, &tree), 3);
        assert!(collect(Dialect::Lfe, &tree).is_empty());
    }

    #[test]
    fn the_denominator_counts_legal_and_illegal_alike() {
        let source = "(when (andalso (erlang:is_atom x) (lists:member y z)))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Lfe).expect("parse");
        assert_eq!(candidate_count(Dialect::Lfe, &tree), 2);
        assert_eq!(collect(Dialect::Lfe, &tree).len(), 1);
    }

    /// An unqualified guard has no candidates at all, which is what makes a
    /// clean sweep over it uninformative rather than reassuring.
    #[test]
    fn an_unqualified_guard_has_no_candidates() {
        let tree =
            SyntaxTree::parse_with_dialect("(when (is_atom x))", Dialect::Lfe).expect("parse");
        assert_eq!(candidate_count(Dialect::Lfe, &tree), 0);
    }

    #[test]
    fn the_span_covers_the_whole_call() {
        let source = "(when (lists:member x '(1 2)))";
        let found = guards(source);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].span.slice(source), "(lists:member x '(1 2))");
        assert_eq!(found[0].module, "lists");
        assert_eq!(found[0].function, "member");
        assert_eq!(found[0].arity, 2);
        assert_eq!(found[0].syntax, CallSyntax::Colon);
    }

    /// The guard sits inside a real clause in practice; the walk has to reach
    /// it from the top of a `defun`.
    #[test]
    fn a_guard_inside_a_defun_clause_is_reached() {
        assert_eq!(
            reasons("(defun f ((x) (when (lists:member x '(1 2))) 'yes) ((_) 'no))"),
            vec![Reason::NotErlangModule]
        );
    }
}
