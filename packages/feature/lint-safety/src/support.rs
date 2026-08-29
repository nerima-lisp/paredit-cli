//! What the six injection-shaped rules in this package share: whether a matched
//! node is *code*, whether it sits inside a function definition, and how a
//! string was assembled.
//!
//! Three things none of these rules can do without, and none of which the
//! engine or `core/syntax` provides:
//!
//! - **Evaluation context.** `'(open "/tmp/x" :if-exists :supersede)` is a list
//!   of symbols, not an `open` call. The lint engine's dispatch walks into
//!   quoted data like any other subtree and [`RuleContext`] carries no parent
//!   pointer, so a head-matched node cannot tell on its own whether it is code.
//!   [`context_at`] answers that.
//!
//! - **Whether an enclosing form is a function definition.**
//!   `read-eval-star-rebound-to-t` needs it, and for one reason only: to stay
//!   off the exact forms `read-without-read-eval-guard` already reports. That
//!   rule anchors on `defun`/`defmethod`/`lambda`, so "is a `defun` above me?"
//!   is precisely the question that makes the two disjoint. It is answered by
//!   the *same* descent that answers the quote question, so it costs nothing
//!   extra.
//!
//! - **How a string was built.** Four of the six rules care that a value was
//!   spliced into a string that is then interpreted by something — a format
//!   control, a filename, a SQL statement. [`StringBuild`] is the one place
//!   `(concatenate 'string …)`, `(format nil …)` and `(uiop:strcat …)` are read
//!   as the same shape, so the four rules cannot drift on what counts as a
//!   literal part.
//!
//! # Quote semantics
//!
//! `QuoteState` and the descent in [`context_at`] are copied from
//! `paredit-feature-lint-testing`'s `support.rs`, which copied them from
//! `paredit-feature-lint-condition-system`'s, deliberately as a copy rather
//! than as a dependency: a lint feature package depending on another lint
//! feature package would be a new feature→feature edge (and a
//! `tests/cli/feature_dependency_contract.rs` entry) for a hundred lines of
//! traversal.
//!
//! The two counters are not one depth number. A comma inside `'(…)` is a comma
//! character in a literal list, so `hard` never clears; a comma inside `` `(…) ``
//! escapes back to code, so `quasi` counts up and down. And the verdict is read
//! *at* the target: a node one level inside a `'` is still data, which a
//! node-local `reader_prefixes` check would miss.
//!
//! # Cost
//!
//! Nothing here runs per visited node. Every rule in this package declares
//! [`HeadFilter::Heads`], and every call into this module happens *after* a head
//! has matched and a finding is otherwise established — which, in the
//! `clean/forms/*` benchmarks that lint files with zero findings, is never.
//!
//! [`context_at`] never calls `tree.root_view()`: that materializes a `Vec` of
//! children and a `String` per atom for *every node in the file*, so asking it
//! about one node costs the whole document and makes F findings cost F×N. It
//! binary-searches the top level for the one enclosing root child instead and
//! materializes only that.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext
//! [`HeadFilter::Heads`]: paredit_core_lint_engine::model::HeadFilter::Heads

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, symbol_is};

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
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

/// The definition forms `read-without-read-eval-guard` anchors on, and
/// therefore the exact set `read-eval-star-rebound-to-t` must defer inside.
const DEFINITION_HEADS: [&str; 3] = ["defun", "defmethod", "lambda"];

fn is_definition_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_in(head, &DEFINITION_HEADS))
}

/// Whether `outer` covers every byte of `inner`. Equal spans contain each
/// other, so a caller that means "strictly inside" compares the spans too.
#[must_use]
pub const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
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
/// prefixes, plus a `String` per atom — for *every node in the file*, so asking
/// it about one node costs the whole document. This is called once per
/// candidate finding, so `root_view` would make a file of F findings cost F×N.
///
/// Selecting the one root child instead costs a binary search over the top
/// level — each step a node-id lookup and a span read, neither of which
/// allocates — plus that one form's own subtree.
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

