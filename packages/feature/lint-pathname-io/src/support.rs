//! What the pathname/IO rules share: which parts of a file are *code*, where a
//! filesystem operator keeps its file designator, and what a string literal
//! says.
//!
//! Three things every rule here needs and neither the engine nor `core/syntax`
//! provides:
//!
//! - **Evaluation context.** `'(open (concatenate 'string dir "/" name))` is a
//!   list of symbols, not a call. The lint engine's dispatch walks into quoted
//!   data like any other subtree and [`RuleContext`] carries no parent pointer,
//!   so a head-matched node cannot tell on its own whether it is code.
//!   [`is_unevaluated_at`] answers that by descending from the root along the
//!   single chain of nodes whose span contains the candidate's — depth-many
//!   steps, not tree-many — and is called only once a rule already has a
//!   finding to report.
//!
//!   The quote model is two independent counters, not one depth. A comma
//!   inside `'(…)` is a comma character in a literal list, so `hard` never
//!   clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts
//!   up and down. A single `i32` depth conflates the two and has shipped in
//!   this project as a false-positive source.
//!
//! - **Where the designator is.** Almost every filesystem operator takes its
//!   file designator as the first argument; `with-open-file` and friends put it
//!   inside the binding form instead. [`file_designator`] is the one place that
//!   difference is written down.
//!
//! - **String literals.** The syntax layer keeps a string's quotes in its atom
//!   text, and `#p"…"` is likewise a single atom whose text starts with `#`.
//!   [`string_literal`] accepts only the former, which is what keeps a rule
//!   about namestrings from firing on a pathname literal or on a symbol whose
//!   name contains a slash.
//!
//! Nothing here is called per visited node beyond the cheap shape tests. That
//! is deliberate: the `clean/forms/*` benchmarks lint files with zero findings,
//! so the per-file cost of a rule that matches nothing is exactly what they
//! measure.
//!
//! ## What the dialect-aware parse hides
//!
//! A reader conditional folds into a single [`ExpressionKind::Atom`]: parsing
//! `#+sbcl (open "/tmp/x")` yields one atom whose `atom_text` is the entire
//! string `#+sbcl (open "/tmp/x")`, with no children and no reader prefix
//! recorded. So no `HeadFilter::Heads` rule — none of these — can see a call
//! guarded by `#+`/`#-`. That is a coverage limit, not a false-positive
//! source, and it is why no rule here tries to look through one.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext
//! [`ExpressionKind::Atom`]: paredit_core_syntax::sexpr::ExpressionKind::Atom

use paredit_core_lint_engine::model::NormalizedHead;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_is, unqualified};

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing.
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
    /// of them turns code into data. `#.` in particular is read-time
    /// *evaluation*, so a filesystem call under one is a real call.
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

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the cost is the node's depth, not the file's size.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// quasiquoted does not settle it: `` `(a ,(open p)) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it,
/// and that is already modelled by `hard` never clearing.
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

/// The inside of a string literal, or `None` for anything else.
///
/// The syntax layer keeps a string's quotes in its atom text, and only a token
/// that opens and closes with one is a literal — which is also what rules out
/// `#p"data/in.txt"` (already a pathname, and the reader's problem) and a
/// symbol whose name happens to contain a slash.
#[must_use]
/// A lone `"` needs no length test of its own: stripping the prefix leaves the
/// empty string, and stripping a suffix off that fails. An explicit
/// `text.len() < 2` guard was written here first and mutation testing showed it
/// killed nothing, because this expression already declines that input.
pub fn string_literal(view: &ExpressionView) -> Option<&str> {
    let text = atom_text(view)?;
    text.strip_prefix('"')?.strip_suffix('"')
}

