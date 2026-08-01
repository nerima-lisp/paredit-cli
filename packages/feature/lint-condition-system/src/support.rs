//! What the condition-system rules share: which parts of a file are *code*,
//! and what the file's own `define-condition` forms say about each other.
//!
//! Two things every rule here needs and neither the engine nor `core/syntax`
//! provides:
//!
//! - **Evaluation context.** A `(restart-case …)` inside `'(…)` is a list of
//!   symbols, not a restart. The lint engine's dispatch walks into quoted data
//!   like any other subtree and [`RuleContext`] carries no parent pointer, so a
//!   head-matched node cannot tell on its own whether it is code.
//!   [`is_unevaluated_at`] answers that by descending from the root along the
//!   single chain of nodes whose span contains the candidate's — depth-many
//!   steps, not tree-many — and is called only once a rule already has a
//!   finding to report.
//! - **The file's condition hierarchy.** `signal`, `ignore-errors` and
//!   `define-condition` are only diagnosable against the *other*
//!   `define-condition` forms in the same file. [`ConditionHierarchy::collect`]
//!   reads them, and is likewise built lazily: a file whose `signal` call names
//!   no literal type never pays for it, and a file that never spells
//!   `define-condition` pays a byte scan rather than a walk. What is *not*
//!   memoized, and why, is [`LazyHierarchy`]'s own documentation.
//!
//! Nothing here is called per visited node. That is deliberate: the
//! `clean/forms/*` benchmarks lint files with zero findings, so the per-file
//! cost of a rule that matches nothing is exactly what they measure.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{
    is_paren_list, list_head, symbol_in, symbol_is, unqualified,
};

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
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
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
/// `ignore-errors-wraps-non-error-signal` stops at an inner `handler-case`: a
/// `signal` that some nested form already handles is not one that escapes, and
/// reporting it would be a false positive rather than a missed subtlety.
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

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the cost is the node's depth, not the file's size.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(signal 'x)) `` has a quasiquoted ancestor
/// and an evaluated target. Being inside a hard `'` does settle it, and that is
/// already modelled by `hard` never clearing.
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
        // A span that names no node is judged by the innermost node that
        // contains it, which is the honest answer for a span the caller
        // synthesized rather than took from the tree.
        let Some(child) = view
            .children
            .iter()
            .find(|child| span_contains(child.span, target))
        else {
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

/// An atom's symbol text, past any reader prefix, lowercased and stripped of
/// its package qualifier — the spelling every comparison here is written in.
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

/// `'foo` or `(quote foo)` read as `foo`. `None` for anything else, including
/// an unquoted symbol, a quoted *list*, and a string.
#[must_use]
pub fn quoted_symbol(view: &ExpressionView) -> Option<String> {
    if is_quote_form(view) {
        return view.children.get(1).and_then(symbol_name);
    }
    if !view.reader_prefixes.contains(&ReaderPrefix::Quote) {
        return None;
    }
    symbol_name(view)
}

/// The condition type a signalling call names, when it names one literally.
///
/// `(signal 'my-error …)` and `(signal (make-condition 'my-error) …)` both name
/// `my-error`. `(signal "boom")` names no type at all — that is a
/// `simple-condition` built from a format control — and a computed datum cannot
/// be read, so both are `None` rather than a guess.
#[must_use]
pub fn signalled_condition_type(call: &ExpressionView) -> Option<String> {
    let datum = call.children.get(1)?;
    if let Some(name) = quoted_symbol(datum) {
        return Some(name);
    }
    if is_paren_list(datum)
        && list_head(datum)
            .is_some_and(|head| symbol_in(head, &["make-condition", "make-instance"]))
    {
        return datum.children.get(1).and_then(quoted_symbol);
    }
    None
}

/// One `define-condition` in the file being linted.
#[derive(Debug, Clone)]
pub struct ConditionClass {
    /// The condition's name, normalized.
    pub name: String,
    /// Its declared direct supertypes, normalized. Empty for the `()` list,
    /// which is exactly what `define-condition-empty-superclass-list` is about.
    pub supertypes: Vec<String>,
    /// Whether the form carries a `(:report …)` option.
    pub has_report: bool,
}

/// The `define-condition` forms of one file, and what they say about each
/// other.
///
/// Same-file only, on purpose. A supertype defined in another file is a name
/// this analysis knows nothing about, and inventing an answer for it would make
/// every rule that consults the hierarchy report on faith.
#[derive(Debug, Default)]
pub struct ConditionHierarchy {
    classes: Vec<ConditionClass>,
}

/// The standard hierarchy, as the edges that decide whether a type is an error.
///
/// Not the whole of CLHS figure 9-1 — only the edges a `handler-case (error …)`
/// clause's reachability depends on, plus the two `warning` subtypes, which are
/// the common "this is deliberately *not* an error" supertypes.
const STANDARD_SUPERTYPES: &[(&str, &str)] = &[
    ("simple-condition", "condition"),
    ("serious-condition", "condition"),
    ("warning", "condition"),
    ("simple-warning", "warning"),
    ("style-warning", "warning"),
    ("error", "serious-condition"),
    ("storage-condition", "serious-condition"),
    ("arithmetic-error", "error"),
    ("cell-error", "error"),
    ("control-error", "error"),
    ("file-error", "error"),
    ("package-error", "error"),
    ("parse-error", "error"),
    ("print-not-readable", "error"),
    ("program-error", "error"),
    ("simple-error", "error"),
    ("stream-error", "error"),
    ("type-error", "error"),
    ("division-by-zero", "arithmetic-error"),
    ("floating-point-inexact", "arithmetic-error"),
    ("floating-point-invalid-operation", "arithmetic-error"),
    ("floating-point-overflow", "arithmetic-error"),
    ("floating-point-underflow", "arithmetic-error"),
    ("unbound-slot", "cell-error"),
    ("unbound-variable", "cell-error"),
    ("undefined-function", "cell-error"),
    ("end-of-file", "stream-error"),
    ("reader-error", "parse-error"),
    ("simple-type-error", "type-error"),
];

fn standard_supertype(name: &str) -> Option<&'static str> {
    STANDARD_SUPERTYPES
        .iter()
        .find(|(subtype, _)| *subtype == name)
        .map(|(_, supertype)| *supertype)
}

/// Reads one `(define-condition name (supertype*) (slot*) option*)`.
///
/// `None` for anything that is not one, and for a form too short to have a
/// supertype list: a malformed definition is a different rule's subject, and
/// guessing at its shape here would make every consumer of the hierarchy
/// depend on that guess.
#[must_use]
pub fn read_define_condition(view: &ExpressionView) -> Option<ConditionClass> {
    if !is_paren_list(view)
        || !list_head(view).is_some_and(|head| symbol_is(head, "define-condition"))
    {
        return None;
    }
    let name = view.children.get(1).and_then(symbol_name)?;
    let supertypes = supertype_list(view)?;
    Some(ConditionClass {
        name,
        supertypes,
        has_report: has_option(view, ":report"),
    })
}

/// The supertype names of a `define-condition`, or `None` when child 2 is not a
/// `(…)` list at all.
#[must_use]
pub fn supertype_list(view: &ExpressionView) -> Option<Vec<String>> {
    let list = view.children.get(2)?;
    if !is_paren_list(list) {
        return None;
    }
    Some(list.children.iter().filter_map(symbol_name).collect())
}

/// Whether a `define-condition` carries `(<keyword> …)` among its options.
///
/// Options start at child 4: `(define-condition name (parents) (slots)
/// option*)`. Starting the scan earlier would read a *slot* named `:report` as
/// the option.
#[must_use]
pub fn has_option(view: &ExpressionView, keyword: &str) -> bool {
    view.children.iter().skip(4).any(|option| {
        is_paren_list(option)
            && option
                .children
                .first()
                .and_then(atom_symbol_text)
                .is_some_and(|text| text.eq_ignore_ascii_case(keyword))
    })
}

/// The spelling every `define-condition` form contains, whatever package
/// qualifier or case it is written in.
const DEFINE_CONDITION: &str = "define-condition";

/// Whether the file's bytes contain the spelling at all.
///
/// A head-matched node's head text comes from the source, so a file whose
/// source never spells `define-condition` cannot contain one, and its hierarchy
/// is empty without looking at a single node. The converse does not hold — a
/// mention inside a string or a comment answers `true` — which is the harmless
/// direction: the walk then runs and finds nothing, exactly as before.
///
/// A byte scan, not a tree walk: no allocation, no per-node work, and it reads
/// each byte once instead of visiting each node and asking about its head.
fn mentions_define_condition(source: &str) -> bool {
    let needle = DEFINE_CONDITION.as_bytes();
    source
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

impl ConditionHierarchy {
    /// Reads every `define-condition` reachable as code in one file.
    ///
    /// A whole-file walk, guarded by [`mentions_define_condition`] so that a
    /// file which defines no condition at all — the common case for the rules
    /// that consult the hierarchy, since a `(signal 'app-error …)` usually
    /// names a type defined in some *other* file — pays one byte scan instead.
    ///
    /// On a file that does define conditions the walk is real, and it is paid
    /// once per `check()` call that reaches it rather than once per file: see
    /// [`LazyHierarchy`] for why that repetition is not currently removable.
    #[must_use]
    pub fn collect(tree: &SyntaxTree) -> Self {
        if !mentions_define_condition(tree.source()) {
            return Self::default();
        }
        let mut classes = Vec::new();
        for_each_evaluated_subview(&tree.root_view(), |view| {
            if let Some(class) = read_define_condition(view) {
                classes.push(class);
            }
        });
        Self { classes }
    }

    #[must_use]
    pub fn class(&self, name: &str) -> Option<&ConditionClass> {
        self.classes.iter().find(|class| class.name == name)
    }

    /// Whether this file defines `name` at all. The precondition for saying
    /// anything about it: an undefined name may be a standard type, a type from
    /// another file, or a typo, and those are not distinguishable here.
    #[must_use]
    pub fn declares(&self, name: &str) -> bool {
        self.class(name).is_some()
    }

    /// `name` and every type it transitively inherits from, following this
    /// file's own definitions and the standard hierarchy both.
    ///
    /// Reflexive (the list starts with `name`) and cycle-safe: a definition
    /// that names itself, directly or through a loop, terminates instead of
    /// hanging. Cycles are `inspect class-cycles`'s subject, not this module's.
    #[must_use]
    pub fn ancestry(&self, name: &str) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        let mut frontier = vec![name.to_ascii_lowercase()];
        while let Some(current) = frontier.pop() {
            if seen.contains(&current) {
                continue;
            }
            if let Some(class) = self.class(&current) {
                frontier.extend(class.supertypes.iter().cloned());
            }
            if let Some(standard) = standard_supertype(&current) {
                frontier.push(standard.to_owned());
            }
            seen.push(current);
        }
        seen
    }

    /// Whether `name` reaches `error` or `serious-condition` — the two types
    /// that decide whether `handler-case (error …)` and `ignore-errors` catch
    /// it, and whether an unhandled `signal` enters the debugger.
    #[must_use]
    pub fn is_error_type(&self, name: &str) -> bool {
        self.ancestry(name)
            .iter()
            .any(|ancestor| ancestor == "error" || ancestor == "serious-condition")
    }

    /// Whether `name` or any same-file ancestor carries a `:report`.
    ///
    /// A supertype from outside this file cannot be checked, so a class whose
    /// ancestry leaves the file is *not* claimed to be report-less — see
    /// [`ConditionHierarchy::ancestry_leaves_the_file`].
    #[must_use]
    pub fn reports_anywhere(&self, name: &str) -> bool {
        self.ancestry(name)
            .iter()
            .any(|ancestor| self.class(ancestor).is_some_and(|class| class.has_report))
    }

    /// Whether `name`'s ancestry mentions a user-defined-looking type this file
    /// does not define, in which case nothing can be concluded about what
    /// `:report` it might supply.
    ///
    /// A name in the standard hierarchy is not "outside the file" in this
    /// sense: the standard types' reports are the generic ones a missing
    /// `:report` is a complaint about in the first place.
    #[must_use]
    pub fn ancestry_leaves_the_file(&self, name: &str) -> bool {
        self.ancestry(name).iter().any(|ancestor| {
            !self.declares(ancestor)
                && standard_supertype(ancestor).is_none()
                && ancestor != "condition"
        })
    }
}

/// A [`ConditionHierarchy`] that is read from the file only if someone asks.
///
/// The rules that consult the hierarchy are all head-matched on `signal`,
/// `ignore-errors` or `define-condition`, so their `check()` already runs on
/// almost no files — but "almost no" is not "none", and a file with one
/// `define-condition` and no candidate must not pay for a whole-file walk. This
/// makes the walk happen at the moment a candidate is confirmed and never
/// before.
///
/// Constructing one costs nothing, so a rule creates it unconditionally at the
/// top of `check()` and simply never calls [`LazyHierarchy::get`] on a node that
/// turns out not to matter.
///
/// # What this does *not* memoize
///
/// The `OnceCell` spans one `check()` call, because a rule builds a
/// `LazyHierarchy` inside `check()` and drops it at the end. A file with N
/// nodes that reach the hierarchy therefore builds it N times — and reaching it
/// is not the same as reporting: `signal-on-error-condition-returns-silently`
/// asks as soon as it sees a *literal* condition type, then gets `declares() ==
/// false` and reports nothing when that type is defined elsewhere. So a file
/// with N such `signal` calls does N builds and emits no findings at all. The
/// [`ConditionHierarchy::collect`] guard makes each of those N builds a byte
/// scan rather than a tree walk, which is what that shape actually costs today;
/// it does not make them one.
///
/// Removing the repetition needs a cache that outlives a single `check()`, and
/// the only such slot a rule is handed is `RuleContext::scratch_cache` — one
/// type-erased slot per file's pass, already claimed by
/// `paredit-feature-lint-repl-debug`, which panics by construction if a second
/// type is stored in it during the same pass. The alternatives inside this
/// package are a global or thread-local keyed by something (a pointer, a path)
/// that is not a reliable identity for a file's contents. Trading a byte scan
/// for a cache that can return another file's hierarchy is not a trade worth
/// making, so this is deliberately left as it is.
///
/// [`RuleContext::scratch_cache`]: paredit_core_lint_engine::engine::RuleContext::scratch_cache
#[derive(Debug)]
pub struct LazyHierarchy<'a> {
    tree: &'a SyntaxTree,
    hierarchy: std::cell::OnceCell<ConditionHierarchy>,
}