/// What the descent from the top level to a node found out about its
/// surroundings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceContext {
    /// The node is quoted data rather than evaluated code.
    pub unevaluated: bool,
    /// Some *strict* ancestor is a `defun`, `defmethod`, or `lambda`.
    pub inside_definition: bool,
}

impl SourceContext {
    /// The answer for a span that belongs to no top-level form — one a caller
    /// synthesized rather than took from the tree. Nothing quotes it and
    /// nothing encloses it.
    const UNENCLOSED: Self = Self {
        unevaluated: false,
        inside_definition: false,
    };
}

/// Everything the rules here need to know about what encloses the node at
/// `target`, in one descent.
///
/// Descends to `target` through the one child at each level whose span contains
/// it, so the cost is the enclosing top-level form's size, and never the
/// file's.
///
/// The quote verdict is read *at* the target and nowhere shallower. An ancestor
/// being data does not settle it: `` `(a ,(open p)) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it, and
/// that is already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all.
#[must_use]
pub fn context_at(tree: &SyntaxTree, target: ByteSpan) -> SourceContext {
    let Some(top_level) = root_child_containing(tree, target) else {
        return SourceContext::UNENCLOSED;
    };
    let mut view: &ExpressionView = &top_level;
    // The root carries no reader prefix and is not a `(quote …)` form, so the
    // state entering the top-level form is whatever that form's own prefixes
    // say.
    let mut state = QuoteState::EVALUATED.after_prefixes(view);
    let mut inside_definition = false;

    while view.span != target {
        // Read at strict ancestors only: a `lambda` is not inside itself.
        if is_definition_form(view) {
            inside_definition = true;
        }
        let quoting = is_quote_form(view);
        // A span that names no node is judged by the innermost node that
        // contains it, which is the honest answer for a span the caller
        // synthesized rather than took from the tree.
        let Some(child) = child_containing(view, target) else {
            return SourceContext {
                unevaluated: state.is_data(),
                inside_definition,
            };
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
    }

    SourceContext {
        unevaluated: state.is_data(),
        inside_definition,
    }
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Every rule here calls this at most once per finding, *after* its head has
/// already matched and its own analysis has found something — never per visited
/// node.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    context_at(tree, target).unevaluated
}

// ---------------------------------------------------------------------------
// Literals
// ---------------------------------------------------------------------------

/// The contents of a string literal atom, without its delimiters.
///
/// Escapes are left exactly as written. Every consumer here looks for a
/// directory prefix, a SQL keyword, or a `~` directive, and `\"` cannot spell
/// any of those, so decoding would allocate for no change in the answer. What
/// it *does* buy is the guarantee that a form spelled inside a string is never
/// read as a form: the reader keeps `"(chmod \"x\" #o777)"` as this one atom,
/// with no children, so no walk can reach into it.
#[must_use]
pub fn string_literal(view: &ExpressionView) -> Option<&str> {
    let text = atom_text(view)?;
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    Some(inner)
}

/// Whether `view` is a value the source shows in full: a string, a number, a
/// character, a keyword, `t`/`nil`, or anything quoted.
///
/// This is the "not attacker-controlled" side of every rule here. It is
/// deliberately generous — a value it cannot prove literal is treated as
/// non-literal, which is the direction that produces a finding — so the
/// generosity costs recall, not precision.
#[must_use]
pub fn is_literal(view: &ExpressionView) -> bool {
    if view.reader_prefixes.contains(&ReaderPrefix::Quote) {
        return true;
    }
    if is_quote_form(view) {
        return true;
    }
    let Some(text) = atom_text(view) else {
        return false;
    };
    let mut characters = text.chars();
    match characters.next() {
        None => false,
        Some('"' | ':' | '#') => true,
        Some(first) if first.is_ascii_digit() => true,
        Some('-' | '+' | '.') => characters.next().is_some_and(|c| c.is_ascii_digit()),
        _ => text.eq_ignore_ascii_case("t") || text.eq_ignore_ascii_case("nil"),
    }
}

/// Whether `name` is spelled the way Common Lisp spells a program-owned global:
/// `*earmuffs*` for a special, `+plus+` for a constant.
///
/// Used to keep `format-tilde-slash-unvalidated-function-designator` off
/// `(format t *usage-banner*)`. A value the program named in its own source is
/// not the untrusted control string that rule is about.
#[must_use]
pub fn looks_like_program_global(name: &str) -> bool {
    let stripped = paredit_core_syntax::view_query::unqualified(name);
    stripped.len() > 2
        && ((stripped.starts_with('*') && stripped.ends_with('*'))
            || (stripped.starts_with('+') && stripped.ends_with('+')))
}

// ---------------------------------------------------------------------------
// Assembled strings
// ---------------------------------------------------------------------------

/// The three spellings of "this string was assembled from parts", read as one
/// shape.
///
/// `format` is kept apart from the others because its parts are not
/// interchangeable: the control string is the *program's* text and the
/// arguments are the values spliced into it, whereas `concatenate` treats every
/// argument alike. A rule that wants "is there a literal base directory" has to
/// ask that of the control string only.
#[derive(Debug, Clone, Copy)]
pub enum StringBuild<'v> {
    /// `(concatenate 'string a b …)` — the parts follow the result type.
    Concatenate(&'v [ExpressionView]),
    /// `(format nil control args…)`.
    Format {
        control: &'v ExpressionView,
        arguments: &'v [ExpressionView],
    },
    /// `(uiop:strcat a b …)` or `(string+ a b …)`.
    Strcat(&'v [ExpressionView]),
}

/// Reads `view` as an assembled string, or `None` if it is not one.
///
/// `(format t …)` prints and returns `nil`; it assembles nothing, so it is not
/// one of these. That check is why a `(run-program (format t …))` is not a
/// command line either — see `subprocess_string_building`, which makes the same
/// distinction for its own sink.
#[must_use]
pub fn string_build(view: &ExpressionView) -> Option<StringBuild<'_>> {
    let head = list_head(view)?;
    if symbol_is(head, "concatenate") {
        // `(concatenate 'string …)`; the result type is children[1].
        return Some(StringBuild::Concatenate(view.children.get(2..)?));
    }
    if symbol_in(head, &["strcat", "string+"]) {
        return Some(StringBuild::Strcat(view.children.get(1..)?));
    }
    if symbol_is(head, "format") {
        let destination = view.children.get(1)?;
        if !atom_text(destination).is_some_and(|text| text.eq_ignore_ascii_case("nil")) {
            return None;
        }
        return Some(StringBuild::Format {
            control: view.children.get(2)?,
            arguments: view.children.get(3..)?,
        });
    }
    None
}

impl StringBuild<'_> {
    /// The operator that did the assembling, for the finding's message.
    #[must_use]
    pub const fn builder(&self) -> &'static str {
        match self {
            Self::Concatenate(_) => "concatenate",
            Self::Format { .. } => "format nil",
            Self::Strcat(_) => "strcat",
        }
    }

    /// The literal fragments the program itself wrote, each kept separate.
    ///
    /// Separate rather than joined because adjacency is a claim: `(concatenate
    /// 'string dir "/" name)` has no literal directory in it, and joining its
    /// fragments would invent one.
    #[must_use]
    pub fn literal_fragments(&self) -> Vec<&str> {
        match self {
            Self::Concatenate(parts) | Self::Strcat(parts) => {
                parts.iter().filter_map(string_literal).collect()
            }
            Self::Format { control, .. } => string_literal(control).into_iter().collect(),
        }
    }

    /// The values spliced into the assembled string — everything but the
    /// program's own literal text.
    #[must_use]
    pub fn interpolated(&self) -> Vec<&ExpressionView> {
        match self {
            Self::Concatenate(parts) | Self::Strcat(parts) => {
                parts.iter().filter(|part| !is_literal(part)).collect()
            }
            Self::Format { control, arguments } => {
                let mut spliced: Vec<&ExpressionView> = arguments
                    .iter()
                    .filter(|argument| !is_literal(argument))
                    .collect();
                // A non-literal control is itself a spliced value: whatever it
                // holds becomes the directives.
                if !is_literal(control) {
                    spliced.push(control);
                }
                spliced
            }
        }
    }
}

