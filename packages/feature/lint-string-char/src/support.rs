//! What the `format` control-string rules share: which parts of a file are
//! *code*, which argument of a `format`-family call is the control string, and
//! how to read the `~` directives out of one.
//!
//! # Why a control-string scanner lives here at all
//!
//! A control string is a program in a second language carried inside a string
//! literal, which every tree-shaped analysis treats as one opaque atom. Three
//! rules in this package need to read that second program, and each of them
//! needs the *same* answer to the same question — where does one directive end
//! and the next begin — so the scan is written once.
//!
//! The scan is deliberately not a `contains("~%~&")`-style substring search.
//! `~~` is the literal-tilde directive, so the four characters `~~%` are the
//! directive `~~` followed by the ordinary character `%`, and a substring
//! search reads that as a `~%`. Every rule here therefore asks this module for
//! *directives*, never for characters.
//!
//! # Cost
//!
//! Every rule here is `HeadFilter::Heads`, so `check()` runs only on
//! `format`-family calls; the `clean/forms/*` benchmarks lint a file with no
//! findings, and what they measure is the per-node cost a rule pays for
//! matching nothing. Nothing in this module allocates before a call has
//! survived the cheap disqualifiers in [`literal_control_string`]: the head
//! lookup is a handful of case-insensitive comparisons on a borrowed `&str`,
//! the control-string child is a borrowed slice of an atom the view already
//! holds, and [`directives`] is an iterator over that slice with no
//! intermediate collection. [`resolve_escapes`] borrows unless the literal
//! actually contains a backslash.
//!
//! [`is_unevaluated_at`] is the one exception, and it is called only once a
//! rule already has a finding in hand — see its own documentation.

use std::borrow::Cow;

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_is};