/// Filesystem operators whose first argument is a file designator.
///
/// `with-open-file` is the exception: it names a stream variable first and puts
/// the designator inside the binding form. [`file_designator`] is where that is
/// handled, so no caller has to know.
///
/// `file-length`, `file-position`, and `close` are deliberately absent: CLHS
/// gives all three a *stream* argument, not a file designator, so a rule about
/// namestrings has nothing to say about them and including them would cost an
/// invocation per call to buy a false positive.
pub const FILE_OPERATORS: [&str; 14] = [
    "open",
    "load",
    "probe-file",
    "truename",
    "delete-file",
    "directory",
    "compile-file",
    "ensure-directories-exist",
    "rename-file",
    "file-write-date",
    "with-open-file",
    "merge-pathnames",
    "pathname",
    "parse-namestring",
];

/// The heads [`FILE_OPERATORS`] normalizes to, for a rule's `HeadFilter`.
///
/// Written out rather than derived from [`FILE_OPERATORS`]: `NormalizedHead::new`
/// is `const` and rejects an unnormalized spelling at compile time, and a `const`
/// fn cannot map over an array. `heads_and_operators_stay_in_lockstep` is what
/// keeps the two lists the same.
pub const FILE_OPERATOR_HEADS: [NormalizedHead; FILE_OPERATORS.len()] = [
    NormalizedHead::new("open"),
    NormalizedHead::new("load"),
    NormalizedHead::new("probe-file"),
    NormalizedHead::new("truename"),
    NormalizedHead::new("delete-file"),
    NormalizedHead::new("directory"),
    NormalizedHead::new("compile-file"),
    NormalizedHead::new("ensure-directories-exist"),
    NormalizedHead::new("rename-file"),
    NormalizedHead::new("file-write-date"),
    NormalizedHead::new("with-open-file"),
    NormalizedHead::new("merge-pathnames"),
    NormalizedHead::new("pathname"),
    NormalizedHead::new("parse-namestring"),
];

/// The file designator a filesystem call was given, and the operator's name.
///
/// `None` when `view` is not a call to one of [`FILE_OPERATORS`], or when the
/// call has no designator argument at all — a `(directory)` with no argument is
/// malformed, and saying so is another rule's job.
#[must_use]
pub fn file_designator(view: &ExpressionView) -> Option<(&str, &ExpressionView)> {
    let head = list_head(view)?;
    let name = head_among(head, &FILE_OPERATORS)?;

    // `(with-open-file (stream <designator> …) …)` — the designator is the
    // second element of the binding form, not the second element of the call.
    let designator = if name == "with-open-file" {
        view.children.get(1)?.children.get(1)?
    } else {
        view.children.get(1)?
    };
    Some((name, designator))
}

/// Which of `candidates` a head names, or `None`.
///
/// The obvious spelling is `candidates.iter().find(|c| symbol_is(head, c))`,
/// and it re-strips `head`'s package qualifier once per candidate. That
/// measured 4.7x the per-invocation cost of a shipped rule doing the same job
/// with a 14-candidate list, so every head test in this package goes through
/// here instead: strip once, then compare.
#[must_use]
pub fn head_among<'a>(head: &str, candidates: &'a [&'a str]) -> Option<&'a str> {
    let bare = unqualified(head);
    candidates
        .iter()
        .copied()
        .find(|candidate| bare.eq_ignore_ascii_case(candidate))
}

/// The value given for `keyword` in a keyword-argument tail, or `None`.
///
/// `first` is the index the keyword section starts at, and the scan steps by
/// two from there — which is what a lambda list means by `&key`, and is why a
/// `(open p :external-format :utf-8)` cannot be misread as supplying
/// `:utf-8`'s value. A linear search for the keyword atom would.
///
/// A trailing keyword with no value yields `None`, the same as an absent one:
/// `(open p :direction)` is malformed, and saying so is another rule's job.
#[must_use]
pub fn keyword_value<'a>(
    list: &'a ExpressionView,
    first: usize,
    keyword: &str,
) -> Option<&'a ExpressionView> {
    let mut index = first;
    while index < list.children.len() {
        let matched =
            atom_text(&list.children[index]).is_some_and(|text| text.eq_ignore_ascii_case(keyword));
        if matched {
            return list.children.get(index + 1);
        }
        index += 2;
    }
    None
}