/// `fragment` with every `~…X` format directive removed, so what is left is the
/// text the *program* wrote rather than the holes it left for values.
///
/// The parameter prefix a directive may carry (`~10,'0d`, `~v@a`, `~:{`) is
/// consumed along with it. A `~` at the very end of the fragment is left as
/// itself; it is not a directive.
fn without_directives(fragment: &str) -> String {
    let mut kept = String::with_capacity(fragment.len());
    let mut characters = fragment.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '~' {
            kept.push(character);
            continue;
        }
        // Prefix parameters, then the one dispatch character.
        while characters.peek().is_some_and(|c| {
            c.is_ascii_digit() || matches!(c, ',' | ':' | '@' | '\'' | '#' | 'v' | 'V' | '-' | '+')
        }) {
            characters.next();
        }
        if characters.next().is_none() {
            kept.push('~');
        }
    }
    kept
}

/// Whether `fragment` contains a directory the program itself named — a `/`
/// with a non-empty, non-`/` segment of *literal* text before it.
///
/// `"/var/data/"` and `"data/"` have one. `"~a/~a"` and `"/"` do not: there the
/// directory came from a value, and only the separator came from the source.
/// That distinction is what keeps `path-traversal-via-concatenated-filename` on
/// the "base directory plus a non-literal" shape it claims and off
/// `(format nil "~a/~a" dir name)`, which is how a great deal of correct code
/// joins two paths.
#[must_use]
pub fn names_a_base_directory(fragment: &str) -> bool {
    let literal = without_directives(fragment);
    literal
        .match_indices('/')
        .any(|(index, _)| literal[..index].chars().any(|c| c != '/'))
}

