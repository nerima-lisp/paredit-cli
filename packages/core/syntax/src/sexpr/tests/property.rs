use std::collections::HashSet;

use proptest::prelude::*;

use crate::dialect::Dialect;

use super::*;

#[test]
fn property_generated_formatter_output_is_parseable_and_stable() {
    let delimiters = [('(', ')'), ('[', ']'), ('{', '}')];

    for depth in 1..5 {
        for width in 1..6 {
            for (open, close) in delimiters {
                let mut input = String::new();
                for form_index in 0..8 {
                    input.push(open);
                    input.push_str(&format!("root-{depth}-{width}-{form_index}"));
                    for item_index in 0..width {
                        input.push(' ');
                        input.push_str(&format!("atom-{depth}-{item_index}"));
                    }
                    for nested_depth in 0..depth {
                        input.push(' ');
                        input.push(open);
                        input.push_str(&format!("nested-{nested_depth} leaf-{form_index}"));
                        input.push(close);
                    }
                    input.push(close);
                    input.push('\n');
                }

                let formatter = Formatter::new(2);
                let tree = SyntaxTree::parse(&input).expect("generated input parses");
                let formatted = formatter.format(&tree);
                let reparsed =
                    SyntaxTree::parse(&formatted).expect("formatted output parses again");
                let reformatted = formatter.format(&reparsed);

                assert_eq!(
                    formatted, reformatted,
                    "formatter output must be stable after reparsing"
                );
            }
        }
    }
}

/// Every Clojure layout has to survive a reparse unchanged.
///
/// This matters more than it does for Common Lisp, because several Clojure
/// layouts decide their head-line width from the tree's shape rather than from
/// the head alone: whether `defn`'s name is followed by a parameter vector,
/// whether `fn` carries a self-reference name, and how `^meta` descriptors group
/// with the form they decorate. Every one of those decisions has to read the
/// same way on the formatter's own output as it did on the original source.
#[test]
fn property_generated_clojure_formatter_output_is_parseable_and_stable() {
    let heads = [
        "defn",
        "defn-",
        "defmacro",
        "fn",
        "let",
        "loop",
        "doseq",
        "letfn",
        "->",
        "->>",
        "as->",
        "some->",
        "cond",
        "case",
        "condp",
        "ns",
        "def",
        "defonce",
        "defrecord",
        "deftype",
        "defmethod",
        "defprotocol",
        "do",
        "try",
        "comment",
        "when",
        "when-let",
        "if",
        "if-let",
        "doto",
        // An ordinary call, which must keep the general layout.
        "process-batch",
    ];
    let bodies = [
        "",
        " [alpha beta]",
        " [alpha beta] (one alpha)",
        " name [alpha beta] (one alpha) (two beta)",
        " \"docstring\" {:added \"1.0\"} [alpha beta] (one alpha) (two beta)",
        " ([alpha] (one alpha)) ([alpha beta] (two alpha beta))",
        " item (one item) other (two other) :else (three)",
        " ^:private ^:const tagged [alpha] (one alpha)",
        " [alpha (a) beta (b) gamma (c)] (one alpha) (two beta)",
        " value (alpha 1) (beta 2) (gamma 3)",
    ];

    let formatter = Formatter::with_dialect(2, Dialect::Clojure);
    for head in heads {
        for body in bodies {
            let input = format!("({head}{body})\n");
            let tree = SyntaxTree::parse_with_dialect(&input, Dialect::Clojure)
                .expect("generated input parses");
            let formatted = formatter.format(&tree);
            let reparsed = SyntaxTree::parse_with_dialect(&formatted, Dialect::Clojure)
                .expect("formatted output parses again");

            assert_eq!(
                formatter.format(&reparsed),
                formatted,
                "Clojure formatter output must be stable after reparsing {input:?}"
            );
        }
    }
}