impl<'a> LazyHierarchy<'a> {
    #[must_use]
    pub const fn new(tree: &'a SyntaxTree) -> Self {
        Self {
            tree,
            hierarchy: std::cell::OnceCell::new(),
        }
    }

    /// Reads the file's `define-condition` forms, once.
    pub fn get(&self) -> &ConditionHierarchy {
        self.hierarchy
            .get_or_init(|| ConditionHierarchy::collect(self.tree))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
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

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(restart-case (foo))").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(
            evaluated_heads("(quote (restart-case (foo)))"),
            vec!["quote"]
        );
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(restart-case (foo))").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        // The backquoted list itself is data and is skipped; everything from
        // the comma down is code and is visited.
        assert_eq!(
            evaluated_heads("`(a ,(restart-case (foo)))"),
            vec!["restart-case", "foo"]
        );
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(restart-case (foo)))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(restart-case (foo))\")"), vec!["f"]);
    }

    /// The span-directed lookup must agree with the walk, since one is used by
    /// the standalone reports and the other by the registered rules.
    fn unevaluated_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        // Deliberately the *unfiltered* walk: the point is to find the node
        // even when it is data.
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "'(restart-case (foo))",
            "restart-case"
        ));
    }

    #[test]
    fn a_span_inside_a_quote_form_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "(quote (restart-case (foo)))",
            "restart-case"
        ));
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "(defun f () (restart-case (foo)))",
            "restart-case"
        ));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "`(a ,(restart-case (foo)))",
            "restart-case"
        ));
    }

    #[test]
    fn a_quoted_symbol_is_read_through_both_spellings() {
        let parsed = tree("(signal 'my-error)");
        let call = &parsed.root_view().children[0];
        assert_eq!(signalled_condition_type(call).as_deref(), Some("my-error"));

        let parsed = tree("(signal (make-condition 'my-error))");
        let call = &parsed.root_view().children[0];
        assert_eq!(signalled_condition_type(call).as_deref(), Some("my-error"));
    }

    #[test]
    fn a_format_control_string_names_no_condition_type() {
        let parsed = tree("(signal \"boom ~A\" x)");
        let call = &parsed.root_view().children[0];
        assert_eq!(signalled_condition_type(call), None);
    }

    #[test]
    fn a_computed_datum_names_no_condition_type() {
        let parsed = tree("(signal (compute-condition))");
        let call = &parsed.root_view().children[0];
        assert_eq!(signalled_condition_type(call), None);
    }

    #[test]
    fn a_package_qualified_type_normalizes_to_its_name() {
        let parsed = tree("(signal 'app::my-error)");
        let call = &parsed.root_view().children[0];
        assert_eq!(signalled_condition_type(call).as_deref(), Some("my-error"));
    }

    #[test]
    fn the_hierarchy_reads_a_definitions_name_supertypes_and_report() {
        let parsed = tree(
            "(define-condition my-error (error)\n  ((code :initarg :code))\n  (:report \"boom\"))",
        );
        let hierarchy = ConditionHierarchy::collect(&parsed);
        let class = hierarchy.class("my-error").expect("the class");
        assert_eq!(class.supertypes, vec!["error".to_owned()]);
        assert!(class.has_report);
    }

    #[test]
    fn an_empty_supertype_list_is_read_as_empty_not_missing() {
        let parsed = tree("(define-condition my-note () ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(
            hierarchy
                .class("my-note")
                .expect("the class")
                .supertypes
                .is_empty()
        );
    }

    #[test]
    fn a_slot_named_report_is_not_mistaken_for_the_option() {
        let parsed = tree("(define-condition my-error (error) ((:report :initform nil)))");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(!hierarchy.class("my-error").expect("the class").has_report);
    }

    #[test]
    fn inheritance_follows_the_files_own_definitions() {
        let parsed = tree("(define-condition base (error) ())\n(define-condition leaf (base) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(hierarchy.is_error_type("leaf"));
        assert!(hierarchy.is_error_type("base"));
    }

    #[test]
    fn inheritance_follows_the_standard_hierarchy_too() {
        let parsed = tree("(define-condition leaf (file-error) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(hierarchy.is_error_type("leaf"));
    }

    #[test]
    fn a_warning_subtype_is_not_an_error_type() {
        let parsed = tree("(define-condition my-note (warning) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(!hierarchy.is_error_type("my-note"));
    }

    #[test]
    fn an_empty_supertype_list_does_not_reach_error() {
        let parsed = tree("(define-condition my-note () ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(!hierarchy.is_error_type("my-note"));
    }

    #[test]
    fn a_report_on_a_supertype_counts_for_its_subtype() {
        let parsed = tree(
            "(define-condition base (error) () (:report \"boom\"))\n\
             (define-condition leaf (base) ())",
        );
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(hierarchy.reports_anywhere("leaf"));
    }

    #[test]
    fn a_cyclic_definition_terminates_instead_of_hanging() {
        let parsed = tree("(define-condition a (b) ())\n(define-condition b (a) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        let mut ancestry = hierarchy.ancestry("a");
        ancestry.sort_unstable();
        assert_eq!(ancestry, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn a_definition_inside_quoted_data_is_not_part_of_the_hierarchy() {
        let parsed = tree("'(define-condition my-error (error) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(!hierarchy.declares("my-error"));
    }

    #[test]
    fn an_ancestry_that_leaves_the_file_is_reported_as_such() {
        let parsed = tree("(define-condition leaf (other-packages-error) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(hierarchy.ancestry_leaves_the_file("leaf"));
    }

    #[test]
    fn an_ancestry_of_standard_types_only_does_not_leave_the_file() {
        let parsed = tree("(define-condition leaf (error) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(!hierarchy.ancestry_leaves_the_file("leaf"));
    }

    #[test]
    fn the_walk_guard_answers_yes_only_for_the_spelling() {
        assert!(mentions_define_condition("(define-condition e (error) ())"));
        assert!(mentions_define_condition("(DEFINE-CONDITION e (error) ())"));
        assert!(mentions_define_condition(
            "(cl:define-condition e (error) ())"
        ));
        assert!(!mentions_define_condition("(signal 'e)"));
        assert!(!mentions_define_condition("(define-conditio"));
        assert!(!mentions_define_condition(""));
    }

    /// The guard is an optimization, so the only thing that may change is the
    /// cost: a file with no definition read as empty either way.
    #[test]
    fn a_file_that_defines_no_condition_has_an_empty_hierarchy() {
        let parsed = tree("(defun f () (signal 'defined-elsewhere-error))");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(!hierarchy.declares("defined-elsewhere-error"));
        assert!(!hierarchy.is_error_type("defined-elsewhere-error"));
    }

    /// The spelling in a string clears the guard and the walk then finds
    /// nothing, which is the same answer the walk gave before the guard existed.
    #[test]
    fn a_mention_in_a_string_is_not_read_as_a_definition() {
        let parsed = tree("(format t \"define-condition\")");
        assert!(mentions_define_condition(parsed.source()));
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(!hierarchy.declares("define-condition"));
    }

    #[test]
    fn a_lazy_hierarchy_reads_the_file_once_and_reuses_it() {
        let parsed = tree("(define-condition my-error (error) ())");
        let lazy = LazyHierarchy::new(&parsed);
        let first: *const ConditionHierarchy = lazy.get();
        let second: *const ConditionHierarchy = lazy.get();
        assert!(std::ptr::eq(first, second));
        assert!(lazy.get().declares("my-error"));
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
}