/// Whether `haystack` contains `needle` as a whole word, comparing
/// case-insensitively.
///
/// `"selected"` does not contain the word `select`, which is the entire reason
/// `sql-query-string-built-via-format` does not fire on `(format nil "~a rows
/// selected" n)`.
#[must_use]
pub fn contains_word(haystack: &str, needle: &str) -> bool {
    let lowered = haystack.to_ascii_lowercase();
    let is_word_byte = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    lowered.match_indices(needle).any(|(index, matched)| {
        let before = index
            .checked_sub(1)
            .and_then(|previous| lowered.as_bytes().get(previous).copied());
        let after = lowered.as_bytes().get(index + matched.len()).copied();
        !before.is_some_and(is_word_byte) && !after.is_some_and(is_word_byte)
    })
}

/// What each rule's own tests drive their `examine` with.
///
/// The point is fidelity to dispatch. A test that parses a snippet and calls
/// `examine` on `root_child(0)` only ever exercises a *top-level* match, which
/// silently skips every quote-context and nesting case — the shapes these rules
/// exist to get right. [`testing::findings_for_heads`] instead visits every node
/// of every top-level form and calls `examine` on exactly those whose head the
/// rule declared, which is what `HeadFilter::Heads` does.
#[cfg(test)]
pub mod testing {
    use super::{Path, SyntaxTree};
    use paredit_core_lint_engine::engine::RuleContext;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::ExpressionView;
    use paredit_core_syntax::view_query::{for_each_subview, list_head, symbol_in};
    use std::path::Path as FsPath;

