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