#[test]
fn property_generated_rename_preserves_parse_and_atom_spans() {
    for index in 0..64 {
        let from = SymbolName::new(format!("old-symbol-{index}")).expect("valid symbol");
        let to = SymbolName::new(format!("new-symbol-{index}")).expect("valid symbol");
        let input = format!(
            "(defun {from} (x{index} y{index})\n  (let ((local-{index} ({from} x{index})))\n    (list local-{index} y{index} \"{from}\")))\n; {from} in comment\n({from} 1 2)\n",
            from = from.as_str()
        );

        let tree = SyntaxTree::parse(&input).expect("generated input parses");
        for occurrence in tree.atom_occurrences() {
            assert_eq!(
                &input[occurrence.span.as_range()],
                occurrence.text,
                "atom span must slice back to the exact atom text"
            );
        }

        let output = tree.rename_symbol(&from, &to);
        let output_tree = SyntaxTree::parse(&output).expect("renamed output parses");
        let output_atoms = output_tree
            .atom_occurrences()
            .into_iter()
            .map(|occurrence| occurrence.text)
            .collect::<Vec<_>>();

        assert!(!output_atoms.iter().any(|atom| atom == from.as_str()));
        assert!(output_atoms.iter().any(|atom| atom == to.as_str()));
        assert!(output.contains(&format!("\"{}\"", from.as_str())));
        assert!(output.contains(&format!("; {} in comment", from.as_str())));
    }
}

/// Maps `ExpressionKind` to a small discriminant so `(span, kind)` pairs can
/// be hashed without adding a `Hash` derive to the public type.
fn expression_kind_discriminant(kind: ExpressionKind) -> u8 {
    match kind {
        ExpressionKind::Root => 0,
        ExpressionKind::List => 1,
        ExpressionKind::Atom => 2,
    }
}

/// Collects the `(span, kind)` identity of every NON-ROOT node reachable
/// from `view`, i.e. exactly the key shape a side table keyed by
/// `ExpressionView` identity (which carries no `NodeId`) would use.
fn collect_non_root_node_keys(view: &ExpressionView, keys: &mut Vec<(usize, usize, u8)>) {
    if !matches!(view.kind, ExpressionKind::Root) {
        keys.push((
            view.span.start().get(),
            view.span.end().get(),
            expression_kind_discriminant(view.kind),
        ));
    }
    for child in &view.children {
        collect_non_root_node_keys(child, keys);
    }
}

/// Asserts the node-identity invariant side tables rely on: every non-root
/// node's `(span, kind)` pair is unique across the whole tree.
fn assert_node_span_kind_pairs_are_unique(input: &str) {
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp)
        .unwrap_or_else(|error| panic!("{input:?} must parse: {error}"));

    let mut keys = Vec::new();
    collect_non_root_node_keys(&tree.root_view(), &mut keys);

    let mut seen = HashSet::new();
    for key in &keys {
        assert!(
            seen.insert(*key),
            "duplicate (span, kind) key {key:?} across the tree for {input:?}"
        );
    }
}

/// Generates balanced S-expression source using only constructs the Common
/// Lisp reader accepts (see `DialectReaderPolicy::allows_delimiter` and
/// `classify_common_lisp`): parenthesized lists, `#(...)` vector literals,
/// and the `'`, `` ` ``, `,`, `,@`, `#'` reader prefixes. Brackets/braces are
/// deliberately excluded -- CL only allows `Delimiter::Paren`.
fn sexpr_source_strategy() -> impl Strategy<Value = String> {
    let leaf = prop_oneof![
        "[a-zA-Z][a-zA-Z0-9]{0,3}",
        "[a-z]{0,4}".prop_map(|text| format!("\"{text}\"")),
    ];

    leaf.prop_recursive(4, 64, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4)
                .prop_map(|children| format!("({})", children.join(" "))),
            prop::collection::vec(inner.clone(), 0..4)
                .prop_map(|children| format!("#({})", children.join(" "))),
            inner.clone().prop_map(|form| format!("'{form}")),
            inner.clone().prop_map(|form| format!("`{form}")),
            inner.clone().prop_map(|form| format!(",{form}")),
            inner.clone().prop_map(|form| format!(",@{form}")),
            inner.prop_map(|form| format!("#'{form}")),
        ]
    })
}