    /// Every finding `examine` produces over the nodes the engine would hand it.
    ///
    /// `heads` is the rule's own `HEADS`, spelled as plain strings; matching
    /// goes through `symbol_in`, so a package-qualified spelling in the snippet
    /// is matched the same way the engine's head index matches it.
    pub fn findings_for_heads<T>(
        input: &str,
        heads: &[&str],
        mut examine: impl FnMut(&ExpressionView, &RuleContext<'_>) -> Vec<T>,
    ) -> Vec<T> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let context = RuleContext::new(FsPath::new("t.lisp"), Dialect::CommonLisp, &tree, input);
        let mut found = Vec::new();
        for index in 0..tree.root_children().len() {
            let Ok(selection) = tree.select_path(&Path::root_child(index)) else {
                continue;
            };
            let form = selection.view();
            for_each_subview(&form, |node| {
                if list_head(node).is_some_and(|head| symbol_in(head, heads)) {
                    found.extend(examine(node, &context));
                }
            });
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::view_query::for_each_subview;

    fn tree_of(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    fn root_form(tree: &SyntaxTree) -> ExpressionView {
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    /// The context read at the first node whose head is `head`, which is the
    /// node the engine's `Heads` dispatch would hand a rule.
    fn context_at_first_head(source: &str, head: &str) -> SourceContext {
        let tree = tree_of(source);
        let root = root_form(&tree);
        let mut target = None;
        for_each_subview(&root, |view| {
            if target.is_none() && list_head(view).is_some_and(|found| symbol_is(found, head)) {
                target = Some(view.span);
            }
        });
        context_at(&tree, target.expect("a node with that head"))
    }

    // --- the five quote shapes ------------------------------------------

    #[test]
    fn plain_code_is_evaluated() {
        assert!(
            !context_at_first_head(r#"(open "/tmp/x" :if-exists :supersede)"#, "open").unevaluated
        );
    }

    #[test]
    fn a_hard_quoted_form_is_data() {
        assert!(
            context_at_first_head(r#"'(open "/tmp/x" :if-exists :supersede)"#, "open").unevaluated
        );
    }

    #[test]
    fn a_node_one_level_inside_a_quote_is_still_data() {
        // The node itself carries no reader prefix, so a node-local
        // `reader_prefixes` check would call this code.
        assert!(context_at_first_head(r#"'(progn (open "/tmp/x"))"#, "open").unevaluated);
    }

    #[test]
    fn a_long_hand_quote_form_makes_its_body_data() {
        assert!(context_at_first_head(r#"(quote (open "/tmp/x"))"#, "open").unevaluated);
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(context_at_first_head(r#"`(open "/tmp/x")"#, "open").unevaluated);
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert!(!context_at_first_head(r#"`(a ,(open "/tmp/x"))"#, "open").unevaluated);
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        // A single depth counter would clear here and call this code.
        assert!(context_at_first_head(r#"'(a ,(open "/tmp/x"))"#, "open").unevaluated);
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        let tree = tree_of(r#"(f "(open \"/tmp/x\")")"#);
        let root = root_form(&tree);
        let mut heads = Vec::new();
        for_each_subview(&root, |view| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
        });
        assert_eq!(heads, vec!["f"]);
    }

    // --- enclosing definition -------------------------------------------

    #[test]
    fn a_definition_ancestor_is_seen() {
        assert!(
            context_at_first_head("(defun f (s) (let ((*read-eval* t)) (read s)))", "let")
                .inside_definition
        );
        assert!(
            context_at_first_head("(defmethod f ((s stream)) (let ((x 1)) x))", "let")
                .inside_definition
        );
        assert!(context_at_first_head("(lambda (s) (let ((x 1)) x))", "let").inside_definition);
    }

    #[test]
    fn a_top_level_form_has_no_definition_ancestor() {
        assert!(
            !context_at_first_head("(let ((*read-eval* t)) (read s))", "let").inside_definition
        );
    }

    #[test]
    fn a_definition_is_not_its_own_ancestor() {
        assert!(!context_at_first_head("(defun f (s) (read s))", "defun").inside_definition);
    }

    // --- cost ------------------------------------------------------------

    /// The regression this module's `root_child_containing` exists for: asking
    /// about one node must not materialize the file. A `root_view()`-based
    /// implementation makes this quadratic, and at 4000 forms it does not
    /// finish in a test's patience.
    #[test]
    fn resolving_a_span_does_not_scan_the_top_level() {
        let source = "(open \"/tmp/x\" :direction :output)\n".repeat(4000);
        let tree = tree_of(&source);
        let spans: Vec<ByteSpan> = (0..4000)
            .map(|index| {
                tree.select_path(&Path::root_child(index))
                    .expect("root child")
                    .span()
            })
            .collect();
        assert_eq!(spans.len(), 4000);
        for span in spans {
            assert!(!is_unevaluated_at(&tree, span));
        }
    }

    // --- literals ---------------------------------------------------------

    fn literal(source: &str) -> bool {
        is_literal(&root_form(&tree_of(source)))
    }

    #[test]
    fn literals_are_recognised() {
        assert!(literal(r#""text""#));
        assert!(literal("42"));
        assert!(literal("-3"));
        assert!(literal(":keyword"));
        assert!(literal("#\\a"));
        assert!(literal("nil"));
        assert!(literal("T"));
        assert!(literal("'(a b)"));
        assert!(literal("(quote x)"));
    }

    #[test]
    fn values_are_not_literals() {
        assert!(!literal("name"));
        assert!(!literal("(user-input)"));
        assert!(!literal("*root*"));
    }

    // --- assembled strings ------------------------------------------------

    fn build_of(source: &str) -> Option<(&'static str, Vec<String>, usize)> {
        let tree = tree_of(source);
        let root = root_form(&tree);
        let build = string_build(&root)?;
        Some((
            build.builder(),
            build
                .literal_fragments()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            build.interpolated().len(),
        ))
    }

    #[test]
    fn reads_a_concatenate() {
        let (builder, fragments, spliced) =
            build_of(r#"(concatenate 'string "data/" name)"#).expect("a build");
        assert_eq!(builder, "concatenate");
        assert_eq!(fragments, vec!["data/"]);
        assert_eq!(spliced, 1);
    }

    #[test]
    fn reads_a_format_nil() {
        let (builder, fragments, spliced) =
            build_of(r#"(format nil "/var/~a" name)"#).expect("a build");
        assert_eq!(builder, "format nil");
        assert_eq!(fragments, vec!["/var/~a"]);
        assert_eq!(spliced, 1);
    }

    #[test]
    fn a_printing_format_assembles_nothing() {
        assert!(build_of(r#"(format t "/var/~a" name)"#).is_none());
    }

    #[test]
    fn an_all_literal_build_splices_nothing() {
        let (_, _, spliced) = build_of(r#"(concatenate 'string "a" "b")"#).expect("a build");
        assert_eq!(spliced, 0);
    }

    #[test]
    fn a_package_qualified_strcat_is_read() {
        let (builder, _, spliced) = build_of(r#"(uiop:strcat "a" name)"#).expect("a build");
        assert_eq!(builder, "strcat");
        assert_eq!(spliced, 1);
    }

    // --- fragments and words ----------------------------------------------

    #[test]
    fn a_literal_base_directory_is_recognised() {
        assert!(names_a_base_directory("data/"));
        assert!(names_a_base_directory("/var/data/"));
        assert!(names_a_base_directory("/var/data/~a"));
    }

    #[test]
    fn a_separator_alone_names_no_directory() {
        assert!(!names_a_base_directory("/"));
        assert!(!names_a_base_directory("~a/~a"));
        assert!(!names_a_base_directory("/~a"));
        assert!(!names_a_base_directory("plain-name"));
        // A directive with prefix parameters is still a hole, not a segment.
        assert!(!names_a_base_directory("~10,'0d/~a"));
        assert!(!names_a_base_directory("~v@a/~s"));
    }

    #[test]
    fn directives_are_stripped_but_ordinary_text_survives() {
        assert_eq!(without_directives("/var/lib/app/~a"), "/var/lib/app/");
        assert_eq!(without_directives("~a/~a"), "/");
        assert_eq!(without_directives("~10,'0d-~a.log"), "-.log");
        // A trailing `~` is not a directive.
        assert_eq!(without_directives("dir/~"), "dir/~");
    }

    #[test]
    fn word_matching_respects_boundaries() {
        assert!(contains_word("SELECT * FROM t", "select"));
        assert!(contains_word("select * from t", "from"));
        assert!(!contains_word("~a rows selected", "select"));
        assert!(!contains_word("preselect x", "select"));
        assert!(contains_word("(select)", "select"));
    }
}