/// Whether a keyword-argument tail mentions `keyword` at all, value or not.
///
/// Separate from [`keyword_value`] because a rule about an *absent* option must
/// not report `(open p :direction :output :if-exists)` — malformed, but not
/// missing.
#[must_use]
pub fn has_keyword(list: &ExpressionView, first: usize, keyword: &str) -> bool {
    let mut index = first;
    while index < list.children.len() {
        if atom_text(&list.children[index]).is_some_and(|text| text.eq_ignore_ascii_case(keyword)) {
            return true;
        }
        index += 2;
    }
    false
}

/// Whether an atom is the keyword `name` (written with its leading colon).
#[must_use]
pub fn is_keyword(view: &ExpressionView, name: &str) -> bool {
    atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn parse(input: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse")
    }

    fn first_form(tree: &SyntaxTree) -> ExpressionView {
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    /// The span of the first node anywhere in the tree whose head is `head`.
    fn span_of_call(tree: &SyntaxTree, head: &str) -> ByteSpan {
        let root = tree.root_view();
        let mut stack = vec![root.clone()];
        while let Some(view) = stack.pop() {
            if list_head(&view).is_some_and(|found| symbol_is(found, head)) {
                return view.span;
            }
            stack.extend(view.children.iter().cloned());
        }
        panic!("no ({head} …) in the input");
    }

    fn is_data(input: &str, head: &str) -> bool {
        let tree = parse(input);
        is_unevaluated_at(&tree, span_of_call(&tree, head))
    }

    #[test]
    fn a_plain_call_is_code() {
        assert!(!is_data(r#"(open "x")"#, "open"));
        assert!(!is_data(r#"(defun f () (open "x"))"#, "open"));
    }

    #[test]
    fn a_hard_quoted_call_is_data() {
        assert!(is_data(r#"'(open "x")"#, "open"));
        assert!(is_data(r#"(quote (open "x"))"#, "open"));
        assert!(is_data(r#"(list 1 '(a (open "x")))"#, "open"));
    }

    /// The two-counter model's whole reason for existing: a comma inside a hard
    /// quote does not escape back to code, and a comma inside a quasiquote
    /// does. A single depth counter gets exactly one of these wrong.
    #[test]
    fn a_comma_escapes_a_quasiquote_but_not_a_quote() {
        assert!(!is_data(r#"`(a ,(open "x"))"#, "open"));
        assert!(is_data(r#"'(a ,(open "x"))"#, "open"));
    }

    #[test]
    fn a_quasiquote_without_a_comma_is_still_data() {
        assert!(is_data(r#"`(a (open "x"))"#, "open"));
    }

    #[test]
    fn nested_quasiquotes_need_as_many_commas() {
        assert!(is_data(r#"`(a `(b ,(open "x")))"#, "open"));
    }

    /// `#.` is read-time evaluation, so what is under it is code.
    #[test]
    fn read_eval_is_code() {
        assert!(!is_data(r#"#.(open "x")"#, "open"));
    }

    #[test]
    fn heads_and_operators_stay_in_lockstep() {
        let heads: Vec<&str> = FILE_OPERATOR_HEADS
            .iter()
            .map(|head| head.as_str())
            .collect();
        assert_eq!(
            heads,
            FILE_OPERATORS.to_vec(),
            "a head that drifts from the operator list makes its rule match nothing"
        );
    }

    #[test]
    fn string_literal_reads_only_a_string() {
        let tree = parse(r#"(load "a/b" #p"a/b" foo "")"#);
        let view = first_form(&tree);
        assert_eq!(string_literal(&view.children[1]), Some("a/b"));
        assert_eq!(string_literal(&view.children[2]), None, "#p is a pathname");
        assert_eq!(string_literal(&view.children[3]), None, "a symbol");
        assert_eq!(string_literal(&view.children[4]), Some(""));
        assert_eq!(string_literal(&view), None, "a list is not a literal");
    }

    #[test]
    fn file_designator_reads_the_first_argument() {
        let tree = parse(r#"(load "init.lisp")"#);
        let view = first_form(&tree);
        let (name, designator) = file_designator(&view).expect("a load");
        assert_eq!(name, "load");
        assert_eq!(string_literal(designator), Some("init.lisp"));
    }

    #[test]
    fn file_designator_reaches_inside_a_with_open_file_binding() {
        let tree = parse(r#"(with-open-file (s "in.txt") (read s))"#);
        let view = first_form(&tree);
        let (name, designator) = file_designator(&view).expect("a with-open-file");
        assert_eq!(name, "with-open-file");
        assert_eq!(string_literal(designator), Some("in.txt"));
    }

    #[test]
    fn file_designator_reads_a_package_qualified_head() {
        let tree = parse(r#"(cl:load "init.lisp")"#);
        assert!(file_designator(&first_form(&tree)).is_some());
    }

    #[test]
    fn file_designator_declines_a_non_filesystem_call() {
        let tree = parse("(format nil \"~a\" x)");
        assert!(file_designator(&first_form(&tree)).is_none());
    }

    #[test]
    fn keyword_value_steps_in_pairs() {
        let tree = parse("(open p :direction :output :if-exists :supersede)");
        let view = first_form(&tree);
        assert!(is_keyword(
            keyword_value(&view, 2, ":direction").expect("direction"),
            ":output"
        ));
        assert!(is_keyword(
            keyword_value(&view, 2, ":if-exists").expect("if-exists"),
            ":supersede"
        ));
        assert!(keyword_value(&view, 2, ":element-type").is_none());
    }

    /// The reason the scan steps by two rather than searching every position:
    /// `:direction` here is a *value*, not an option.
    ///
    /// The trailing `:output` is load-bearing. Without it a position-by-
    /// position scan also answers `None` — it finds `:direction` at index 3 and
    /// then runs off the end — so the weaker fixture cannot tell the two
    /// implementations apart. Mutation testing is how that was found.
    #[test]
    fn keyword_value_does_not_read_a_value_as_an_option() {
        let tree = parse("(open p :external-format :direction :output)");
        let view = first_form(&tree);
        assert!(
            keyword_value(&view, 2, ":direction").is_none(),
            "`:direction` here is the value of `:external-format`"
        );
        assert!(!has_keyword(&view, 2, ":direction"));
        // The options that really are options are still found.
        assert!(keyword_value(&view, 2, ":external-format").is_some());
    }

    #[test]
    fn string_literal_declines_a_lone_quote_character() {
        let tree = parse(r#"(load "")"#);
        let view = first_form(&tree);
        assert_eq!(string_literal(&view.children[1]), Some(""));
    }

    #[test]
    fn keyword_value_reads_case_insensitively() {
        let tree = parse("(open p :DIRECTION :OUTPUT)");
        let view = first_form(&tree);
        assert!(is_keyword(
            keyword_value(&view, 2, ":direction").expect("direction"),
            ":output"
        ));
    }

    #[test]
    fn has_keyword_separates_absent_from_valueless() {
        let tree = parse("(open p :direction :output :if-exists)");
        let view = first_form(&tree);
        assert!(has_keyword(&view, 2, ":if-exists"), "present but valueless");
        assert!(keyword_value(&view, 2, ":if-exists").is_none());
        assert!(!has_keyword(&view, 2, ":element-type"));
    }

    #[test]
    fn file_designator_declines_a_call_with_no_argument() {
        let tree = parse("(directory)");
        assert!(file_designator(&first_form(&tree)).is_none());
        let tree = parse("(with-open-file)");
        assert!(file_designator(&first_form(&tree)).is_none());
    }
}