/// A whole document: one to four top-level forms separated by whitespace.
fn sexpr_document_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(sexpr_source_strategy(), 1..5).prop_map(|forms| forms.join(" \n "))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Foundational invariant for span/kind-keyed side tables (see
    /// `ExpressionView`, which deliberately carries no `NodeId`): in any
    /// successfully parsed document, every non-root node's `(span, kind)`
    /// pair is unique across the whole tree.
    #[test]
    fn pbt_non_root_node_span_kind_pairs_are_unique(input in sexpr_document_strategy()) {
        let tree = SyntaxTree::parse_with_dialect(&input, Dialect::CommonLisp)
            .expect("generated input parses");

        let mut keys = Vec::new();
        collect_non_root_node_keys(&tree.root_view(), &mut keys);

        let mut seen = HashSet::new();
        for key in &keys {
            prop_assert!(
                seen.insert(*key),
                "duplicate (span, kind) key {:?} across the tree for {:?}",
                key,
                input
            );
        }
    }
}

// Hand-picked shapes that are easy for a generator to under-sample but
// exercise the sharpest edges of the invariant above: a document that is a
// single atom (root and child could plausibly share a span), a minimal
// list, a reader-prefixed list, a vector literal, an opaque `#.` read-eval
// form, and an opaque `#+`/`#-` feature-conditional form (the latter two
// are represented as one verbatim atom node with no exposed children, per
// `Node::opaque_reader_form`).

#[test]
fn node_span_kind_pairs_are_unique_for_a_single_atom_document() {
    assert_node_span_kind_pairs_are_unique("x");
}

#[test]
fn node_span_kind_pairs_are_unique_for_a_one_element_list() {
    assert_node_span_kind_pairs_are_unique("(a)");
}

#[test]
fn node_span_kind_pairs_are_unique_for_a_quoted_list() {
    assert_node_span_kind_pairs_are_unique("'(a)");
}

#[test]
fn node_span_kind_pairs_are_unique_for_a_vector_literal() {
    assert_node_span_kind_pairs_are_unique("#(1 2)");
}

#[test]
fn node_span_kind_pairs_are_unique_for_a_read_eval_form() {
    assert_node_span_kind_pairs_are_unique("#.(f)");
}

#[test]
fn node_span_kind_pairs_are_unique_for_a_feature_conditional_form() {
    assert_node_span_kind_pairs_are_unique("#+sbcl (a)");
}

// --- The `--all` edit loop's carried parse ------------------------------
//
// `edit_target_with` in `paredit-core-cli` applies one edit per match, right
// to left, re-parsing between each. It used to parse the whole document twice
// per match: once itself, and once inside
// `Edit::normalize_changed_line_trivia`, which parses the rewrite it is handed
// and then drops it. It now carries both parses forward instead.
//
// That is a claim about *when* the loop parses, never about what a parse
// returns -- there is no incremental parsing here, and deliberately so. What
// could still go wrong is carrying a parse that no longer describes the
// document, which would hand an edit a tree whose spans point into text that
// has moved. The loop asserts against exactly that on every pass in a debug
// build; these tests are the randomized half, comparing the two loops step by
// step over documents the generator varies in form count, form size, and
// whether a form is hidden behind a reader conditional.
//
// The loops below mirror `edit_target_with`; that function cannot be called
// from here, because it lives a package away and reads its document from a
// file or stdin.

/// One top-level form: whether a `#+sbcl` guard hides it from editing, how
/// many markers it offers as targets, and whether its marker line ends in
/// trailing whitespace.
#[derive(Debug, Clone, Copy)]
struct GeneratedForm {
    guarded: bool,
    markers: usize,
    trailing_space: bool,
}