// -- evaluation context ------------------------------------------------------

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down. A single depth counter reads `'(a ,(format …))` as code, which it
/// is not.
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
/// Quoted subtrees are still *descended* — `` `(a ,(format …)) `` has code
/// inside data — but their data nodes are never visited. Iterative rather than
/// recursive, so a deeply nested file cannot overflow the stack.
///
/// This is the walk the standalone `inspect <rule>` reports use. The registered
/// rules do not use it: the dispatcher hands them head-matched nodes
/// *including* the ones inside `'(…)`, and they call [`is_unevaluated_at`]
/// instead.
pub fn for_each_evaluated_subview(root: &ExpressionView, mut visit: impl FnMut(&ExpressionView)) {
    let mut stack = vec![(root, QuoteState::EVALUATED)];
    while let Some((view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() {
            visit(view);
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
/// data does not settle it: `` `(a ,(format t "~Q")) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it,
/// and that is already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all.
///
/// # Cost
///
/// `SyntaxTree::root_view` deep-materializes the document and is not cached, so
/// this is expensive and every caller here calls it *only after* a violation is
/// confirmed — that is, on a control string that is already known to be
/// malformed. A file with no findings never reaches it, which is what the
/// `clean/forms/*` benchmarks measure.
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

// -- reaching the control string ---------------------------------------------

/// The `format`-family operators whose control string is a fixed argument, and
/// which child index holds it.
///
/// `format` and `cerror` take something before the control string — a
/// destination and a continue-format-control respectively — and the rest take
/// the control string immediately. Getting that offset wrong would make every
/// rule here read an ordinary argument as a control string.
///
/// The same five heads, at the same five offsets, that
/// `paredit_feature_lisp_analysis::format_directive_report` anchors on. They
/// are repeated rather than imported because this package does not depend on
/// that one, and a feature→feature edge needs an allowlist entry to add.
const FORMAT_HEADS: [(&str, usize); 5] = [
    ("format", 2),
    ("error", 1),
    ("warn", 1),
    ("cerror", 2),
    ("format-to-string", 1),
];

/// Which child of a `format`-family call is its control string, by head name.
///
/// Case-insensitive and package-qualifier-insensitive, so `CL:FORMAT` answers
/// the same as `format`. `None` for every other operator.
#[must_use]
pub fn control_string_index(head: &str) -> Option<usize> {
    FORMAT_HEADS
        .iter()
        .find(|(name, _)| symbol_is(head, name))
        .map(|(_, index)| *index)
}

/// The control string of a `format`-family call, as it is written in the
/// source, without its surrounding quotes.
///
/// `None` — meaning "there is nothing here this package can analyze" — for
/// every one of:
///
///   - a head that is not in [`FORMAT_HEADS`];
///   - a call too short to have a control-string slot at all;
///   - a *computed* control string (`(format t (banner) x)`), because a control
///     string that does not exist in the source cannot be read;
///   - a control string that contains no `~` at all, and therefore no
///     directive for any rule here to have an opinion about.
///
/// The last of those is the cheap disqualifier that keeps this whole package
/// off the hot path: it is a byte scan of one already-borrowed slice, and it
/// rejects the great majority of real control strings before anything
/// allocates.
///
/// The returned slice is the *source* text, so `\"` and `\\` are still
/// escaped — see [`resolve_escapes`] for the string `format` actually sees.
#[must_use]
pub fn literal_control_string(view: &ExpressionView) -> Option<&str> {
    let head = list_head(view)?;
    let index = control_string_index(head)?;
    let control = view.children.get(index)?;
    // No separate reader-prefix guard. `atom_text` returns an atom's source
    // *including* its reader prefixes, so a `'"~a"` in the control slot reads
    // as `'"~a"`, whose first character is not a quote, and the
    // `strip_prefix('"')` below already rejects it. An explicit
    // `reader_prefixes.is_empty()` check here was verified to be unreachable —
    // no test could distinguish its presence — so it is not carried on a path
    // this hot. `a_reader_prefix_stays_in_the_atoms_text` pins the assumption.
    let text = atom_text(control)?;
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    inner.contains('~').then_some(inner)
}

/// The control-string child's own span, for reporting.
///
/// A finding is about the string, not about the whole call, and pointing at the
/// string is what lets a reader see the directive that was complained about.
#[must_use]
pub fn control_string_span(view: &ExpressionView) -> Option<ByteSpan> {
    let head = list_head(view)?;
    let index = control_string_index(head)?;
    view.children.get(index).map(|control| control.span)
}

/// The string `format` actually receives, with `\"` and `\\` resolved.
///
/// Borrows unless the literal contains a backslash, which the overwhelming
/// majority do not. Resolving matters: `"~%\~&"` is the four-character control
/// string `~%~&` at run time, and a rule reading the unresolved source would
/// see a `\` between the two directives and conclude they are not adjacent.
#[must_use]
pub fn resolve_escapes(raw: &str) -> Cow<'_, str> {
    if !raw.contains('\\') {
        return Cow::Borrowed(raw);
    }
    let mut resolved = String::with_capacity(raw.len());
    let mut escaped = false;
    for character in raw.chars() {
        if escaped {
            resolved.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            resolved.push(character);
        }
    }
    Cow::Owned(resolved)
}

// -- reading the directives --------------------------------------------------

/// One `~` directive of a control string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Directive {
    /// The dispatch character, exactly as written. CLHS 22.3 says its case is
    /// ignored, so every comparison against it must fold case.
    pub character: char,
    /// Whether anything at all sat between the tilde and the dispatch
    /// character: a prefix parameter, a `:` or `@` modifier, or both.
    ///
    /// The rules that reason about a directive being a no-op need this.
    /// `~&` is a no-op after `~%`; `~2&` is not, because CLHS 22.3.1.3 makes
    /// `~n&` emit `n-1` newlines beyond the fresh-line.
    pub decorated: bool,
    /// Byte offset of the `~`, within the *resolved* control string.
    pub start: usize,
    /// Byte offset just past the directive, within the resolved control string.
    ///
    /// For `~/pkg:fn/` this is past the closing slash, not past the `/` that
    /// opened it.
    pub end: usize,
}

impl Directive {
    /// The dispatch character folded to lower case, which is the spelling every
    /// table in this package is written in.
    #[must_use]
    pub const fn folded(self) -> char {
        self.character.to_ascii_lowercase()
    }
}

/// Every `~` directive in a control string, in order.
///
/// The prefix scan is the whole point. A directive is `~`, then optional prefix
/// parameters, then optional `:`/`@` modifiers, then one dispatch character
/// (CLHS 22.3), so the character that decides what a directive *is* is the
/// first one after all of that — not the one after the tilde. Reading `~10,'0D`
/// as a `~1` would invent three directives that are not there.
///
/// Three details of that grammar that a naive scanner gets wrong, all taken
/// from CLHS 22.3's own description and examples:
///
///   - **Parameters are signed.** The spec's own example is `"~3,-4:@s"`, with
///     parameters `3` and `-4`, and `"~,+4S"`, whose first parameter is omitted.
///     A scanner that stops at `+` reports a `~+` directive that does not exist.
///   - **`'` quotes the next character.** In `~5,'*d` the `*` is a padding
///     character, not the go-to directive, and in `~'{d` the `{` does not open
///     an iteration.
///   - **`v`, `V` and `#` stand in for a parameter**, so they are consumed as
///     one rather than read as dispatch characters.
///
/// An unterminated directive — a `~` with nothing after it, or a `~/` whose
/// closing slash never arrives — ends the iteration rather than yielding a
/// guess. That is a false negative on a control string that is certainly
/// malformed, and it is the direction this package errs in throughout.
pub fn directives(control: &str) -> impl Iterator<Item = Directive> + '_ {
    let mut characters = control.char_indices().peekable();
    std::iter::from_fn(move || {
        loop {
            let (start, character) = characters.next()?;
            if character != '~' {
                continue;
            }
            let mut decorated = false;
            loop {
                let (index, next) = characters.peek().copied()?;
                match next {
                    '0'..='9' | '+' | '-' | ',' | ':' | '@' | '#' | 'v' | 'V' => {
                        decorated = true;
                        characters.next();
                    }
                    '\'' => {
                        decorated = true;
                        characters.next();
                        // Whatever follows is a literal padding character, even
                        // if it looks like a dispatch character.
                        characters.next()?;
                    }
                    _ => {
                        characters.next();
                        let mut end = index + next.len_utf8();
                        if next == '/' {
                            // ~/name/ names a function; the name runs to the
                            // closing slash and is not control-string text.
                            end = loop {
                                let (slash, character) = characters.next()?;
                                if character == '/' {
                                    break slash + 1;
                                }
                            };
                        }
                        return Some(Directive {
                            character: next,
                            decorated,
                            start,
                            end,
                        });
                    }
                }
            }
        }
    })
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

    fn scan(control: &str) -> Vec<(char, bool)> {
        directives(control)
            .map(|directive| (directive.character, directive.decorated))
            .collect()
    }

    // -- the directive grammar ----------------------------------------------

    #[test]
    fn a_plain_directive_is_its_character_and_is_undecorated() {
        assert_eq!(scan("~a"), vec![('a', false)]);
        assert_eq!(scan("hello ~A world ~S"), vec![('A', false), ('S', false)]);
    }

    #[test]
    fn a_prefix_parameter_does_not_hide_the_dispatch_character() {
        assert_eq!(scan("~10,'0D"), vec![('D', true)]);
        assert_eq!(scan("~5,'*d"), vec![('d', true)]);
    }

    /// CLHS 22.3: "Prefix parameters are notated as signed (sign is optional)
    /// decimal numbers". Its own examples are `~3,-4:@s` and `~,+4S`.
    #[test]
    fn a_signed_prefix_parameter_is_not_read_as_a_directive() {
        assert_eq!(scan("~3,-4:@s"), vec![('s', true)]);
        assert_eq!(scan("~,+4S"), vec![('S', true)]);
    }

    /// The single-quote parameter quotes whatever follows it, including a
    /// character that would otherwise open a bracketing construct.
    #[test]
    fn a_quoted_parameter_character_is_not_a_directive() {
        assert_eq!(scan("~'{d"), vec![('d', true)]);
        assert_eq!(scan("~'~d"), vec![('d', true)]);
    }

    #[test]
    fn v_and_hash_stand_in_for_a_parameter() {
        assert_eq!(scan("~vd"), vec![('d', true)]);
        assert_eq!(scan("~V,vF"), vec![('F', true)]);
        assert_eq!(
            scan("~#[none~;one~]"),
            vec![('[', true), (';', false), (']', false)]
        );
    }

    #[test]
    fn modifiers_are_decoration_not_dispatch() {
        assert_eq!(scan("~:@a"), vec![('a', true)]);
        assert_eq!(scan("~@:a"), vec![('a', true)]);
    }

    /// The trap that makes a substring search wrong: `~~` is one directive
    /// emitting a literal tilde, so the `%` after it is ordinary text.
    #[test]
    fn a_literal_tilde_directive_does_not_start_the_next_one() {
        assert_eq!(scan("~~%"), vec![('~', false)]);
        assert_eq!(scan("~~~%"), vec![('~', false), ('%', false)]);
    }

    #[test]
    fn a_call_function_directive_consumes_its_function_name() {
        assert_eq!(scan("~/pkg:fn/~a"), vec![('/', false), ('a', false)]);
        // The name is not scanned for directives of its own.
        assert_eq!(scan("~/a~qb/"), vec![('/', false)]);
    }

    #[test]
    fn an_ignored_newline_directive_is_its_newline() {
        assert_eq!(scan("one ~\n   two"), vec![('\n', false)]);
        assert_eq!(scan("one ~:\n   two"), vec![('\n', true)]);
    }

    #[test]
    fn an_unterminated_directive_ends_the_scan_rather_than_guessing() {
        assert_eq!(scan("done ~"), Vec::new());
        assert_eq!(scan("~a~"), vec![('a', false)]);
        assert_eq!(scan("~/unclosed"), Vec::new());
        assert_eq!(scan("~'"), Vec::new());
    }

    #[test]
    fn a_directives_span_covers_the_tilde_and_the_dispatch_character() {
        let scanned: Vec<Directive> = directives("ab~%cd").collect();
        assert_eq!(scanned.len(), 1);
        assert_eq!((scanned[0].start, scanned[0].end), (2, 4));
    }

    #[test]
    fn adjacent_directives_share_a_boundary() {
        let scanned: Vec<Directive> = directives("~%~&").collect();
        assert_eq!(scanned[0].end, scanned[1].start);
        let apart: Vec<Directive> = directives("~% ~&").collect();
        assert_ne!(apart[0].end, apart[1].start);
    }

    #[test]
    fn folded_lowercases_the_dispatch_character() {
        assert_eq!(directives("~A").next().expect("a directive").folded(), 'a');
    }

    // -- reaching the control string ----------------------------------------

    fn first_form(source: &str) -> (SyntaxTree, ByteSpan) {
        let parsed = tree(source);
        let span = parsed.root_view().children[0].span;
        (parsed, span)
    }

    fn control_of(source: &str) -> Option<String> {
        let parsed = tree(source);
        let root = parsed.root_view();
        literal_control_string(&root.children[0]).map(ToOwned::to_owned)
    }

    #[test]
    fn each_format_head_names_its_own_control_string_slot() {
        assert_eq!(control_string_index("format"), Some(2));
        assert_eq!(control_string_index("cerror"), Some(2));
        assert_eq!(control_string_index("error"), Some(1));
        assert_eq!(control_string_index("warn"), Some(1));
        assert_eq!(control_string_index("format-to-string"), Some(1));
        assert_eq!(control_string_index("princ"), None);
        assert_eq!(control_string_index("list"), None);
    }

    #[test]
    fn the_head_lookup_folds_case_and_package_qualifiers() {
        assert_eq!(control_string_index("FORMAT"), Some(2));
        assert_eq!(control_string_index("cl:format"), Some(2));
        assert_eq!(control_string_index("CL-USER::error"), Some(1));
    }

    #[test]
    fn the_control_string_is_read_from_the_right_slot_per_head() {
        assert_eq!(control_of("(format t \"~a\" x)").as_deref(), Some("~a"));
        assert_eq!(control_of("(error \"~a\" x)").as_deref(), Some("~a"));
        assert_eq!(control_of("(warn \"~a\" x)").as_deref(), Some("~a"));
        assert_eq!(
            control_of("(cerror \"Retry.\" \"~a\" x)").as_deref(),
            Some("~a")
        );
    }

    /// The argument slot is never scanned. A `~` sequence in an ordinary
    /// argument is data being printed, not a directive being interpreted.
    #[test]
    fn a_tilde_in_a_non_control_argument_is_not_reached() {
        assert_eq!(control_of("(format t \"value\" \"~Q\")"), None);
        // `cerror`'s *first* string is its continue-format-control, but the
        // slot this package reads is the second.
        assert_eq!(control_of("(cerror \"~Q\" \"plain\" x)"), None);
    }

    #[test]
    fn a_computed_control_string_is_not_analyzed() {
        assert_eq!(control_of("(format t (banner) x)"), None);
        assert_eq!(control_of("(format t control x)"), None);
    }

    #[test]
    fn a_control_string_with_no_tilde_is_disqualified_before_anything_allocates() {
        assert_eq!(control_of("(format t \"plain text\")"), None);
    }

    #[test]
    fn a_call_too_short_to_have_a_control_slot_is_not_analyzed() {
        assert_eq!(control_of("(format t)"), None);
        assert_eq!(control_of("(format)"), None);
        assert_eq!(control_of("(error)"), None);
    }

    #[test]
    fn a_quoted_string_in_the_control_slot_is_not_a_control_string() {
        assert_eq!(control_of("(format t '\"~a\" x)"), None);
        assert_eq!(control_of("(format t `\"~a\" x)"), None);
    }

    /// The assumption the missing reader-prefix guard rests on: an atom's text
    /// carries its reader prefixes, so a prefixed literal never survives
    /// `strip_prefix('"')`. If this ever stops holding, `'"~a"` starts reading
    /// as a control string and the test above is what says so.
    #[test]
    fn a_reader_prefix_stays_in_the_atoms_text() {
        let parsed = tree("(format t '\"~a\" x)");
        let root = parsed.root_view();
        let control = &root.children[0].children[2];
        assert_eq!(control.reader_prefixes, vec![ReaderPrefix::Quote]);
        assert_eq!(atom_text(control), Some("'\"~a\""));
    }

    #[test]
    fn the_reported_span_is_the_control_string_not_the_call() {
        let parsed = tree("(format t \"~a\" x)");
        let root = parsed.root_view();
        let span = control_string_span(&root.children[0]).expect("a control string span");
        assert_eq!(
            &"(format t \"~a\" x)"[span.start().get()..span.end().get()],
            "\"~a\""
        );
    }

    // -- escapes -------------------------------------------------------------

    #[test]
    fn a_literal_without_a_backslash_is_borrowed() {
        assert!(matches!(resolve_escapes("~a ~s"), Cow::Borrowed(_)));
    }

    #[test]
    fn an_escaped_quote_resolves_to_the_string_format_receives() {
        assert_eq!(resolve_escapes(r#"~a \"~a\""#), r#"~a "~a""#);
        assert_eq!(resolve_escapes(r"~%\~&"), "~%~&");
        assert_eq!(resolve_escapes(r"a\\b"), r"a\b");
    }

    // -- the five quote shapes ----------------------------------------------

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
    fn a_hard_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(format t \"~Q\")").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(evaluated_heads("(quote (format t \"~Q\"))"), vec!["quote"]);
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(format t \"~Q\")").is_empty());
    }

    /// The backquoted list itself is data and is skipped; everything from the
    /// comma down is code and is visited. `t` is an atom, so it contributes no
    /// head of its own.
    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(evaluated_heads("`(a ,(format t \"~Q\"))"), vec!["format"]);
    }

    /// A comma inside a hard quote is a literal comma in a literal list, not an
    /// escape back to code — the shape a single depth counter reads wrongly.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(format t \"~Q\"))").is_empty());
    }

    fn unevaluated_at_first_format(source: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|head| symbol_is(head, "format")) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(
            &parsed,
            span.expect("the source must contain a format call"),
        )
    }

    #[test]
    fn the_span_lookup_agrees_with_the_walk_on_all_five_shapes() {
        assert!(unevaluated_at_first_format("'(format t \"~Q\")"));
        assert!(unevaluated_at_first_format("(quote (format t \"~Q\"))"));
        assert!(unevaluated_at_first_format("`(format t \"~Q\")"));
        assert!(unevaluated_at_first_format("'(a ,(format t \"~Q\"))"));
        assert!(!unevaluated_at_first_format("`(a ,(format t \"~Q\"))"));
        assert!(!unevaluated_at_first_format(
            "(defun f () (format t \"~Q\"))"
        ));
    }

    #[test]
    fn a_span_that_names_no_node_is_judged_by_its_container() {
        let (parsed, form) = first_form("'(format t \"~Q\")");
        assert!(is_unevaluated_at(&parsed, form));
    }
}