/// A document of `(marker i-j)` forms to aim edits at, some of them inside a
/// reader conditional.
///
/// Three things vary, each because it reaches a branch the others do not:
///
/// * A `#+sbcl` guard. Under `Dialect::CommonLisp` the guard and the form it
///   guards fold into one opaque atom, so a guarded form's markers are not
///   nodes and no edit can land in them. Generating both kinds is what puts a
///   form whose parse must survive untouched next to the ones being rewritten.
/// * The marker count, so that several edits land in one top-level form and
///   each pass has to see the document the pass before it produced.
/// * Trailing whitespace on the marker line. Without it every rewrite here
///   normalizes to itself, and `normalize_changed_line_trivia_reusing_parse`
///   returns its parse every time -- leaving the branch that must *not*
///   return one, because a removal has since edited the text, unreached.
///   `both_edit_loops_agree_when_normalizing_removes_trailing_space` pins that
///   the generator still reaches it.
fn generated_document(forms: &[GeneratedForm]) -> String {
    let mut source = String::new();
    for (index, form) in forms.iter().enumerate() {
        if form.guarded {
            source.push_str("#+sbcl ");
        }
        source.push_str(&format!("(defun form-{index} (x)\n  (list x"));
        for marker in 0..form.markers {
            source.push_str(&format!(" (marker {index}-{marker})"));
        }
        // `done` closes the line after the markers on purpose. `Edit::kill`
        // absorbs the whitespace around what it removes, so a marker that ends
        // its line takes the trailing spaces (and the newline) with it, and
        // the removal branch below would never run.
        source.push_str(" done");
        if form.trailing_space {
            source.push_str("  ");
        }
        source.push_str("\n        more))\n\n");
    }
    source
}

/// Every `(marker ...)` that is a node in its own right, in source order.
///
/// A marker inside a guarded form is skipped here the same way the CLI's
/// selector skips it: `select_at` lands on the enclosing opaque atom, whose
/// span starts before the marker's own text does.
fn editable_marker_spans(source: &str, tree: &SyntaxTree) -> Vec<ByteSpan> {
    let mut spans = Vec::new();
    let mut cursor = 0usize;
    while let Some(found) = source[cursor..].find("(marker ") {
        let start = cursor + found;
        if let Ok(selection) = tree.select_at(start) {
            if selection.span().start().get() == start {
                spans.push(selection.span());
            }
        }
        cursor = start + 1;
    }
    spans
}

/// The document after each pass of the loop, or the pass that refused.
type LoopTrace = Result<Vec<String>, usize>;

/// The loop as it was: one parse of its own plus one inside `normalize`.
fn edit_loop_parsing_every_pass(source: &str, spans: &[ByteSpan], dialect: Dialect) -> LoopTrace {
    let mut trace = Vec::new();
    let mut current = source.to_owned();
    for (pass, span) in spans.iter().rev().enumerate() {
        let Ok(tree) = SyntaxTree::parse_with_dialect(&current, dialect) else {
            return Err(pass);
        };
        let Ok(selection) = tree.select_at(span.start().get()) else {
            return Err(pass);
        };
        if selection.span() != *span {
            return Err(pass);
        }
        let Ok(rewritten) = Edit::kill(&current, &tree, selection) else {
            return Err(pass);
        };
        let Ok(normalized) = Edit::normalize_changed_line_trivia(&current, rewritten, dialect)
        else {
            return Err(pass);
        };
        current = normalized;
        trace.push(current.clone());
    }
    Ok(trace)
}

/// The loop as it is: the parse `normalize` makes is carried into the next
/// pass, and the document's first parse is carried into the first.
///
/// Checks the carried tree against a parse made from scratch on every pass,
/// which is the invariant the change rests on.
fn edit_loop_carrying_the_parse(
    source: &str,
    spans: &[ByteSpan],
    dialect: Dialect,
    seed: SyntaxTree,
    removals: &mut usize,
) -> LoopTrace {
    let mut trace = Vec::new();
    let mut current = source.to_owned();
    let mut parsed = Some(seed);
    for (pass, span) in spans.iter().rev().enumerate() {
        let tree = match parsed.take() {
            Some(tree) => tree,
            None => match SyntaxTree::parse_with_dialect(&current, dialect) {
                Ok(tree) => tree,
                Err(_) => return Err(pass),
            },
        };
        assert_eq!(
            tree,
            SyntaxTree::parse_with_dialect(&current, dialect)
                .expect("the document under edit parses"),
            "the carried parse diverged from a parse of the same text on pass {pass}"
        );
        let Ok(selection) = tree.select_at(span.start().get()) else {
            return Err(pass);
        };
        if selection.span() != *span {
            return Err(pass);
        }
        let Ok(rewritten) = Edit::kill(&current, &tree, selection) else {
            return Err(pass);
        };
        let Ok((normalized, reusable)) =
            Edit::normalize_changed_line_trivia_reusing_parse(&current, rewritten, dialect)
        else {
            return Err(pass);
        };
        // Nothing came back and the text moved: normalization removed
        // something, so the parse it made no longer describes the document.
        if reusable.is_none() && normalized != current {
            *removals += 1;
        }
        parsed = if normalized == current {
            Some(tree)
        } else {
            reusable
        };
        current = normalized;
        trace.push(current.clone());
    }
    Ok(trace)
}

/// Runs both loops over the same targets and returns how many passes
/// normalization removed trailing whitespace on.
#[track_caller]
fn assert_both_edit_loops_agree(forms: &[GeneratedForm], keep: &[bool]) -> usize {
    let dialect = Dialect::CommonLisp;
    let source = generated_document(forms);
    let tree = SyntaxTree::parse_with_dialect(&source, dialect).expect("generated input parses");

    let spans = editable_marker_spans(&source, &tree)
        .into_iter()
        .enumerate()
        .filter(|(index, _)| keep.get(*index).copied().unwrap_or(true))
        .map(|(_, span)| span)
        .collect::<Vec<_>>();
    if spans.is_empty() {
        return 0;
    }

    let mut removals = 0;
    let expected = edit_loop_parsing_every_pass(&source, &spans, dialect);
    let actual = edit_loop_carrying_the_parse(&source, &spans, dialect, tree, &mut removals);
    assert_eq!(
        expected,
        actual,
        "the two loops diverged on {source:?} with {} targets",
        spans.len()
    );

    // A guarded form is opaque, so no edit may have reached inside one.
    if let Ok(trace) = &actual {
        let last = trace.last().expect("at least one pass ran");
        for (index, form) in forms.iter().enumerate() {
            if form.guarded && form.markers > 0 {
                assert!(
                    last.contains(&format!("(marker {index}-0)")),
                    "an edit reached inside the reader conditional on form {index}"
                );
            }
        }
    }
    removals
}

fn generated_form_strategy() -> impl Strategy<Value = GeneratedForm> {
    (any::<bool>(), 0usize..4, any::<bool>()).prop_map(|(guarded, markers, trailing_space)| {
        GeneratedForm {
            guarded,
            markers,
            trailing_space,
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Carrying a parse from one pass of the `--all` edit loop to the next
    /// must produce the same document, byte for byte, after every single pass
    /// -- not merely at the end.
    #[test]
    fn pbt_carrying_the_parse_matches_parsing_every_pass(
        forms in prop::collection::vec(generated_form_strategy(), 1..7),
        keep in prop::collection::vec(any::<bool>(), 0..16),
    ) {
        assert_both_edit_loops_agree(&forms, &keep);
    }
}

/// Two targets in one top-level form: the pass that edits the second must see
/// a parse of the document the first pass produced, not of the one before it.
#[test]
fn both_edit_loops_agree_on_two_targets_in_one_form() {
    assert_both_edit_loops_agree(
        &[GeneratedForm {
            guarded: false,
            markers: 2,
            trailing_space: false,
        }],
        &[true, true],
    );
}

/// An edit in a form neighbouring a reader conditional must leave the
/// conditional's folding alone.
#[test]
fn both_edit_loops_agree_next_to_a_reader_conditional() {
    assert_both_edit_loops_agree(
        &[
            GeneratedForm {
                guarded: false,
                markers: 2,
                trailing_space: false,
            },
            GeneratedForm {
                guarded: true,
                markers: 2,
                trailing_space: false,
            },
            GeneratedForm {
                guarded: false,
                markers: 1,
                trailing_space: false,
            },
        ],
        &[true, true, true],
    );
}

/// Pins the one branch the generator would otherwise be free to stop
/// reaching: the pass where normalization removes trailing whitespace, and so
/// must *not* hand its now-stale parse to the next pass.
#[test]
fn both_edit_loops_agree_when_normalizing_removes_trailing_space() {
    let removals = assert_both_edit_loops_agree(
        &[GeneratedForm {
            guarded: false,
            markers: 3,
            trailing_space: true,
        }],
        &[true, true, true],
    );
    assert!(
        removals > 0,
        "this shape exists to exercise normalization's removal path"
    );
}
