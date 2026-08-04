use super::*;
use crate::dialect::Dialect;
use crate::sexpr::parser::MAX_DISCARDED_FORM_STACK_FRAMES;
use crate::sexpr::reader_policy::{DialectReaderPolicy, HyStringExtent, LongStringExtent};

#[test]
fn parses_balanced_document() {
    let tree = SyntaxTree::parse("(defun add (x y) (+ x y))").expect("valid");
    assert_eq!(tree.root_children().len(), 1);
}

#[test]
fn applies_dialect_reader_collisions_without_splitting_reader_forms() {
    struct Case {
        dialect: Dialect,
        input: &'static str,
        delimiter: Delimiter,
        children: &'static [&'static str],
    }

    let cases = [
        Case {
            dialect: Dialect::CommonLisp,
            input: "(#+feature guarded tail)",
            delimiter: Delimiter::Paren,
            children: &["#+feature guarded", "tail"],
        },
        Case {
            dialect: Dialect::EmacsLisp,
            input: "[#'f tail]",
            delimiter: Delimiter::Bracket,
            children: &["#'f", "tail"],
        },
        Case {
            dialect: Dialect::Scheme,
            input: "(#;discard kept)",
            delimiter: Delimiter::Paren,
            children: &["kept"],
        },
        Case {
            dialect: Dialect::Clojure,
            input: "{left,right}",
            delimiter: Delimiter::Brace,
            children: &["left", "right"],
        },
        Case {
            dialect: Dialect::Janet,
            input: "[;value # ignored\n next]",
            delimiter: Delimiter::Bracket,
            children: &[";value", "next"],
        },
        Case {
            dialect: Dialect::Fennel,
            input: "{#(value) tail}",
            delimiter: Delimiter::Brace,
            children: &["#(value)", "tail"],
        },
    ];

    for case in cases {
        let tree = SyntaxTree::parse_with_dialect(case.input, case.dialect)
            .unwrap_or_else(|error| panic!("{}: {error}", case.dialect.label()));
        let root = tree.root_view();
        assert_eq!(root.children.len(), 1, "{}", case.dialect.label());
        let form = &root.children[0];
        assert_eq!(
            form.delimiter,
            Some(case.delimiter),
            "{}",
            case.dialect.label()
        );
        let children = form
            .children
            .iter()
            .map(|child| child.span.slice(case.input))
            .collect::<Vec<_>>();
        assert_eq!(children, case.children, "{}", case.dialect.label());
    }
}

#[test]
fn multi_datum_reader_forms_are_single_siblings() {
    let cases = [
        (
            Dialect::CommonLisp,
            "(#+feature (guarded value) tail)",
            &["#+feature (guarded value)", "tail"] as &[&str],
        ),
        // The permissive reader reaches the same shape. It used to produce
        // `#+`, `feature`, `(guarded value)`, `tail` -- four siblings where
        // Common Lisp sees two.
        (
            Dialect::Unknown,
            "(#+feature (guarded value) tail)",
            &["#+feature (guarded value)", "tail"] as &[&str],
        ),
        (
            Dialect::Clojure,
            "(^:private target tail)",
            &["^:private", "target", "tail"] as &[&str],
        ),
    ];

    for (dialect, input, expected) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("valid reader form");
        let form = &tree.root_view().children[0];
        let children = form
            .children
            .iter()
            .map(|child| child.span.slice(input))
            .collect::<Vec<_>>();
        assert_eq!(children, expected, "{}", dialect.label());
    }
}

#[test]
fn unsupported_dispatch_fails_closed_in_live_and_discarded_forms() {
    let cases = [
        (Dialect::CommonLisp, "#?value"),
        (Dialect::CommonLisp, "#12Q"),
        (Dialect::CommonLisp, "#12Qvalue"),
        (Dialect::EmacsLisp, "#(value)"),
        (Dialect::Scheme, "#_value"),
        (Dialect::Scheme, "#12Qvalue"),
        (Dialect::Clojure, "#;value"),
        (Dialect::Clojure, "#?value"),
        (Dialect::Clojure, "#12Qvalue"),
        (Dialect::CommonLisp, "#+feature #?value"),
        (Dialect::CommonLisp, "#+feature #12Q"),
        (Dialect::CommonLisp, "#+feature #12Qvalue"),
        (Dialect::Scheme, "#;#?value"),
        (Dialect::Scheme, "#;#12Qvalue"),
        (Dialect::Clojure, "#_#;value"),
        (Dialect::Clojure, "#_#?value"),
        (Dialect::Clojure, "#_#12Qvalue"),
    ];

    for (dialect, input) in cases {
        let error = SyntaxTree::parse_with_dialect(input, dialect).unwrap_err();
        assert!(
            matches!(error, ParseError::UnsupportedReaderDispatch { .. }),
            "{} returned the wrong error for {input}: {error}",
            dialect.label(),
        );
        assert!(error.to_string().contains("unsupported reader dispatch"));
    }

    assert_eq!(
        SyntaxTree::parse_with_dialect("#_value", Dialect::Scheme).unwrap_err(),
        ParseError::UnsupportedReaderDispatch {
            dispatch: "#".to_owned(),
            position: 0,
        }
    );
}

#[test]
fn common_lisp_atom_like_dispatches_round_trip_losslessly() {
    let cases = [
        "#:done", "#36rz", "#36RZ", "#37r10", "#16ra.", "#b1010", "#o17", "#d10", "#xFF",
    ];

    for input in cases {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp)
            .expect("valid atom dispatch");
        let root = tree.root_view();
        assert_eq!(root.children.len(), 1, "{input}");
        assert_eq!(root.children[0].span.slice(input), input);
        assert_eq!(root.children[0].text.as_deref(), Some(input));
    }
}

#[test]
fn standard_dialect_dispatch_forms_are_single_opaque_spans() {
    let cases = [
        (
            Dialect::CommonLisp,
            "#P\"/tmp/example.lisp\" tail",
            "#P\"/tmp/example.lisp\"",
        ),
        (
            Dialect::CommonLisp,
            "#S(point :x 1 :y 2) tail",
            "#S(point :x 1 :y 2)",
        ),
        (Dialect::CommonLisp, "#A(1 2) tail", "#A(1 2)"),
        (
            Dialect::CommonLisp,
            "#2a((1 2) (3 4)) tail",
            "#2a((1 2) (3 4))",
        ),
        (
            Dialect::CommonLisp,
            "#1=(node . #1#) tail",
            "#1=(node . #1#)",
        ),
        (Dialect::CommonLisp, "#1# tail", "#1#"),
        (Dialect::Scheme, "#1=(node . #1#) tail", "#1=(node . #1#)"),
        (Dialect::Scheme, "#1# tail", "#1#"),
        (Dialect::Scheme, "#u8(1 2 3) tail", "#u8(1 2 3)"),
        (Dialect::Clojure, r##"#"foo.*" tail"##, r##"#"foo.*""##),
        (
            Dialect::Clojure,
            r#"#:person{:first "Ada"} tail"#,
            r#"#:person{:first "Ada"}"#,
        ),
        (
            Dialect::Clojure,
            r#"#inst "1985-04-12T23:20:50.52-00:00" tail"#,
            r#"#inst "1985-04-12T23:20:50.52-00:00""#,
        ),
        (Dialect::Clojure, "#+/foo 1 tail", "#+/foo 1"),
    ];

    for (dialect, input, expected_span) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("valid dispatch form");
        let root = tree.root_view();
        assert_eq!(root.children.len(), 2, "{}", dialect.label());
        assert_eq!(
            root.children[0].span.slice(input),
            expected_span,
            "{}",
            dialect.label()
        );
        assert_eq!(root.children[1].text.as_deref(), Some("tail"));
    }
}

#[test]
fn standard_dispatch_forms_require_their_payload_datum() {
    let cases = [
        (Dialect::CommonLisp, "#P"),
        (Dialect::CommonLisp, "#S"),
        (Dialect::CommonLisp, "#A"),
        (Dialect::CommonLisp, "#2A"),
        (Dialect::CommonLisp, "#1="),
        (Dialect::Scheme, "#1="),
    ];

    for (dialect, input) in cases {
        assert_eq!(
            SyntaxTree::parse_with_dialect(input, dialect),
            Err(ParseError::MissingReaderForm(0)),
            "{}: {input}",
            dialect.label()
        );
    }
}

#[test]
fn standard_dispatch_forms_are_consumed_inside_skipped_datums() {
    let cases = [
        (
            Dialect::CommonLisp,
            "#+feature #2A((1 2) (3 4)) tail",
            &["#+feature #2A((1 2) (3 4))", "tail"] as &[&str],
        ),
        (
            Dialect::CommonLisp,
            "#+feature #1=(node . #1#) tail",
            &["#+feature #1=(node . #1#)", "tail"] as &[&str],
        ),
        (
            Dialect::CommonLisp,
            "#+feature #:done tail",
            &["#+feature #:done", "tail"] as &[&str],
        ),
        (
            Dialect::CommonLisp,
            "#+feature #36rz tail",
            &["#+feature #36rz", "tail"] as &[&str],
        ),
        (Dialect::Scheme, "#;#1=(node . #1#) tail", &["tail"]),
        (Dialect::Scheme, "#;#1# tail", &["tail"]),
        (Dialect::Clojure, "#_#+/foo 1 tail", &["tail"]),
    ];

    for (dialect, input, expected) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("valid skipped form");
        let spans = tree
            .root_view()
            .children
            .iter()
            .map(|child| child.span.slice(input))
            .collect::<Vec<_>>();
        assert_eq!(spans, expected, "{}", dialect.label());
    }
}

#[test]
fn opaque_dialect_dispatch_forms_are_not_traversed_by_rename() {
    let cases = [
        (
            Dialect::Clojure,
            "#:foo{:key foo} foo",
            "#:foo{:key foo} bar",
        ),
        (
            Dialect::CommonLisp,
            "#S(node :value foo) foo",
            "#S(node :value foo) bar",
        ),
        (
            Dialect::CommonLisp,
            "#1=(foo . #1#) foo",
            "#1=(foo . #1#) bar",
        ),
        (Dialect::Scheme, "#1=(foo . #1#) foo", "#1=(foo . #1#) bar"),
    ];

    for (dialect, input, expected) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("valid reader form");
        assert_eq!(
            tree.rename_symbol(
                &SymbolName::new("foo").expect("source symbol"),
                &SymbolName::new("bar").expect("target symbol"),
            ),
            expected,
            "{}",
            dialect.label()
        );
    }
}

#[test]
fn parses_reader_delimiters() {
    let tree = SyntaxTree::parse("(mapv inc [1 2 {:x 3}])").expect("valid");
    assert_eq!(Formatter::new(2).format(&tree), "(mapv inc [1 2 {:x 3}])\n");
}

#[test]
fn parses_common_lisp_reader_prefixes() {
    let input = "'value #'call `(list ,item ,@rest)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let root = tree.root_children();
    assert_eq!(root.len(), 3);

    let quoted = tree.select_path(&parse_path("0")).expect("quoted").view();
    assert_eq!(quoted.reader_prefixes, vec![ReaderPrefix::Quote]);
    assert_eq!(quoted.text.as_deref(), Some("'value"));

    let function = tree.select_path(&parse_path("1")).expect("function").view();
    assert_eq!(function.reader_prefixes, vec![ReaderPrefix::Function]);
    assert_eq!(function.text.as_deref(), Some("#'call"));

    let quasiquoted = tree
        .select_path(&parse_path("2"))
        .expect("quasiquoted")
        .view();
    assert_eq!(quasiquoted.reader_prefixes, vec![ReaderPrefix::Quasiquote]);
    assert_eq!(quasiquoted.content_span.slice(input), "(list ,item ,@rest)");
    assert_eq!(
        quasiquoted.children[1].reader_prefixes,
        vec![ReaderPrefix::Unquote]
    );
    assert_eq!(
        quasiquoted.children[2].reader_prefixes,
        vec![ReaderPrefix::UnquoteSplicing]
    );
}

#[test]
fn preserves_stacked_quasiquote_and_unquote_prefix_order() {
    let tree = SyntaxTree::parse("``(list ,quoted ,,evaluated)").expect("valid");
    let quasiquoted = tree
        .select_path(&parse_path("0"))
        .expect("quasiquoted")
        .view();

    assert_eq!(
        quasiquoted.reader_prefixes,
        vec![ReaderPrefix::Quasiquote, ReaderPrefix::Quasiquote]
    );
    assert_eq!(
        quasiquoted.children[1].reader_prefixes,
        vec![ReaderPrefix::Unquote]
    );
    assert_eq!(
        quasiquoted.children[2].reader_prefixes,
        vec![ReaderPrefix::Unquote, ReaderPrefix::Unquote]
    );
}

#[test]
fn parses_clojure_hash_literals_as_one_node() {
    // `#{...}` (set) and `#(...)` (anonymous fn / CL-Scheme vector literal)
    // glue `#` directly onto the following collection with no space in every
    // supported dialect, so both must parse as one prefixed list rather than
    // a disconnected `#` atom followed by an unrelated sibling list.
    let tree = SyntaxTree::parse("#{1 2 3} #(+ % 1)").expect("valid");
    let root = tree.root_children();
    assert_eq!(root.len(), 2);

    let set = tree.select_path(&parse_path("0")).expect("set").view();
    assert_eq!(set.reader_prefixes, vec![ReaderPrefix::HashLiteral]);
    assert_eq!(set.delimiter, Some(Delimiter::Brace));

    let anon_fn = tree.select_path(&parse_path("1")).expect("anon_fn").view();
    assert_eq!(anon_fn.reader_prefixes, vec![ReaderPrefix::HashLiteral]);
    assert_eq!(anon_fn.delimiter, Some(Delimiter::Paren));
}

#[test]
fn parses_clojure_metadata_prefix_on_map_and_atom() {
    let tree = SyntaxTree::parse(r#"^{:doc "x"} target ^:private y"#).expect("valid");
    let root = tree.root_children();
    assert_eq!(root.len(), 4);

    let metadata_map = tree.select_path(&parse_path("0")).expect("map").view();
    assert_eq!(metadata_map.reader_prefixes, vec![ReaderPrefix::Metadata]);
    assert_eq!(metadata_map.delimiter, Some(Delimiter::Brace));

    let target = tree.select_path(&parse_path("1")).expect("target").view();
    assert_eq!(target.reader_prefixes, Vec::new());
    assert_eq!(target.text.as_deref(), Some("target"));

    let metadata_keyword = tree.select_path(&parse_path("2")).expect("kw").view();
    assert_eq!(
        metadata_keyword.reader_prefixes,
        vec![ReaderPrefix::Metadata]
    );
    assert_eq!(metadata_keyword.text.as_deref(), Some("^:private"));
}

#[test]
fn clojure_metadata_keeps_target_live_and_discard_skips_target() {
    let input = "^:private (defn foo [] (foo)) (foo)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("valid metadata");

    let root = tree.root_view();
    assert_eq!(root.children.len(), 3);
    assert_eq!(
        root.children[0].reader_prefixes,
        vec![ReaderPrefix::Metadata]
    );
    assert_eq!(root.children[0].text.as_deref(), Some("^:private"));
    assert_eq!(root.children[1].span.slice(input), "(defn foo [] (foo))");
    assert_eq!(root.children[2].span.slice(input), "(foo)");

    let foo_paths = tree
        .atom_occurrences()
        .into_iter()
        .filter(|occurrence| occurrence.text == "foo")
        .map(|occurrence| occurrence.path.to_string())
        .collect::<Vec<_>>();
    assert_eq!(foo_paths, vec!["1.1", "1.3.0", "2.0"]);

    let outline = tree.outline(|head| Dialect::Clojure.is_definition_head(head));
    assert_eq!(outline.len(), 2);
    assert_eq!(outline[0].path.to_string(), "1");
    assert_eq!(outline[0].head.as_deref(), Some("defn"));
    assert!(outline[0].definition_like);

    let skipped_input = "#_^:private (defn foo [] (foo)) tail";
    let skipped = SyntaxTree::parse_with_dialect(skipped_input, Dialect::Clojure)
        .expect("valid discarded metadata");
    let spans = skipped
        .root_view()
        .children
        .iter()
        .map(|child| child.span.slice(skipped_input))
        .collect::<Vec<_>>();
    assert_eq!(spans, vec!["tail"]);
}

#[test]
fn parses_clojure_reader_conditionals_as_one_node() {
    let tree =
        SyntaxTree::parse("#?(:clj (foo) :cljs (bar)) #?@(:clj [a] :cljs [b])").expect("valid");
    let root = tree.root_children();
    assert_eq!(root.len(), 2);

    let conditional = tree
        .select_path(&parse_path("0"))
        .expect("conditional")
        .view();
    assert_eq!(
        conditional.reader_prefixes,
        vec![ReaderPrefix::ReaderConditional]
    );
    assert_eq!(conditional.delimiter, Some(Delimiter::Paren));

    let splicing = tree.select_path(&parse_path("1")).expect("splicing").view();
    assert_eq!(
        splicing.reader_prefixes,
        vec![ReaderPrefix::ReaderConditionalSplicing]
    );
    assert_eq!(splicing.delimiter, Some(Delimiter::Paren));
}

#[test]
fn parses_common_lisp_reader_eval_as_opaque_form() {
    let tree = SyntaxTree::parse("#.(foo (bar baz))").expect("valid");
    let root = tree.root_children();
    assert_eq!(root.len(), 1);

    let expression = tree
        .select_path(&parse_path("0"))
        .expect("expression")
        .view();
    assert_eq!(expression.reader_prefixes, vec![ReaderPrefix::ReadEval]);
    assert_eq!(expression.kind, ExpressionKind::List);
}

#[test]
fn skips_common_lisp_reader_comments() {
    let input = "(foo bar) #;(foo baz) (foo qux)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(tree.root_children().len(), 2);
    let from = SymbolName::new("foo").expect("symbol");
    let to = SymbolName::new("bar").expect("symbol");
    assert_eq!(
        tree.rename_symbol(&from, &to),
        "(bar bar) #;(foo baz) (bar qux)"
    );
}

#[test]
fn skips_clojure_discard_forms() {
    // `#_` is Clojure's discard reader macro: it reads and discards exactly
    // one following form, the same shape as Scheme/CL `#;` datum comments,
    // so it must not surface as a live tree node or a rename target.
    let input = "(foo bar) #_(foo baz) (foo qux)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(tree.root_children().len(), 2);
    let from = SymbolName::new("foo").expect("symbol");
    let to = SymbolName::new("bar").expect("symbol");
    assert_eq!(
        tree.rename_symbol(&from, &to),
        "(bar bar) #_(foo baz) (bar qux)"
    );
}

#[test]
fn keeps_reader_prefix_pending_across_a_reader_comment() {
    let input = "'#;ignored (kept)";
    let tree = SyntaxTree::parse(input).expect("valid");

    assert_eq!(tree.root_children().len(), 1);
    let kept = tree.select_path(&parse_path("0")).expect("kept form");
    assert_eq!(kept.text(), input);
    assert_eq!(kept.view().reader_prefixes, vec![ReaderPrefix::Quote]);
}

#[test]
fn keeps_discarded_prefix_pending_across_a_nested_reader_comment() {
    let input = "#;'#;ignored (kept) (live)";
    let tree = SyntaxTree::parse(input).expect("valid");

    assert_eq!(tree.root_children().len(), 1);
    assert_eq!(
        tree.select_path(&parse_path("0"))
            .expect("live form")
            .text(),
        "(live)"
    );
}

#[test]
fn rejects_reader_prefixes_without_a_form() {
    for input in ["'", "`", ",", ",@", "#'", "#.", "#?", "#?@", "^"] {
        assert_eq!(
            SyntaxTree::parse(input).unwrap_err(),
            ParseError::MissingReaderForm(0),
            "input: {input}"
        );
    }
}

/// A reader prefix with a *closing delimiter* after it is refused, in every
/// dialect, exactly as one at end of input is.
///
/// It used to parse clean with the prefix silently dropped: `form` collected
/// the prefixes, found `)` instead of a datum, and called `close_list`
/// without ever consuming them. `(a ')` therefore became the tree for `(a)`,
/// `edit format` re-emitted it without the quote, and the result parsed — a
/// meaning change no reparse-based write guard could see. `skip_form`, the
/// scanning twin of `form`, already refused the same shape.
///
/// The position reported is the *prefix's*, not the delimiter's, matching
/// [`ParseError::MissingReaderForm`]'s end-of-input spelling above.
#[test]
fn rejects_a_reader_prefix_before_a_closing_delimiter() {
    let dialects = [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Racket,
        Dialect::Clojure,
        Dialect::Fennel,
        Dialect::Lfe,
        Dialect::Unknown,
    ];

    for dialect in dialects {
        assert_eq!(
            SyntaxTree::parse_with_dialect("(a ')", dialect).unwrap_err(),
            ParseError::MissingReaderForm(3),
            "dialect: {}",
            dialect.label()
        );
        assert_eq!(
            SyntaxTree::parse_with_dialect("(a `)", dialect).unwrap_err(),
            ParseError::MissingReaderForm(3),
            "dialect: {}",
            dialect.label()
        );
        // Nested, and alone, so the fix is not merely about a top-level list.
        assert_eq!(
            SyntaxTree::parse_with_dialect("(a (b ') c)", dialect).unwrap_err(),
            ParseError::MissingReaderForm(6),
            "dialect: {}",
            dialect.label()
        );
        assert_eq!(
            SyntaxTree::parse_with_dialect("(')", dialect).unwrap_err(),
            ParseError::MissingReaderForm(1),
            "dialect: {}",
            dialect.label()
        );
        // A prefix that *does* have a form after it is untouched, including on
        // the last element of a list — the neighbouring case a fix here could
        // plausibly over-reach into.
        let tree = SyntaxTree::parse_with_dialect("(a 'b)", dialect)
            .unwrap_or_else(|error| panic!("{} rejected (a 'b): {error:?}", dialect.label()));
        assert_eq!(tree.root_view().children[0].children.len(), 2);
        assert_eq!(
            tree.root_view().children[0].children[1].reader_prefixes,
            vec![ReaderPrefix::Quote],
            "dialect: {}",
            dialect.label()
        );
    }
}

/// Hy is absent from the matrix above because `,` is not a reader macro there,
/// so there is no dangling prefix to refuse.
///
/// `hy/reader/hy_reader.py` registers its reader macros with `@reader_for`, and
/// the comma has no entry: the list is `'`, `` ` ``, `~`, `(`, `[`, `{`, `#{`,
/// `#(`, `#`, `#_`, `#*`, `#**`, `#^` and `#[`. It does not end a token either
/// — `NON_IDENT = set("()[]{};\"'`~")` is the complete set of identifier
/// terminators. A comma is therefore an ordinary symbol constituent, and Hy
/// spells unquote `~`, not `,`:
///
/// ```text
/// $ hy -c '(print (list (hy.read-many "(foo ,bar)")))'
/// [Expression([Symbol('foo'), Symbol(',bar')])]
/// ```
///
/// Every expectation below was read off Hy 1.3.1 with that command.
#[test]
fn hy_reads_a_comma_as_a_symbol_constituent_not_as_unquote() {
    struct Case {
        input: &'static str,
        children: &'static [&'static str],
        hy_reads: &'static str,
    }

    let cases = [
        // The shape this test exists for. Hy's tuple constructor is the symbol
        // `,`, so `(,)` is the empty tuple and appears throughout real Hy —
        // including Hy's own `contrib/walk.hy`, `hylang/simalq` and
        // `kanaka/mal`. Read as an unquote it was a hard parse failure.
        Case {
            input: "(,)",
            children: &[","],
            hy_reads: "Expression([Symbol(',')])",
        },
        Case {
            input: "(, 1 2)",
            children: &[",", "1", "2"],
            hy_reads: "Expression([Symbol(','), Integer(1), Integer(2)])",
        },
        // A trailing comma before a closing bracket or brace, idiomatic in
        // Hy's Python-flavoured list and dict literals.
        Case {
            input: "[1 ,]",
            children: &["1", ","],
            hy_reads: "List([Integer(1), Symbol(',')])",
        },
        Case {
            input: "{\"a\" 1 ,}",
            children: &["\"a\"", "1", ","],
            hy_reads: "Dict([String('a'), Integer(1), Symbol(',')])",
        },
        // A comma between two forms is its own datum, not a prefix on the one
        // after it. This parsed clean before and was silently the wrong tree:
        // two children, the second an unquoted `b`.
        Case {
            input: "(a , b)",
            children: &["a", ",", "b"],
            hy_reads: "Expression([Symbol('a'), Symbol(','), Symbol('b')])",
        },
        // Glued to what follows it, a comma is part of that symbol.
        Case {
            input: "(foo ,bar)",
            children: &["foo", ",bar"],
            hy_reads: "Expression([Symbol('foo'), Symbol(',bar')])",
        },
        // Glued to what precedes it, likewise — `[1, 2]` is two data, not
        // three, because the comma belongs to the token before it.
        Case {
            input: "[1, 2]",
            children: &["1,", "2"],
            hy_reads: "List([Integer(1), Integer(2)])",
        },
        // `,@` is not unquote-splicing either; `@` is not in `NON_IDENT`.
        Case {
            input: "(,@)",
            children: &[",@"],
            hy_reads: "Expression([Symbol(',@')])",
        },
        Case {
            input: "(,,)",
            children: &[",,"],
            hy_reads: "Expression([Symbol(',,')])",
        },
        // The `contrib/walk.hy` shape: a quoted comma, where the quote is a
        // real prefix and the comma is the datum it quotes.
        Case {
            input: "(= (first x) ',)",
            children: &["=", "(first x)", "',"],
            hy_reads: "Expression([Symbol('='), Expression([Symbol('.'), ...]), \
                       Expression([Symbol('quote'), Symbol(',')])])",
        },
    ];

    for case in cases {
        let tree = SyntaxTree::parse_with_dialect(case.input, Dialect::Hy)
            .unwrap_or_else(|error| panic!("{}: {error:?}", case.input));
        let form = &tree.root_view().children[0];
        let children = form
            .children
            .iter()
            .map(|child| child.span.slice(case.input))
            .collect::<Vec<_>>();
        assert_eq!(
            children, case.children,
            "{} (hy reads {})",
            case.input, case.hy_reads
        );
        for child in &form.children {
            assert!(
                !child.reader_prefixes.iter().any(|prefix| matches!(
                    prefix,
                    ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing
                )),
                "{}: a comma left an unquote prefix behind",
                case.input
            );
        }
    }
}

/// Dropping the comma does not soften Hy's other prefixes.
///
/// `'` and `` ` `` *are* Hy reader macros and each takes exactly one following
/// form, so a closing delimiter after one is still the refusal
/// `rejects_a_reader_prefix_before_a_closing_delimiter` established — this
/// change is not a reinstatement of the permissive behaviour that predated it.
#[test]
fn hy_still_refuses_its_real_prefixes_before_a_closing_delimiter() {
    for input in ["(a ')", "(a `)"] {
        assert_eq!(
            SyntaxTree::parse_with_dialect(input, Dialect::Hy).unwrap_err(),
            ParseError::MissingReaderForm(3),
            "input: {input}"
        );
    }
    let tree = SyntaxTree::parse_with_dialect("(a 'b)", Dialect::Hy).expect("valid");
    assert_eq!(
        tree.root_view().children[0].children[1].reader_prefixes,
        vec![ReaderPrefix::Quote]
    );
}

/// The comma keeps meaning unquote in every dialect whose reader gives it that
/// meaning, and keeps meaning whitespace in Clojure and Carp.
///
/// `classify_hy` is reached only from the `Dialect::Hy` arm of
/// `classify_reader_macro`, so it cannot drift by construction; the test pins
/// it anyway, because the arm it was split out of still serves `Unknown`.
///
/// Carp moved out of the unquote list after the fact. When this test was
/// written Carp still inherited the legacy reader, where `,` was a prefix --
/// but `,` is *whitespace* in Carp, and reading it as unquote gave `max` and
/// `val` in `[min, max, val]` phantom `Unquote` prefixes at 39 corpus sites.
/// The two changes were green independently and collided only once both were
/// on `main`: a semantic conflict git cannot see.
#[test]
fn hy_comma_arm_does_not_change_other_dialects() {
    for dialect in [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Racket,
        Dialect::Fennel,
        Dialect::Lfe,
        Dialect::Janet,
        Dialect::Unknown,
    ] {
        let input = "(a ,b)";
        let tree = SyntaxTree::parse_with_dialect(input, dialect)
            .unwrap_or_else(|error| panic!("{}: {error:?}", dialect.label()));
        let form = &tree.root_view().children[0];
        assert_eq!(
            form.children[1].reader_prefixes,
            vec![ReaderPrefix::Unquote],
            "{}",
            dialect.label()
        );
        // And a comma with nothing after it is still the refusal, so #100's
        // guarantee is untouched outside Hy.
        assert_eq!(
            SyntaxTree::parse_with_dialect("(a ,)", dialect).unwrap_err(),
            ParseError::MissingReaderForm(3),
            "{}",
            dialect.label()
        );
    }

    // Clojure and Carp both read a comma as whitespace, so `(a ,b)` is two data
    // there and `(a ,)` is one -- neither is a prefix, and neither is an error.
    // Carp's separators are written that way: `[min, max, val]`, `[x Int, y Int]`.
    for dialect in [Dialect::Clojure, Dialect::Carp] {
        let tree = SyntaxTree::parse_with_dialect("(a ,b)", dialect)
            .unwrap_or_else(|error| panic!("{}: {error:?}", dialect.label()));
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 2, "{}", dialect.label());
        assert!(
            form.children[1].reader_prefixes.is_empty(),
            "{}",
            dialect.label()
        );
        SyntaxTree::parse_with_dialect("(a ,)", dialect)
            .unwrap_or_else(|error| panic!("{}: comma is whitespace, {error:?}", dialect.label()));
    }
}

/// A stray closing delimiter with no prefix in front of it still reports the
/// delimiter, not a missing reader form — the prefix check added for
/// `rejects_a_reader_prefix_before_a_closing_delimiter` must not swallow the
/// pre-existing diagnostic.
#[test]
fn a_bare_stray_closing_delimiter_still_reports_itself() {
    assert_eq!(
        SyntaxTree::parse(")").unwrap_err(),
        ParseError::UnexpectedClose {
            delimiter: ')',
            position: 0
        }
    );
}

#[test]
fn rejects_reader_comments_without_a_form() {
    for input in ["#;", "#_"] {
        assert_eq!(
            SyntaxTree::parse(input).unwrap_err(),
            ParseError::MissingReaderForm(0),
            "input: {input}"
        );
    }
}

#[test]
fn rejects_unterminated_strings_inside_reader_comments() {
    for input in ["#;\"unterminated", "#_\"unterminated"] {
        assert_eq!(
            SyntaxTree::parse(input).unwrap_err(),
            ParseError::UnterminatedString(2),
            "input: {input}"
        );
    }
}

#[test]
fn skips_deeply_nested_reader_comment_forms_without_recursion() {
    const DEPTH: usize = 10_000;

    let nested_list = format!("#;{}ignored{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
    let tree = SyntaxTree::parse(&nested_list).expect("deep discarded list should parse");
    assert!(tree.root_children().is_empty());

    let nested_comments = format!("{}{}", "#;".repeat(DEPTH), "ignored ".repeat(DEPTH));
    let tree = SyntaxTree::parse(&nested_comments).expect("deep nested comments should parse");
    assert!(tree.root_children().is_empty());
}

#[test]
fn bounds_nested_reader_comment_frames() {
    let limit = MAX_DISCARDED_FORM_STACK_FRAMES;
    for reader_comment in ["#;", "#_"] {
        let below = format!(
            "{}{}",
            reader_comment.repeat(limit),
            "ignored ".repeat(limit)
        );
        let tree = SyntaxTree::parse(&below).expect("frame count at limit should parse");
        assert!(tree.root_children().is_empty());

        let above = format!(
            "{}{}",
            reader_comment.repeat(limit + 1),
            "ignored ".repeat(limit + 1)
        );
        assert!(matches!(
            SyntaxTree::parse(&above),
            Err(ParseError::ResourceLimitExceeded {
                limit: MAX_DISCARDED_FORM_STACK_FRAMES,
                ..
            })
        ));
    }
}

#[test]
fn bounds_discarded_list_frames() {
    let limit = MAX_DISCARDED_FORM_STACK_FRAMES;
    for reader_comment in ["#;", "#_"] {
        let below = format!(
            "{reader_comment}{}ignored{}",
            "(".repeat(limit - 1),
            ")".repeat(limit - 1)
        );
        SyntaxTree::parse(&below).expect("frame count at limit should parse");

        let above = format!(
            "{reader_comment}{}ignored{}",
            "(".repeat(limit),
            ")".repeat(limit)
        );
        assert!(matches!(
            SyntaxTree::parse(&above),
            Err(ParseError::ResourceLimitExceeded {
                limit: MAX_DISCARDED_FORM_STACK_FRAMES,
                ..
            })
        ));
    }
}

#[test]
fn bounds_discarded_feature_dispatch_frames() {
    let limit = MAX_DISCARDED_FORM_STACK_FRAMES;
    for reader_comment in ["#;", "#_"] {
        for feature_dispatch in ["#+", "#-"] {
            let below_depth = limit - 2;
            let below = format!(
                "{reader_comment}{}{feature_dispatch}feature guarded{}",
                "(".repeat(below_depth),
                ")".repeat(below_depth)
            );
            SyntaxTree::parse(&below).expect("frame count at limit should parse");

            let above_depth = limit - 1;
            let above = format!(
                "{reader_comment}{}{feature_dispatch}feature guarded{}",
                "(".repeat(above_depth),
                ")".repeat(above_depth)
            );
            assert!(matches!(
                SyntaxTree::parse(&above),
                Err(ParseError::ResourceLimitExceeded {
                    limit: MAX_DISCARDED_FORM_STACK_FRAMES,
                    ..
                })
            ));
        }
    }
}

#[test]
fn reader_comments_discard_complete_feature_conditionals() {
    for reader_comment in ["#;", "#_"] {
        for feature_dispatch in ["#+", "#-"] {
            let input = format!(
                "{reader_comment}{feature_dispatch}(and sbcl unix) (discarded foo) (live bar)"
            );
            let tree = SyntaxTree::parse(&input).expect("feature conditional should be discarded");

            assert_eq!(tree.root_children().len(), 1, "input: {input}");
            assert_eq!(
                tree.select_path(&parse_path("0"))
                    .expect("live form")
                    .text(),
                "(live bar)",
                "input: {input}"
            );
        }
    }
}

#[test]
fn nested_reader_comments_discard_complete_feature_conditionals() {
    let input = "#;#+sbcl #_#-unix (nested) (guarded) (live)";
    let tree = SyntaxTree::parse(input).expect("nested feature conditional should be discarded");

    assert_eq!(tree.root_children().len(), 1);
    assert_eq!(
        tree.select_path(&parse_path("0"))
            .expect("live form")
            .text(),
        "(live)"
    );
}

#[test]
fn incomplete_discarded_feature_conditionals_return_errors() {
    for input in ["#;#+", "#;#+sbcl", "#_#-", "#_#-(and sbcl"] {
        assert!(SyntaxTree::parse(input).is_err(), "input: {input}");
    }
}

#[test]
fn keeps_reader_eval_body_opaque_during_rename() {
    let input = "#.(foo foo) foo";
    let tree = SyntaxTree::parse(input).expect("valid");
    let output = tree.rename_symbol(
        &SymbolName::new("foo").unwrap(),
        &SymbolName::new("bar").unwrap(),
    );
    assert_eq!(output, "#.(foo foo) bar");
}

#[test]
fn skips_nested_common_lisp_block_comments() {
    let input = "(foo #| outer foo #| nested |# still outer |# bar) (foo baz)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(tree.root_children().len(), 2);
    let from = SymbolName::new("foo").expect("symbol");
    let to = SymbolName::new("bar").expect("symbol");
    assert_eq!(
        tree.rename_symbol(&from, &to),
        "(bar #| outer foo #| nested |# still outer |# bar) (bar baz)"
    );
}

#[test]
fn rejects_unterminated_common_lisp_block_comment() {
    assert_eq!(
        SyntaxTree::parse("#| outer #| nested |#").unwrap_err(),
        ParseError::UnterminatedBlockComment(0)
    );
}

#[test]
fn parses_character_literals_with_delimiter_values() {
    // `#\[`, `#\)`, and `#\]` are character literals, not structural delimiters.
    let input = "(write-char #\\[ stream) (list #\\) #\\] #\\()";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(tree.root_children().len(), 2);

    let first = tree.select_path(&parse_path("0")).expect("first").view();
    assert_eq!(first.children[1].text.as_deref(), Some("#\\["));

    let second = tree.select_path(&parse_path("1")).expect("second").view();
    assert_eq!(second.children[1].text.as_deref(), Some("#\\)"));
    assert_eq!(second.children[2].text.as_deref(), Some("#\\]"));
    assert_eq!(second.children[3].text.as_deref(), Some("#\\("));
}

#[test]
fn parses_named_and_whitespace_character_literals() {
    // Named characters keep their trailing constituents; `#\ ` escapes a space.
    let tree = SyntaxTree::parse("(char= c #\\Space #\\a)").expect("valid");
    let form = tree.select_path(&parse_path("0")).expect("form").view();
    assert_eq!(form.children[2].text.as_deref(), Some("#\\Space"));
    assert_eq!(form.children[3].text.as_deref(), Some("#\\a"));

    let space = SyntaxTree::parse("(x #\\ )").expect("valid");
    let form = space.select_path(&parse_path("0")).expect("form").view();
    assert_eq!(form.children[1].text.as_deref(), Some("#\\ "));
}

#[test]
fn parses_dialect_character_literals_with_closing_delimiters() {
    let cases = [
        (Dialect::Scheme, "(#\\))", "#\\)"),
        (Dialect::Clojure, "(\\))", "\\)"),
        (Dialect::EmacsLisp, "(?\\))", "?\\)"),
    ];

    for (dialect, input, expected) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("valid character literal");
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 1, "{}", dialect.label());
        assert_eq!(
            form.children[0].span.slice(input),
            expected,
            "{}",
            dialect.label()
        );
    }
}

#[test]
fn rejects_truncated_escaped_emacs_lisp_character_literal() {
    assert!(SyntaxTree::parse_with_dialect("?\\", Dialect::EmacsLisp).is_err());
}

#[test]
fn discarded_forms_use_the_same_dialect_character_literal_scanner() {
    let cases = [
        (Dialect::Scheme, "#;(#\\)) kept"),
        (Dialect::Clojure, "#_(\\)) kept"),
    ];

    for (dialect, input) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("valid discarded form");
        let root = tree.root_view();
        assert_eq!(root.children.len(), 1, "{}", dialect.label());
        assert_eq!(root.children[0].text.as_deref(), Some("kept"));
    }
}

#[test]
fn character_literal_does_not_break_rename() {
    let input = "(defun f () (write-char #\\[ out) (foo))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        tree.rename_symbol(
            &SymbolName::new("foo").unwrap(),
            &SymbolName::new("bar").unwrap(),
        ),
        "(defun f () (write-char #\\[ out) (bar))"
    );
}

#[test]
fn parses_pipe_escaped_symbol_with_embedded_space_as_one_atom() {
    // `|Foo Bar|` is a single multiple-escaped symbol (CLHS 2.1.4.2); the
    // embedded space must not act as a token boundary.
    let input = "(defun |Foo Bar| (x) (+ x 1))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let form = tree.select_path(&parse_path("0")).expect("form").view();
    assert_eq!(form.children[1].text.as_deref(), Some("|Foo Bar|"));
    assert_eq!(form.children[2].span.slice(input), "(x)");
}

#[test]
fn parses_pipe_escaped_symbol_with_nested_single_escape() {
    let tree = SyntaxTree::parse(r"|a\|b|").expect("valid");
    let atom = tree.select_path(&parse_path("0")).expect("atom").view();
    assert_eq!(atom.text.as_deref(), Some(r"|a\|b|"));
}

#[test]
fn rejects_unterminated_pipe_escaped_symbol() {
    assert_eq!(
        SyntaxTree::parse("(defun |Foo (x) (+ x 1))").unwrap_err(),
        ParseError::UnterminatedSymbol(7)
    );
}

#[test]
fn rejects_dangling_single_escapes() {
    for (input, position) in [("\\", 0), ("foo\\", 3)] {
        assert_eq!(
            SyntaxTree::parse(input).unwrap_err(),
            ParseError::DanglingSingleEscape(position),
            "input: {input}"
        );
    }
}

/// A feature conditional is one node, in both spellings and in every dialect
/// that reads one.
///
/// This test used to assert the opposite for `SyntaxTree::parse` — that is,
/// for `Dialect::Unknown` — and demanded `#+`, `sbcl` and the guarded datum as
/// *three siblings*. That expectation was not arbitrary: it predates
/// `ReaderMacro::MultiDatum`, and the goal it was written to serve was that
/// `#+sbcl` and `#+(and sbcl x86-64)` produce the same shape rather than one
/// gluing into an atom and the other splitting at the list delimiter.
///
/// Consuming the whole conditional as one opaque form serves that goal too,
/// and serves it better: the two spellings agree here, *and* they agree with
/// the ten named dialects, instead of `Dialect::Unknown` being the one reader
/// that disagrees with the other ten. The three-sibling shape was not merely
/// coarser — it claimed a file had more top-level forms than it has, and
/// `edit format` acted on that claim by putting blank lines between the
/// dispatch, the feature expression and the guarded form.
///
/// The cost is real and accepted: a feature symbol inside `#+sbcl` is no
/// longer separately findable or renameable under `Dialect::Unknown`. It
/// already was not under `Dialect::CommonLisp`, which is the dialect that
/// defines the syntax.
#[test]
fn feature_conditionals_are_one_node_in_every_dialect_that_reads_them() {
    let inputs = [
        (
            "(defun f () #+sbcl (declare (optimize speed)) 1)",
            "#+sbcl (declare (optimize speed))",
        ),
        (
            "(defun f () #+(and sbcl x86-64) (declare (optimize speed)) 1)",
            "#+(and sbcl x86-64) (declare (optimize speed))",
        ),
        (
            "(defun f () #-sbcl (declare (optimize speed)) 1)",
            "#-sbcl (declare (optimize speed))",
        ),
    ];

    for (input, conditional) in inputs {
        for dialect in [Dialect::Unknown, Dialect::CommonLisp] {
            let tree = SyntaxTree::parse_with_dialect(input, dialect)
                .unwrap_or_else(|error| panic!("{}: {input}: {error}", dialect.label()));
            let form = tree.select_path(&parse_path("0")).expect("form").view();

            // `defun`, `f`, `()`, the whole conditional, `1`.
            let children = form
                .children
                .iter()
                .map(|child| child.span.slice(input))
                .collect::<Vec<_>>();
            assert_eq!(
                children,
                vec!["defun", "f", "()", conditional, "1"],
                "{}: {input}",
                dialect.label()
            );
        }
    }
}

/// `Dialect::Unknown` and `Dialect::CommonLisp` agree, node for node.
///
/// The parity is the actual requirement; the shape assertions above are how it
/// is spelled. Comparing the two readers directly means a future change that
/// moves *both* still has to move them together.
#[test]
fn the_unknown_dialect_reads_feature_conditionals_exactly_as_common_lisp_does() {
    for input in [
        "#+sbcl (require :sb-posix)\n(defun f () 1)\n",
        "#-sbcl (require :posix)\n(defun f () 1)\n",
        "#+(and sbcl (not win32)) (defun posix-only () t)\n",
        "(a #+sbcl b c)",
        "(a #-sbcl b c)",
    ] {
        let unknown = SyntaxTree::parse_with_dialect(input, Dialect::Unknown)
            .unwrap_or_else(|error| panic!("Unknown: {input}: {error}"));
        let common_lisp = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp)
            .unwrap_or_else(|error| panic!("CommonLisp: {input}: {error}"));

        assert_eq!(
            unknown.root_view().children.len(),
            common_lisp.root_view().children.len(),
            "top-level form count diverged for {input:?}"
        );
        assert_eq!(
            shape(&unknown.root_view(), input),
            shape(&common_lisp.root_view(), input),
            "tree shape diverged for {input:?}"
        );
    }
}

/// A comparable rendering of a subtree: every node's kind, delimiter, reader
/// prefixes and source text, nested.
fn shape(view: &ExpressionView, source: &str) -> String {
    let children = view
        .children
        .iter()
        .map(|child| shape(child, source))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "({:?} {:?} {:?} {:?} [{children}])",
        view.kind,
        view.delimiter,
        view.reader_prefixes,
        view.span.slice(source),
    )
}

/// An incomplete feature conditional is refused, by the permissive reader as
/// well as by `Dialect::CommonLisp`. **The refusal is intended.**
///
/// This is an acceptance *narrowing*, and the narrowing is the point. Reading
/// `#+` as a token of its own let `Dialect::Unknown` accept every input below:
/// `printf '#+sbcl\n' | paredit inspect check` answered `"ok"` and exited `0`.
/// It could only do that by calling `#+` one top-level form and `sbcl` an
/// unrelated second one, which is not what the text means — CLHS 2.4.8.17
/// requires a feature expression *and* a datum after `#+`, and none of these
/// five supply the datum. `Dialect::CommonLisp` has always refused all five.
///
/// The alternative to refusing was not "accept harmlessly". Every command
/// shares this gate, so an accepted document is a document `edit format
/// --write` will rewrite on that reading — moving the dispatch, the feature
/// expression and the guarded form apart with blank lines between them, and
/// changing what the file compiles to. Silent corruption at exit `0` is worse
/// than a parse failure at exit `1`, which is loud, addressed at a byte
/// offset, and cannot be missed.
///
/// The narrowing applies only to input that was already invalid. A *complete*
/// conditional parses in both readers — see
/// `the_unknown_dialect_reads_feature_conditionals_exactly_as_common_lisp_does`
/// above — so no valid document lost acceptance here.
#[test]
fn incomplete_feature_conditionals_are_refused_by_both_readers() {
    // Every one of these parsed under `Dialect::Unknown` before the reader
    // stopped scanning `#+` as a standalone token.
    let newly_refused = [
        ("#+sbcl", ParseError::MissingReaderForm(0)),
        ("#-sbcl", ParseError::MissingReaderForm(0)),
        // Two dispatches, neither of which ever gets a feature expression:
        // the first claims `#-` as its feature expression, and then there is
        // nothing left to guard.
        ("#+ #-", ParseError::MissingReaderForm(3)),
        // A compound feature expression is no different: the list is the
        // feature expression, and the datum it would guard is absent.
        ("#+(and sbcl x86-64)", ParseError::MissingReaderForm(0)),
    ];

    for (input, expected) in newly_refused {
        for dialect in [Dialect::Unknown, Dialect::CommonLisp] {
            let error = SyntaxTree::parse_with_dialect(input, dialect).expect_err(&format!(
                "{}: {input:?} was accepted, so `inspect check` calls an incomplete \
                 conditional valid and `edit format --write` will rewrite it",
                dialect.label()
            ));
            assert_eq!(error, expected, "{}: {input:?}", dialect.label());
        }
    }
}

/// The in-list case, and a known limitation in how it is reported.
///
/// `(a #+sbcl)` is refused for the same reason as the cases above — `#+`
/// needs two data after it and only `sbcl` is available — but the diagnostic
/// blames the wrong token. The conditional takes `sbcl` as its feature
/// expression and then reads the `)` as its guarded datum, so the failure
/// surfaces as `unexpected closing delimiter ')' at byte 9` rather than as an
/// incomplete conditional at byte 3. Byte 9 really is the `)`; what is wrong
/// is the *cause* it names.
///
/// This is recorded, not fixed. It is pre-existing behaviour of
/// `Dialect::CommonLisp`, which has always refused this input with exactly
/// this message; the permissive reader joining it made the wart reachable
/// from one more dialect without making it any newer. Fixing it means
/// distinguishing "the payload scan hit a closing delimiter" from "the
/// document had a stray one", which is a change to `skip_form`'s error
/// reporting and not to what is accepted.
///
/// Pinned so that a future fix to the message is a deliberate edit here rather
/// than a silent change to what users are told.
#[test]
fn a_conditional_with_no_guarded_datum_inside_a_list_blames_the_closing_delimiter() {
    let input = "(a #+sbcl)";

    for dialect in [Dialect::Unknown, Dialect::CommonLisp] {
        let error = SyntaxTree::parse_with_dialect(input, dialect)
            .expect_err(&format!("{}: {input:?} was accepted", dialect.label()));

        assert_eq!(
            error,
            ParseError::UnexpectedClose {
                delimiter: ')',
                position: 9,
            },
            "{}",
            dialect.label()
        );
        // Known limitation: byte 3, where the `#+` starts, is the honest
        // answer and is not what the user sees.
        assert_eq!(
            error.to_string(),
            "unexpected closing delimiter ')' at byte 9",
            "{}",
            dialect.label()
        );
    }
}

#[test]
fn rejects_unbalanced_document() {
    assert_eq!(
        SyntaxTree::parse("(defun x").unwrap_err(),
        ParseError::UnclosedList(0)
    );
}

#[test]
fn repairs_unclosed_lists_using_matching_delimiters() {
    assert_eq!(
        SyntaxTree::repair_unclosed_lists("(outer [inner {leaf}").expect("repair"),
        "(outer [inner {leaf}])"
    );
}

#[test]
fn repair_unclosed_lists_leaves_balanced_input_unchanged() {
    assert_eq!(
        SyntaxTree::repair_unclosed_lists("(outer [inner])").expect("balanced input"),
        "(outer [inner])"
    );
}

#[test]
fn repair_unclosed_lists_rejects_other_parse_errors() {
    assert_eq!(
        SyntaxTree::repair_unclosed_lists("(alpha]").unwrap_err(),
        ParseError::MismatchedClose {
            found: ']',
            expected: ')',
            position: 6
        }
    );
}

#[test]
fn rejects_mismatched_delimiter() {
    assert_eq!(
        SyntaxTree::parse("(alpha]").unwrap_err(),
        ParseError::MismatchedClose {
            found: ']',
            expected: ')',
            position: 6
        }
    );
}

#[test]
fn a_lang_directive_is_read_as_a_line_directive_not_a_dispatch() {
    // Every real Racket file opens with one, and reading `#lang` as a reader
    // dispatch made all of them fail to parse.
    for dialect in [Dialect::Racket, Dialect::Scheme] {
        let tree = SyntaxTree::parse_with_dialect("#lang racket/base\n(define x 1)\n", dialect)
            .unwrap_or_else(|error| panic!("{dialect:?}: {error}"));
        let root = tree.root_view();

        assert_eq!(root.children.len(), 1, "{dialect:?}");
        assert_eq!(
            root.children[0]
                .children
                .first()
                .and_then(|head| head.text.as_deref()),
            Some("define"),
            "{dialect:?}"
        );
    }
}

#[test]
fn a_lang_directive_is_kept_as_trivia_rather_than_dropped() {
    // The directive names the language for the whole file, so a tool that
    // rewrites the source must not lose it. Reading it as a line comment puts
    // it in the leading trivia, ahead of the first form.
    let source = "#lang racket/base\n(define x 1)\n";
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::Racket).expect("parse");
    let first = tree
        .root_view()
        .children
        .first()
        .cloned()
        .expect("one form");

    assert_eq!(&source[..first.span.start().get()], "#lang racket/base\n");
}

// --- find_parse_errors: recovery and multiple reporting (Q6) ---

#[test]
fn find_parse_errors_is_empty_for_a_clean_document() {
    let errors = SyntaxTree::find_parse_errors("(defun add (x y) (+ x y))\n", Dialect::CommonLisp);
    assert!(errors.is_empty());
}

/// Two independent, self-contained errors — a stray closing delimiter right
/// after an otherwise-complete top-level form, so neither swallows anything
/// past itself — are both found, at their own absolute byte positions.
#[test]
fn find_parse_errors_reports_two_independent_errors_at_their_own_positions() {
    let source = "(foo))\n(bar))\n";
    let errors = SyntaxTree::find_parse_errors(source, Dialect::CommonLisp);

    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(errors[0].position(), 5, "{errors:?}"); // the stray ')' after "(foo)"
    assert_eq!(errors[1].position(), 12, "{errors:?}"); // the stray ')' after "(bar)"
    assert!(matches!(errors[0], ParseError::UnexpectedClose { .. }));
    assert!(matches!(errors[1], ParseError::UnexpectedClose { .. }));
}

/// A valid top-level form recovery skips over on its way to the next error
/// is not itself reported as a problem.
#[test]
fn find_parse_errors_does_not_report_the_valid_form_it_recovers_through() {
    let source = "(a))\n(ok 1)\n(b))\n";
    let errors = SyntaxTree::find_parse_errors(source, Dialect::CommonLisp);
    assert_eq!(errors.len(), 2, "{errors:?}");
}

/// No column-zero `(` follows the failure, so there is nothing to resync on
/// and this falls back to exactly what `parse_with_dialect` already
/// reports: one error.
#[test]
fn find_parse_errors_falls_back_to_one_error_with_no_resync_point() {
    let source = "(defun broken (x y";
    let errors = SyntaxTree::find_parse_errors(source, Dialect::CommonLisp);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].position(), 14); // the unclosed inner "(x y" list
}

/// A syntax error on every line of a large file is bounded work and a
/// bounded report, not one entry per line.
#[test]
fn find_parse_errors_is_capped_on_pathological_input() {
    let source = "(f))\n".repeat(60);
    let errors = SyntaxTree::find_parse_errors(&source, Dialect::CommonLisp);
    assert_eq!(errors.len(), 50, "{errors:?}");
}

/// Janet's `root` state sends every backtick to the `longstring` consumer
/// (`src/core/parse.c`), so a run of N backticks opens a string that the next
/// run of N backticks closes. Backtick is absent from Janet's `symchars`
/// table too, so it also ends whatever token preceded it.
///
/// Every expectation below was read off Janet 1.41.3's own reader before it
/// was written here: these are the values `janet -e '(pp (parse ...))'`
/// prints, not a restatement of what this parser happens to do.
#[test]
fn janet_long_strings_are_one_atom() {
    struct Case {
        input: &'static str,
        /// Source text of each child of the single top-level list.
        children: &'static [&'static str],
        /// What Janet's own reader makes of the literal, for the record.
        janet_value: &'static str,
    }

    let cases = [
        // A *single* backtick opens one. This is the case most likely to be
        // guessed wrong: the rule is not "two or more".
        Case {
            input: "(f `abc`)",
            children: &["f", "`abc`"],
            janet_value: r#""abc""#,
        },
        Case {
            input: "(f ``abc``)",
            children: &["f", "``abc``"],
            janet_value: r#""abc""#,
        },
        // The triple-backtick docstring, the dominant Janet idiom, holding
        // the bracket that used to unbalance the whole document.
        Case {
            input: "(f ```a [b, c) d``` tail)",
            children: &["f", "```a [b, c) d```", "tail"],
            janet_value: r#""a [b, c) d""#,
        },
        // A newline is an ordinary content byte; that is the entire point.
        Case {
            input: "(f ```line1\nline2```)",
            children: &["f", "```line1\nline2```"],
            janet_value: r#""line1\nline2""#,
        },
        // No escape processing at all: the `PFLAG_INSTRING` branch has no
        // `\\` case, so this is a backslash followed by an `n`.
        Case {
            input: "(f `a\\nb`)",
            children: &["f", "`a\\nb`"],
            janet_value: r#""a\\nb""#,
        },
        // A double quote inside is content, not a nested string.
        Case {
            input: "(f `say \"hi\"`)",
            children: &["f", "`say \"hi\"`"],
            janet_value: r#""say \"hi\"""#,
        },
        // Runs shorter than the opener are content, not a close.
        Case {
            input: "(f ```a`b```)",
            children: &["f", "```a`b```"],
            janet_value: r#""a`b""#,
        },
        Case {
            input: "(f ```a``b```)",
            children: &["f", "```a``b```"],
            janet_value: r#""a``b""#,
        },
        // The close is exactly N, not at least N. Janet returns 0 from
        // `stringend`, so the character that revealed the end is re-dispatched
        // and a longer run leaves its surplus to open the *next* datum: Janet
        // reads this as `"ab"` followed by `" x"`, two values.
        Case {
            input: "(f ```ab```` x`)",
            children: &["f", "```ab```", "` x`"],
            janet_value: r#""ab" then " x""#,
        },
        // Backtick is not a symbol character, so it ends the token before it
        // and opens a literal with no whitespace in between.
        Case {
            input: "(foo`bar`)",
            children: &["foo", "`bar`"],
            janet_value: r#"(foo "bar")"#,
        },
        Case {
            input: "(`bar`foo)",
            children: &["`bar`", "foo"],
            janet_value: r#"("bar" foo)"#,
        },
        // `@` before one is Janet's mutable buffer literal: the same lexical
        // extent with a different runtime type. `@` is already a reader prefix
        // here, so it stays glued to the literal instead of scanning loose.
        Case {
            input: "(f @```abc```)",
            children: &["f", "@```abc```"],
            janet_value: r#"@"abc""#,
        },
    ];

    for case in cases {
        let tree = SyntaxTree::parse_with_dialect(case.input, Dialect::Janet)
            .unwrap_or_else(|error| panic!("{}: {error}", case.input));
        let form = &tree.root_view().children[0];
        let children = form
            .children
            .iter()
            .map(|child| child.span.slice(case.input))
            .collect::<Vec<_>>();
        assert_eq!(
            children, case.children,
            "{} (janet reads {})",
            case.input, case.janet_value
        );
    }
}

/// An unterminated long string is refused, not read to EOF as one giant atom.
///
/// Janet refuses it too: `janet_parser_eof` finds the `longstring` state still
/// on the stack and reports "unexpected end of source". Reading the opener as
/// an atom instead would hand every later command a tree in which the rest of
/// the file is one enormous symbol -- silent corruption of exactly the kind
/// this arm exists to remove -- so it fails loudly.
///
/// Note the middle two cases: because the opener is the *whole* run of
/// backticks, ```` `` ```` is a two-backtick opener with no close rather than
/// an empty string, so an empty long string cannot be written at all. Janet
/// agrees, reporting "unexpected end of source, `` opened at line 1".
#[test]
fn janet_unterminated_long_string_is_refused() {
    for input in ["(f ```abc)", "(f ``)", "(f ````)", "(f `abc)"] {
        let error = SyntaxTree::parse_with_dialect(input, Dialect::Janet)
            .expect_err("an unterminated long string must not parse");
        assert!(
            matches!(error, ParseError::UnterminatedString(_)),
            "{input}: {error:?}"
        );
    }
}

/// The extent rule itself, tested on the one function both parser paths call.
///
/// The recording path (`atom_long_string_with_prefixes`) and the discarded-form
/// scanner (`skip_form`) each ask `long_string_extent` where the literal ends,
/// which is what stops them disagreeing. That sharing cannot be exercised
/// end-to-end today because Janet has no datum comment -- `#` is always a line
/// comment there, so nothing reaches `skip_form` under this dialect -- so the
/// shared decision is pinned directly instead.
#[test]
fn janet_long_string_extent_matches_janets_reader() {
    let policy = DialectReaderPolicy::new(Dialect::Janet);
    let closed = |width| Some(LongStringExtent::Closed { width });

    // Opener run length determines the required close.
    assert_eq!(policy.long_string_extent(b"`abc`", 0), closed(5));
    assert_eq!(policy.long_string_extent(b"``abc``", 0), closed(7));
    assert_eq!(policy.long_string_extent(b"```abc```", 0), closed(9));
    // Shorter interior runs are content.
    assert_eq!(policy.long_string_extent(b"```a`b```", 0), closed(9));
    assert_eq!(policy.long_string_extent(b"``a```b``", 0), closed(5));
    // Exactly N closes, surplus backticks are left for the next datum.
    assert_eq!(policy.long_string_extent(b"```ab```` x`", 0), closed(8));
    // Scanning starts at `pos`, not at 0.
    assert_eq!(policy.long_string_extent(b"(f `abc`)", 3), closed(5));
    // No close, and an all-backtick run, are both unterminated.
    assert_eq!(
        policy.long_string_extent(b"```abc", 0),
        Some(LongStringExtent::Unterminated)
    );
    assert_eq!(
        policy.long_string_extent(b"``", 0),
        Some(LongStringExtent::Unterminated)
    );
    // Not a long string at all.
    assert_eq!(policy.long_string_extent(b"abc", 0), None);
    assert_eq!(policy.long_string_extent(b"", 0), None);
    // And never one outside Janet.
    assert_eq!(
        DialectReaderPolicy::new(Dialect::CommonLisp).long_string_extent(b"`abc`", 0),
        None
    );
}

/// A backtick keeps meaning quasiquote everywhere else. Janet is the only
/// dialect that may change, and this pins the other nine so a later edit to
/// `has_long_strings` cannot quietly widen.
#[test]
fn backtick_is_still_quasiquote_outside_janet() {
    for dialect in [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Racket,
        Dialect::Clojure,
        Dialect::Fennel,
        Dialect::Lfe,
        Dialect::Hy,
        Dialect::Carp,
        Dialect::Unknown,
    ] {
        let input = "(f `(a b))";
        let tree = SyntaxTree::parse_with_dialect(input, dialect)
            .unwrap_or_else(|error| panic!("{}: {error}", dialect.label()));
        let form = &tree.root_view().children[0];
        let children = form
            .children
            .iter()
            .map(|child| child.span.slice(input))
            .collect::<Vec<_>>();
        assert_eq!(children, vec!["f", "`(a b)"], "{}", dialect.label());
        assert_eq!(
            form.children[1].reader_prefixes,
            vec![ReaderPrefix::Quasiquote],
            "{}",
            dialect.label()
        );
    }
}

/// Janet's `root` state lists `'` in the same `PFLAG_READERMAC` group as `,`
/// `;` `~` `|` (`src/core/parse.c`), and `popstate` expands it to the tuple
/// `(quote <form>)`. It was the one member of that group with no arm here, so
/// the quote glued onto whatever followed it.
///
/// Every expectation below was read off Janet 1.41.3's own reader before it
/// was written here: the `janet_value` column is what
/// `janet -e '(pp (parse ...))'` prints, not a restatement of what this parser
/// happens to do.
#[test]
fn janet_quote_is_a_reader_prefix() {
    struct Case {
        input: &'static str,
        /// Source text of each child of the single top-level list.
        children: &'static [&'static str],
        /// What Janet's own reader makes of the form, for the record.
        janet_value: &'static str,
    }

    let cases = [
        // The regression that found this: without a `'` arm the quote glued
        // onto the opening `"` of the string after it and the whole document
        // failed with "unterminated string starting at byte 6". This is
        // `spork/spork/cjanet.janet:31` reduced.
        Case {
            input: r#"(a '" " b)"#,
            children: &["a", r#"'" ""#, "b"],
            janet_value: r#"(a (quote " ") b)"#,
        },
        Case {
            input: "(a 'foo b)",
            children: &["a", "'foo", "b"],
            janet_value: "(a (quote foo) b)",
        },
        Case {
            input: "(a '(b c))",
            children: &["a", "'(b c)"],
            janet_value: "(a (quote (b c)))",
        },
        // `#` opens a *line comment* in Janet, so `'` before a brace is the
        // other half of `janet/test/suite-peg.janet:405`: the `{` was being
        // read as an opening brace of a struct that never closed.
        Case {
            input: r#"(a '"{" b)"#,
            children: &["a", r#"'"{""#, "b"],
            janet_value: r#"(a (quote "{") b)"#,
        },
        // Stacked prefixes nest, they do not collapse.
        Case {
            input: "(a ''b)",
            children: &["a", "''b"],
            janet_value: "(a (quote (quote b)))",
        },
        // A quote in front of the other Janet reader forms.
        Case {
            input: "(a '@{})",
            children: &["a", "'@{}"],
            janet_value: "(a (quote @{}))",
        },
        Case {
            input: "(a '`long`)",
            children: &["a", "'`long`"],
            janet_value: r#"(a (quote "long"))"#,
        },
        Case {
            input: "(a '[b c])",
            children: &["a", "'[b c]"],
            janet_value: "(a (quote [b c]))",
        },
    ];

    for case in cases {
        let tree = SyntaxTree::parse_with_dialect(case.input, Dialect::Janet)
            .unwrap_or_else(|error| panic!("{}: {error}", case.input));
        let form = &tree.root_view().children[0];
        let children = form
            .children
            .iter()
            .map(|child| child.span.slice(case.input))
            .collect::<Vec<_>>();
        assert_eq!(
            children, case.children,
            "{} (janet reads {})",
            case.input, case.janet_value
        );
    }

    // The prefix is recorded as `Quote`, not merely swallowed into the span.
    let tree = SyntaxTree::parse_with_dialect("(a 'foo)", Dialect::Janet).expect("valid");
    assert_eq!(
        tree.root_view().children[0].children[1].reader_prefixes,
        vec![ReaderPrefix::Quote]
    );
}

/// A dangling `'` at end of input is a truncated form, not the symbol `'`.
///
/// Janet agrees: `janet -e "(pp (parse-all \"'\"))"` fails with "unexpected end
/// of source, opened at line 1, column 1". Before the `'` arm existed paredit
/// accepted it as a one-character atom.
#[test]
fn janet_dangling_quote_is_refused() {
    let error = SyntaxTree::parse_with_dialect("'", Dialect::Janet)
        .expect_err("a quote with nothing after it is not a form");
    assert!(
        matches!(error, ParseError::MissingReaderForm(0)),
        "{error:?}"
    );

    // `(a ')` is now refused too, which is the change this stated expectation
    // was written to force. `form` used to accumulate prefixes, find a closing
    // delimiter rather than a datum, and call `close_list` without ever
    // consuming them -- so the quote was dropped and the document parsed clean.
    // Janet refuses the same input ("mismatched delimiter )"), and so does
    // every other dialect here now; the defect was cross-dialect and
    // pre-existing, and `skip_form` -- the scanning twin of `form` -- had
    // already been refusing this shape, so the two paths now agree.
    //
    // See `rejects_a_reader_prefix_before_a_closing_delimiter` for the full
    // eight-dialect matrix and the `(a 'b)` control.
    let error = SyntaxTree::parse_with_dialect("(a ')", Dialect::Janet)
        .expect_err("a prefix with a closing delimiter after it is not a form");
    assert!(
        matches!(error, ParseError::MissingReaderForm(3)),
        "{error:?}"
    );
}

/// `'` keeps meaning quote everywhere else, and this pins the other dialects so
/// a later edit to `classify_janet` cannot quietly widen. Fennel already had
/// its own `'` arm; the legacy reader and the named dialects share
/// `classify_quote_prefix`.
#[test]
fn janet_quote_arm_does_not_change_other_dialects() {
    for dialect in [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Racket,
        Dialect::Clojure,
        Dialect::Fennel,
        Dialect::Lfe,
        Dialect::Hy,
        Dialect::Carp,
        Dialect::Unknown,
    ] {
        let input = "(f '(a b))";
        let tree = SyntaxTree::parse_with_dialect(input, dialect)
            .unwrap_or_else(|error| panic!("{}: {error}", dialect.label()));
        let form = &tree.root_view().children[0];
        let children = form
            .children
            .iter()
            .map(|child| child.span.slice(input))
            .collect::<Vec<_>>();
        assert_eq!(children, vec!["f", "'(a b)"], "{}", dialect.label());
        assert_eq!(
            form.children[1].reader_prefixes,
            vec![ReaderPrefix::Quote],
            "{}",
            dialect.label()
        );
    }
}

/// Clojure's `NamespaceMapReader` reads the namespace with a full `read`, then
/// skips `isWhitespace` before demanding the `{`, so `#:foo {:a 1}` is as legal
/// as `#:foo{:a 1}`. Requiring the brace to touch the namespace made the spaced
/// spelling an unsupported dispatch, which fails the whole parse -- so one such
/// literal silently dropped its entire file from every lint run.
///
/// The read/throw split below is taken from Clojure's own reader test suite,
/// `test/clojure/test_clojure/reader.cljc` lines 745-771.
#[test]
fn clojure_namespaced_maps_allow_whitespace_before_the_brace() {
    // Asserted equal to their tight-brace spellings by reader.cljc:745-752.
    let must_read = [
        "#:a{1 nil, :b nil}",
        "#:a {1 nil, :b nil}",
        "#::{1 nil, :a nil}",
        // reader.cljc:749 uses *two* spaces, so the skip is a loop.
        "#::  {1 nil, :a nil}",
        "#::s{1 nil, :a nil}",
        "#::s  {1 nil, :a nil}",
        // `isWhitespace` in LispReader counts a comma, and so does this.
        "#:a,{1 1}",
        "#:a\n{1 1}",
    ];
    for input in must_read {
        SyntaxTree::parse_with_dialect(input, Dialect::Clojure)
            .unwrap_or_else(|error| panic!("{input:?}: {error}"));
    }

    // Refused by Clojure, and still refused here. `#: s{:a 1}` is
    // reader.cljc:764 ("Namespaced map must specify a namespace"); the comment
    // case is refused because LispReader's loop skips `isWhitespace` only, so
    // a `;` is its "must specify a map" error rather than trivia.
    let must_refuse = [
        "#:::",
        "#: {:a 1}",
        "#:{:a 1}",
        "#:a b",
        "#:a",
        "#:a ;; c\n{:a 1}",
    ];
    for input in must_refuse {
        SyntaxTree::parse_with_dialect(input, Dialect::Clojure)
            .expect_err(&format!("{input:?} is not a namespaced map"));
    }
}

/// The dispatch width covers `#:ns` alone, never the brace, and the whitespace
/// between them stays trivia. That is what keeps the spaced spelling formatting
/// back to itself, and it is the property the width return value encodes.
#[test]
fn clojure_namespaced_map_width_stops_at_the_namespace() {
    let policy = DialectReaderPolicy::new(Dialect::Clojure);
    let width = |input: &str| policy.clojure_namespaced_map_width(input.as_bytes(), 0);

    assert_eq!(width("#:foo{:a 1}"), Some(5));
    assert_eq!(width("#:foo {:a 1}"), Some(5));
    assert_eq!(width("#:foo   {:a 1}"), Some(5));
    assert_eq!(width("#::foo{:a 1}"), Some(6));
    assert_eq!(width("#::foo  {:a 1}"), Some(6));
    assert_eq!(width("#::{:a 1}"), Some(3));
    assert_eq!(width("#:: {:a 1}"), Some(3));
    // No namespace, and no auto-resolve marker to excuse it.
    assert_eq!(width("#:{:a 1}"), None);
    assert_eq!(width("#: {:a 1}"), None);
    // A namespace with no map after it.
    assert_eq!(width("#:foo bar"), None);
    assert_eq!(width("#:foo"), None);
    // Comments are not whitespace to Clojure's reader here.
    assert_eq!(width("#:foo ;; c\n{:a 1}"), None);
}

// ---------------------------------------------------------------------------
// LFE reader
//
// The reference is LFE 2.2.0's own scanner, `src/lfe_scan.erl`, and its
// grammar, `src/lfe_parse.spell1`. Every expected reading below is that
// scanner's token stream, checked by running the case through
// `lfe_scan:string/1` -- not a guess at what the notation ought to mean.
// ---------------------------------------------------------------------------

/// The source text of every child of the document's single top-level list.
fn lfe_children(input: &'static str) -> Vec<&'static str> {
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Lfe)
        .unwrap_or_else(|error| panic!("{input:?}: {error}"));
    tree.root_view().children[0]
        .children
        .iter()
        .map(|child| child.span.slice(input))
        .collect()
}

/// `#B(`, `#M(` and `#S(` are single *opening* tokens in `scan_hash2`:
///
/// ```erlang
/// scan_hash2([C,$\(|Cs], Line, Col, [], St) when (C =:= $b) or (C =:= $B) ->
///     {ok,{'#B(',Line},Cs,Line,Col,St};
/// ```
///
/// closed by a plain `)`, since `lfe_parse.spell1` reads
/// `sexpr -> '#B(' proper_list ')'`. Taking the two-byte dispatch as a prefix
/// on the following list gives exactly that shape.
///
/// Before this, the `#B` scanned as its own atom and the list became its
/// *sibling*, so `(f #B(1 2) X)` had four children where LFE sees three --
/// silently, at exit 0, which made every arity-sensitive rule read the form
/// wrong.
#[test]
fn lfe_hash_letter_collections_stay_attached_to_their_list() {
    struct Case {
        input: &'static str,
        children: &'static [&'static str],
        prefix: ReaderPrefix,
    }

    let cases = [
        Case {
            input: "(f #B(1 2) X)",
            children: &["f", "#B(1 2)", "X"],
            prefix: ReaderPrefix::LfeBinary,
        },
        // The scanner accepts either case, and the document keeps its own.
        Case {
            input: "(f #b(1 2) X)",
            children: &["f", "#b(1 2)", "X"],
            prefix: ReaderPrefix::LfeBinary,
        },
        Case {
            input: "(g #M(a 1 b 2) Y)",
            children: &["g", "#M(a 1 b 2)", "Y"],
            prefix: ReaderPrefix::LfeMap,
        },
        Case {
            input: "(g #m(a 1) Y)",
            children: &["g", "#m(a 1)", "Y"],
            prefix: ReaderPrefix::LfeMap,
        },
        // `lfe_scan` emits `'#S('` and `lfe_parse.spell1` declares it a
        // terminal, but 2.2.0 has no production using it, so LFE itself
        // answers `{illegal,'#S('}`. Lexing it anyway beats orphaning the `#S`
        // from its list in a file that is already broken.
        Case {
            input: "(h #S(point x 1) Z)",
            children: &["h", "#S(point x 1)", "Z"],
            prefix: ReaderPrefix::LfeStruct,
        },
        // The tuple opener, which has always worked, pinned so the new arms
        // cannot displace it.
        Case {
            input: "(i #(1 2) W)",
            children: &["i", "#(1 2)", "W"],
            prefix: ReaderPrefix::HashLiteral,
        },
    ];

    for case in cases {
        assert_eq!(lfe_children(case.input), case.children, "{}", case.input);
        let tree = SyntaxTree::parse_with_dialect(case.input, Dialect::Lfe).expect("valid");
        let literal = &tree.root_view().children[0].children[1];
        assert_eq!(literal.kind, ExpressionKind::List, "{}", case.input);
        assert_eq!(literal.reader_prefixes, vec![case.prefix], "{}", case.input);
    }
}

/// `#b` and its friends only open a collection when a `(` follows.
/// `scan_hash2` orders the binary-token clause before the based-number one for
/// exactly this reason ("Scan binary tokens, these must come before the based
/// number"), so `#b1010` stays the number ten.
#[test]
fn lfe_based_numbers_are_not_collection_openers() {
    for input in [
        "(f #b1010 x)",
        "(f #B1010 x)",
        "(f #x1f x)",
        "(f #o17 x)",
        "(f #d99 x)",
        "(f #2r1010 x)",
        "(f #*1010 x)",
    ] {
        let children = lfe_children(input);
        assert_eq!(children.len(), 3, "{input}");
        assert_eq!(children[0], "f", "{input}");
        assert_eq!(children[2], "x", "{input}");
    }
}

/// `scan_hash1([$"|Cs], Line, Col, [], St) -> scan_binary_string(...)` makes
/// `#"..."` a single `binary` token.
///
/// Before this the `#` glued onto the string's *first word* and every later
/// word became a sibling atom, so `#"text/plain; version=0.0.4"` split into
/// three and one containing a `)` closed its enclosing list early. Binary
/// strings are the most common of these constructs in real LFE.
#[test]
fn lfe_binary_strings_are_one_atom() {
    let cases: &[(&str, &[&str])] = &[
        ("(f #\"GET\" x)", &["f", "#\"GET\"", "x"]),
        ("(f #\"a b\" x)", &["f", "#\"a b\"", "x"]),
        ("(f #\"x)y\" x)", &["f", "#\"x)y\"", "x"]),
        ("(f #\"\" x)", &["f", "#\"\"", "x"]),
        (
            "(f #\"text/plain; version=0.0.4\" x)",
            &["f", "#\"text/plain; version=0.0.4\"", "x"],
        ),
        ("(f #\"esc \\\" q\" x)", &["f", "#\"esc \\\" q\"", "x"]),
    ];

    for (input, expected) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Lfe)
            .unwrap_or_else(|error| panic!("{input:?}: {error}"));
        let children: Vec<&str> = tree.root_view().children[0]
            .children
            .iter()
            .map(|child| child.span.slice(input))
            .collect();
        assert_eq!(&children, expected, "{input}");
    }
}

/// LFE's whole character-literal grammar is one clause:
///
/// ```erlang
/// scan_hash2([$\\,C|Cs], Line, Col, [], St) ->
///     {ok,{number,Line,C},Cs,Line,Col+2,St};
/// ```
///
/// Two bytes of prefix and exactly one character, taken verbatim. There are no
/// named characters and no escape processing at all, so `#\newline` is the
/// letter `n` followed by the symbol `ewline` -- `lfe_scan:string/1` answers
/// `[{number,1,110},{symbol,1,ewline}]` for it.
#[test]
fn lfe_character_literal_is_exactly_one_character() {
    let cases: &[(&str, &[&str])] = &[
        // The delimiters. These used to restructure the tree outright: `#\(`
        // scanned as the atom `#\` and then *opened a list*.
        ("(list #\\( #\\))", &["list", "#\\(", "#\\)"]),
        // A `;` would otherwise start a comment and eat the rest of the line.
        ("(list #\\; a)", &["list", "#\\;", "a"]),
        // A `"` would otherwise open a string and swallow the file.
        ("(list #\\\" a)", &["list", "#\\\"", "a"]),
        ("(list #\\\\ a)", &["list", "#\\\\", "a"]),
        ("(list #\\| a)", &["list", "#\\|", "a"]),
        ("(list #\\a #\\b)", &["list", "#\\a", "#\\b"]),
        // One character means one character: the rest is a separate symbol.
        ("(f #\\newline)", &["f", "#\\n", "ewline"]),
        // A multi-byte character is one character, not one byte.
        ("(f #\\\u{e9} x)", &["f", "#\\\u{e9}", "x"]),
    ];

    for (input, expected) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Lfe)
            .unwrap_or_else(|error| panic!("{input:?}: {error}"));
        let children: Vec<&str> = tree.root_view().children[0]
            .children
            .iter()
            .map(|child| child.span.slice(input))
            .collect();
        assert_eq!(&children, expected, "{input}");
    }
}

/// `start_symbol_char($|) -> false` sends a *leading* `|` to `scan_qsymbol`,
/// which runs to the closing `|` taking `\C` verbatim; whitespace and
/// delimiters inside are ordinary content. But `symbol_char/1` has no `$|`
/// clause, so it falls through to `(C > $\s) and (C =< $~)` -- true for `|`
/// (124) -- and a `|` *inside* a token is an ordinary constituent.
///
/// Both halves matter. Without the first, `'|foo bar|` split at the space;
/// with the first but not the second, `a|b c|d` would fuse into one symbol.
#[test]
fn lfe_bar_quoted_symbols_open_only_at_a_token_start() {
    let cases: &[(&str, &[&str])] = &[
        ("(f '|foo bar|)", &["f", "'|foo bar|"]),
        ("(f |a(b| x)", &["f", "|a(b|", "x"]),
        ("(f |x\\|y| x)", &["f", "|x\\|y|", "x"]),
        ("(f || x)", &["f", "||", "x"]),
        ("(f |;| x)", &["f", "|;|", "x"]),
        // A newline inside is content: `scan_qsymbol1` has an explicit `$\n`
        // clause that keeps collecting.
        ("(f |multi\nline| x)", &["f", "|multi\nline|", "x"]),
        // Mid-token, an ordinary constituent. Two symbols, not one.
        ("(f a|b c|d)", &["f", "a|b", "c|d"]),
    ];

    for (input, expected) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Lfe)
            .unwrap_or_else(|error| panic!("{input:?}: {error}"));
        let children: Vec<&str> = tree.root_view().children[0]
            .children
            .iter()
            .map(|child| child.span.slice(input))
            .collect();
        assert_eq!(&children, expected, "{input}");
    }
}

/// An unterminated `|` fails loudly rather than consuming the rest of the file
/// as one symbol.
///
/// LFE refuses it too -- `scan_qsymbol1(eof, ...)` raises
/// `{illegal_chars,[$| | Symcs]}` -- so this agrees with the reference reader
/// rather than inventing a rule. Reading to EOF as one atom would be exactly
/// the silent corruption this work exists to remove.
#[test]
fn lfe_unterminated_bar_quoted_symbol_is_refused() {
    let error = SyntaxTree::parse_with_dialect("(f |unterminated\n(g 1)\n", Dialect::Lfe)
        .expect_err("unterminated multiple escape");
    assert_eq!(error, ParseError::UnterminatedSymbol(3));
}

/// `#\` with nothing after it is a truncated literal, not the character for
/// nothing. LFE answers `{illegal_token,"#\\"}`.
///
/// Accepting it as a complete atom would make the formatter non-idempotent:
/// it appends a trailing newline, the truncated literal claims it as its
/// character, and the next pass appends another. Scheme and Racket already
/// refuse the same input for the same reason.
#[test]
fn lfe_truncated_character_literal_is_refused() {
    let error = SyntaxTree::parse_with_dialect("(f #\\", Dialect::Lfe)
        .expect_err("truncated character literal");
    assert_eq!(
        error,
        ParseError::UnsupportedReaderDispatch {
            dispatch: "#".to_owned(),
            position: 3,
        }
    );
}

/// Everything above is LFE-only.
///
/// `#(` is a vector in Scheme and Racket and a set or lambda in Clojure, so
/// these bytes are live elsewhere with other meanings; a letter between the
/// `#` and the `(` means something different or nothing in all ten. This pins
/// them against a later edit widening `classify_lfe`'s arms by accident.
#[test]
fn lfe_hash_letter_collections_do_not_leak_into_other_dialects() {
    for dialect in [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Racket,
        Dialect::Clojure,
        Dialect::Fennel,
        Dialect::Janet,
        Dialect::Hy,
        Dialect::Carp,
        Dialect::Unknown,
    ] {
        let label = dialect.label();
        // Refusing the input is fine; what must not happen is reading it as
        // one prefixed list the way LFE now does.
        let Ok(tree) = SyntaxTree::parse_with_dialect("(f #B(1 2) X)", dialect) else {
            continue;
        };
        let prefixes: Vec<ReaderPrefix> = tree.root_view().children[0]
            .children
            .iter()
            .flat_map(|child| child.reader_prefixes.clone())
            .collect();
        assert!(
            !prefixes.contains(&ReaderPrefix::LfeBinary),
            "{label} produced an LFE binary literal"
        );
    }
}

/// The two lexical switches this change added, pinned per dialect.
///
/// The table is worth more than the prose: `bar_quoting` replaced a boolean,
/// and its mapping has to stay exactly what that boolean was for the ten
/// dialects LFE is not.
#[test]
fn lfe_lexical_switches_are_scoped_to_lfe() {
    use crate::sexpr::reader_policy::BarQuoting;

    let expected = [
        (Dialect::CommonLisp, BarQuoting::Anywhere, false),
        (Dialect::EmacsLisp, BarQuoting::None, false),
        (Dialect::Lfe, BarQuoting::TokenStart, true),
        (Dialect::Scheme, BarQuoting::Anywhere, false),
        (Dialect::Racket, BarQuoting::Anywhere, false),
        (Dialect::Clojure, BarQuoting::None, false),
        (Dialect::Hy, BarQuoting::None, false),
        (Dialect::Carp, BarQuoting::None, false),
        (Dialect::Janet, BarQuoting::None, false),
        (Dialect::Fennel, BarQuoting::None, false),
        (Dialect::Unknown, BarQuoting::Anywhere, false),
    ];
    assert_eq!(
        expected.len(),
        Dialect::ALL.len(),
        "a dialect was added without a decision here"
    );

    for (dialect, bar, one_char) in expected {
        let policy = DialectReaderPolicy::new(dialect);
        assert_eq!(policy.bar_quoting(), bar, "{}", dialect.label());
        assert_eq!(
            policy.character_literal_is_exactly_one_char(),
            one_char,
            "{}",
            dialect.label()
        );
    }
}

/// The four Hy reader defects, each pinned by the shape it produced before.
///
/// Every expectation here was checked against Hy 1.3.1's own reader
/// (`hy.reader.read_many`) rather than derived from the documentation.
#[test]
fn hy_reads_its_own_string_and_unquote_syntax() {
    struct Case {
        input: &'static str,
        children: &'static [&'static str],
    }

    let cases = [
        // f-strings. Before, `f"hi {name}"` scanned as the token `f"hi`, a
        // brace list, then an unterminated string, and the file did not parse.
        Case {
            input: r#"(print f"hi {name}")"#,
            children: &["print", r#"f"hi {name}""#],
        },
        // Arbitrary Hy code lives in the braces -- `read_fcomponent` calls
        // `parse_one_form` -- but the literal stays one opaque atom.
        Case {
            input: r#"(print f"{(+ 1 2)}")"#,
            children: &["print", r#"f"{(+ 1 2)}""#],
        },
        // A nested string inside a field holds the outer closing quote
        // hostage. This is why the scanner is a sub-reader rather than a
        // search for the next `"`.
        Case {
            input: r#"(print f"{(str "}")}")"#,
            children: &["print", r#"f"{(str "}")}""#],
        },
        // A nested f-string, and a `;` comment inside a field.
        Case {
            input: "(print f\"{f\"{x}\"}\")",
            children: &["print", "f\"{f\"{x}\"}\""],
        },
        Case {
            input: "(print f\"{(+ 1 ; c\n 2)}\")",
            children: &["print", "f\"{(+ 1 ; c\n 2)}\""],
        },
        // Doubled braces are literal text, not fields.
        Case {
            input: r#"(print f"{{literal}}")"#,
            children: &["print", r#"f"{{literal}}""#],
        },
        // Every other valid prefix. `r"a)b"` was the widest-reaching case: the
        // atom scanner ran past the quote and stopped at the `)` *inside* the
        // literal, reporting a stray closing delimiter.
        Case {
            input: r#"(re.match r"a)b" s)"#,
            children: &["re.match", r#"r"a)b""#, "s"],
        },
        Case {
            input: r#"(f b"x" rb"y" br"z" t"{q}" rf"w" fr"v" rt"u")"#,
            children: &[
                "f",
                r#"b"x""#,
                r#"rb"y""#,
                r#"br"z""#,
                r#"t"{q}""#,
                r#"rf"w""#,
                r#"fr"v""#,
                r#"rt"u""#,
            ],
        },
        // Bracket strings. The dangerous one: this used to yield a real `defn`
        // node inside what is actually a raw string.
        Case {
            input: "(setv x #[[(defn evil [] 1)]])",
            children: &["setv", "x", "#[[(defn evil [] 1)]]"],
        },
        Case {
            input: "(setv x #[delim[hello]delim])",
            children: &["setv", "x", "#[delim[hello]delim]"],
        },
        // The close is `]` + delim + `]`, so a bare `]]` inside is content.
        Case {
            input: "(setv x #[d[a]]b]d])",
            children: &["setv", "x", "#[d[a]]b]d]"],
        },
        // Unbalanced delimiters inside are just bytes.
        Case {
            input: "(setv x #[[ ) ) ) ]])",
            children: &["setv", "x", "#[[ ) ) ) ]]"],
        },
        // A bracket string whose delimiter is `f` or starts `f-` interpolates.
        Case {
            input: "(setv x #[f-q[{(+ 1 2)}]f-q])",
            children: &["setv", "x", "#[f-q[{(+ 1 2)}]f-q]"],
        },
        // `#(` and `#{` keep their existing reading: Hy's tuple and set
        // literals really do contain forms, unlike `#[`.
        Case {
            input: "(f #(1 2) #{3 4})",
            children: &["f", "#(1 2)", "#{3 4}"],
        },
    ];

    for case in cases {
        let tree = SyntaxTree::parse_with_dialect(case.input, Dialect::Hy)
            .unwrap_or_else(|error| panic!("{}: {error}", case.input));
        let form = &tree.root_view().children[0];
        let children = form
            .children
            .iter()
            .map(|child| child.span.slice(case.input))
            .collect::<Vec<_>>();
        assert_eq!(children, case.children, "{}", case.input);
    }
}

/// A Hy shebang is trivia, and only at offset 0.
///
/// `HyReader.parse` peeks the first two characters of the stream, so
/// `\n#!/usr/bin/env hy` stays the "reader macro is not defined" error it has
/// always been. Reading it as a line comment rather than stripping the line
/// keeps every later byte offset unchanged.
#[test]
fn hy_shebang_is_trivia_only_at_offset_zero() {
    let input = "#!/usr/bin/env hy\n(print 1)\n";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Hy).expect("valid");
    let roots = tree
        .root_view()
        .children
        .iter()
        .map(|child| child.span.slice(input))
        .collect::<Vec<_>>();
    assert_eq!(roots, vec!["(print 1)"]);

    // Offset 0 only. A `#!` on a later line stays an ordinary dispatch, and
    // the two junk atoms it produces are the pre-existing reading.
    let later = "(print 1)\n#!/usr/bin/env hy\n";
    let tree = SyntaxTree::parse_with_dialect(later, Dialect::Hy).expect("parses");
    assert_eq!(tree.root_view().children.len(), 3, "{later:?}");
}

/// Hy's `~` is deliberately still a bare atom, not a reader prefix.
///
/// This pins the *known-wrong* reading on purpose, so that whoever makes `~` a
/// prefix has to come here and read why it was held back. `classify_hy`
/// carries the reasoning: several formatter paths open a child list without
/// writing its reader prefixes, so promoting `~` deletes it from the output.
/// Measured over 2825 real files, that newly changed the meaning of 14 files
/// `edit format` had previously handled correctly.
///
/// The fix belongs with the formatter fix, not before it.
#[test]
fn hy_unquote_is_not_yet_a_reader_prefix() {
    let input = "`(foo ~bar ~@baz)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Hy).expect("valid");
    let form = &tree.root_view().children[0];
    assert_eq!(form.reader_prefixes, vec![ReaderPrefix::Quasiquote]);
    let children = form
        .children
        .iter()
        .map(|child| (child.span.slice(input), child.reader_prefixes.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        children,
        vec![("foo", vec![]), ("~bar", vec![]), ("~@baz", vec![])]
    );

    // Clojure's `~`, which really is a prefix, must be unaffected either way.
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("valid");
    let prefixes = tree.root_view().children[0]
        .children
        .iter()
        .map(|child| child.reader_prefixes.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        prefixes,
        vec![
            vec![],
            vec![ReaderPrefix::Unquote],
            vec![ReaderPrefix::UnquoteSplicing],
        ]
    );
}

/// An unterminated Hy literal fails loudly rather than swallowing the file.
///
/// Reading to EOF as one atom is the silent corruption this fix removes: every
/// later command would be handed a tree in which the rest of the document is
/// one giant symbol. Hy refuses all of these too.
#[test]
fn hy_unterminated_literals_are_refused() {
    for input in [
        r#"(print f"abc"#,
        r#"(print f"{(+ 1 2"#,
        "(setv x #[[abc)",
        "(setv x #[delim[abc]nope])",
        r#"(print r"abc"#,
    ] {
        let error = SyntaxTree::parse_with_dialect(input, Dialect::Hy)
            .expect_err("an unterminated literal must be refused");
        assert!(
            matches!(
                error,
                ParseError::UnterminatedString(_) | ParseError::UnexpectedClose { .. }
            ),
            "{input:?}: {error}"
        );
    }

    // The two bytes Hy names explicitly become delimiter errors, since that is
    // what they are: a `]` in a bracket string's delimiter, and an undoubled
    // `}` in f-string literal text.
    assert!(matches!(
        SyntaxTree::parse_with_dialect("(setv x #[a]b[y]a]b])", Dialect::Hy),
        Err(ParseError::UnexpectedClose { delimiter: ']', .. })
    ));
    assert!(matches!(
        SyntaxTree::parse_with_dialect(r#"(print f"}")"#, Dialect::Hy),
        Err(ParseError::UnexpectedClose { delimiter: '}', .. })
    ));
}

/// The prefix table, at the boundaries `prefixed_string` actually enforces.
#[test]
fn hy_string_prefix_width_matches_hys_validation() {
    let policy = DialectReaderPolicy::new(Dialect::Hy);
    // Distinct characters, a proper subset of `bfrt`, at most one of `b`/`f`/`t`.
    for good in ["r", "b", "f", "t", "rb", "br", "rf", "fr", "rt", "tr"] {
        let source = format!("{good}\"x\"");
        assert_eq!(
            policy.hy_string_prefix_width(source.as_bytes(), 0),
            Some(good.len()),
            "{good}"
        );
    }
    // Two of `b`/`f`/`t`, a repeated character, or not a prefix at all.
    for bad in ["bf", "ff", "bt", "q", "xf", "foo"] {
        let source = format!("{bad}\"x\"");
        assert_eq!(
            policy.hy_string_prefix_width(source.as_bytes(), 0),
            None,
            "{bad}"
        );
    }
    // A prefix is only a prefix immediately before a quote.
    assert_eq!(policy.hy_string_prefix_width(b"r x", 0), None);
    // And never outside Hy.
    assert_eq!(
        DialectReaderPolicy::new(Dialect::CommonLisp).hy_string_prefix_width(b"r\"x\"", 0),
        None
    );
}

/// The extent scanner, including the cases that decide where a literal ends.
#[test]
fn hy_string_extent_matches_hys_reader() {
    let policy = DialectReaderPolicy::new(Dialect::Hy);
    let closed = |width| Some(HyStringExtent::Closed { width });

    assert_eq!(policy.hy_string_extent(br#"f"hi {name}""#, 0), closed(12));
    assert_eq!(policy.hy_string_extent(br#"r"a)b""#, 0), closed(6));
    // The nested string holds the outer closing quote hostage.
    assert_eq!(policy.hy_string_extent(br#"f"{(str "}")}""#, 0), closed(14));
    // Doubled braces are literal, so this closes at its own final quote.
    assert_eq!(policy.hy_string_extent(br#"f"{{a}}""#, 0), closed(8));
    // An escaped quote does not close a literal.
    assert_eq!(policy.hy_string_extent(br#"f"a\"b {x}""#, 0), closed(11));
    // Bracket strings: the close is `]` + delim + `]`, so `]]` here is content.
    assert_eq!(policy.hy_string_extent(b"#[d[a]]b]d]", 0), closed(11));
    assert_eq!(policy.hy_string_extent(b"#[[a]]", 0), closed(6));
    // Scanning starts at `pos`, not at 0.
    assert_eq!(policy.hy_string_extent(br#"(f r"a)b")"#, 3), closed(6));
    // Unterminated, and not a string at all.
    assert_eq!(
        policy.hy_string_extent(br#"f"abc"#, 0),
        Some(HyStringExtent::Unterminated)
    );
    assert_eq!(
        policy.hy_string_extent(b"#[[abc", 0),
        Some(HyStringExtent::Unterminated)
    );
    assert_eq!(policy.hy_string_extent(b"abc", 0), None);
    assert_eq!(policy.hy_string_extent(b"", 0), None);
    // And never one outside Hy.
    assert_eq!(
        DialectReaderPolicy::new(Dialect::Clojure).hy_string_extent(b"#[[a]]", 0),
        None
    );
}

/// Nothing above may leak into another dialect.
///
/// `#[`, `~` and an `r"..."` prefix all mean something else, or nothing, in the
/// other ten readers. This pins them so a later edit to `has_prefixed_strings`
/// or `has_bracket_strings` cannot quietly widen, the way the Janet long
/// string test pins backtick.
#[test]
fn hy_string_and_unquote_rules_do_not_leak_to_other_dialects() {
    for dialect in [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Racket,
        Dialect::Clojure,
        Dialect::Fennel,
        Dialect::Lfe,
        Dialect::Carp,
        Dialect::Janet,
        Dialect::Unknown,
    ] {
        let policy = DialectReaderPolicy::new(dialect);
        assert!(!policy.has_prefixed_strings(), "{}", dialect.label());
        assert!(!policy.has_bracket_strings(), "{}", dialect.label());
        assert_eq!(
            policy.hy_string_prefix_width(b"r\"x\"", 0),
            None,
            "{}",
            dialect.label()
        );
        assert_eq!(
            policy.hy_string_extent(b"#[[a]]", 0),
            None,
            "{}",
            dialect.label()
        );

        // `#[` stays whatever it already was: a hash-literal prefix on a
        // bracket *list* in the dialects that have one, and a refusal in the
        // rest. Comparing the sliced text would not discriminate -- Fennel's
        // `#` prefix spans exactly the same bytes -- so this compares the
        // node kind, which is the thing that actually changed for Hy.
        let input = "(setv x #[[a]])";
        if let Ok(tree) = SyntaxTree::parse_with_dialect(input, dialect) {
            let third = &tree.root_view().children[0].children[2];
            assert_eq!(
                third.kind,
                ExpressionKind::List,
                "{}: `#[[a]]` must stay a list, not become one raw-string atom",
                dialect.label()
            );
        }
    }

    // `~` is unquote in Clojure too; only Hy gained an arm, and Clojure's
    // existing one must be untouched.
    let input = "`(foo ~bar ~@baz)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("valid");
    let prefixes = tree.root_view().children[0]
        .children
        .iter()
        .map(|child| child.reader_prefixes.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        prefixes,
        vec![
            vec![],
            vec![ReaderPrefix::Unquote],
            vec![ReaderPrefix::UnquoteSplicing],
        ]
    );
}

/// The recording reader and the discarded-form scanner must agree.
///
/// `#_` is Hy's datum comment, so unlike Janet's unreachable arm this path runs
/// on real input. If the two disagreed about where an f-string or bracket
/// string ends, a `#_` would discard the wrong number of bytes.
#[test]
fn hy_discarded_forms_use_the_same_string_extent() {
    for (input, expected) in [
        (r#"(f #_ f"{(str "}")}" tail)"#, vec!["f", "tail"]),
        ("(f #_ #[d[a]]b]d] tail)", vec!["f", "tail"]),
        (r#"(f #_ r"a)b" tail)"#, vec!["f", "tail"]),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Hy)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let children = tree.root_view().children[0]
            .children
            .iter()
            .map(|child| child.span.slice(input))
            .collect::<Vec<_>>();
        assert_eq!(children, expected, "{input}");
    }
}

// ---------------------------------------------------------------------------
// Carp. Every expectation below is read off the `sexpr` dispatch and the
// `aChar` / `pat` / `emptyCharacters` productions in Carp's own
// `src/Parsing.hs`, not off this reader's previous behaviour: Carp shared the
// permissive legacy reader until now, and that reader implemented none of it.
// ---------------------------------------------------------------------------

/// `&`, `@` and `~` prefix the *following form*, not just a symbol.
///
/// This is the defect these tests exist for. `readerMacro` consumes the sigil
/// and recurses into `sexpr`, so `@(g y)` is one form. Reading it as a bare
/// `@` atom plus a sibling inflated the enclosing call's arity by one, which
/// happened 1493 times across 116 of the 248 files in `carp-lang/Carp`.
#[test]
fn carp_sigils_prefix_the_following_form() {
    for (input, prefix, expected) in [
        ("(f @(g y))", ReaderPrefix::Copy, "@(g y)"),
        ("(f &(g y))", ReaderPrefix::Ref, "&(g y)"),
        ("(f ~(g y))", ReaderPrefix::Deref, "~(g y)"),
        ("(f @x)", ReaderPrefix::Copy, "@x"),
        ("(f &x)", ReaderPrefix::Ref, "&x"),
        ("(f $[1 2])", ReaderPrefix::StaticArray, "$[1 2]"),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Carp)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 2, "{input}");
        assert_eq!(form.children[1].span.slice(input), expected, "{input}");
        assert_eq!(form.children[1].reader_prefixes, vec![prefix], "{input}");
    }
}

/// Sigils stack, because `readerMacro` recurses into `sexpr` rather than into
/// `atom`: `@@x` is `(copy (copy x))` and `&@x` is `(ref (copy x))`.
#[test]
fn carp_sigils_stack_in_source_order() {
    let input = "(f &@x)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Carp).expect("valid");
    let form = &tree.root_view().children[0];
    assert_eq!(
        form.children[1].reader_prefixes,
        vec![ReaderPrefix::Ref, ReaderPrefix::Copy]
    );
}

/// `@"a b"` is a copy of a *string literal*, so the string stays whole.
///
/// Because `@` used to glue onto the following token, the opening quote was
/// swallowed and `(f @"a b")` silently became the two atoms `@"a` and `b"`.
#[test]
fn carp_copy_prefix_keeps_a_string_literal_whole() {
    let input = r#"(f @"a b")"#;
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Carp).expect("valid");
    let form = &tree.root_view().children[0];
    assert_eq!(form.children.len(), 2);
    assert_eq!(form.children[1].span.slice(input), r#"@"a b""#);
}

/// `aChar` is `\` plus one character, and the character may be a delimiter.
///
/// `\{` is what made `core/Format.carp` fail to parse: the brace was read as a
/// real delimiter and unbalanced the file. `\ ` is a space character literal
/// and occurs in `core/String.carp`; `\space` needs no separate arm because
/// the atom scanner carries the remaining letters into the same atom.
#[test]
fn carp_character_literals_cover_delimiters_and_named_characters() {
    for (input, expected) in [
        ("(f \\a)", "\\a"),
        ("(f \\{)", "\\{"),
        ("(f \\})", "\\}"),
        ("(f \\()", "\\("),
        ("(f \\))", "\\)"),
        ("(f \\[)", "\\["),
        ("(f \\])", "\\]"),
        ("(f \\\")", "\\\""),
        ("(f \\space)", "\\space"),
        ("(f \\ )", "\\ "),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Carp)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 2, "{input}");
        assert_eq!(form.children[1].span.slice(input), expected, "{input}");
    }
}

/// `#"…"` is a `Pattern` literal: one dispatch byte and one datum.
///
/// `parseInternalPattern` admits a `"` only as `\"`, so a backslash-aware scan
/// ends the literal where Carp ends it.
#[test]
fn carp_pattern_literal_is_one_span() {
    for (input, expected) in [
        (r#"(f #"[a-z]+")"#, r#"#"[a-z]+""#),
        (r#"(f #"a\"b")"#, r#"#"a\"b""#),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Carp)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 2, "{input}");
        assert_eq!(form.children[1].span.slice(input), expected, "{input}");
    }
}

/// A comma is whitespace, not an unquote: `emptyCharacters` lists it beside
/// space and tab. It separates `deftype` fields and `defn` parameters
/// throughout `core/`, e.g. `(deftype Point [x Int, y Int])`.
#[test]
fn carp_comma_is_whitespace() {
    let input = "(deftype Point [x Int, y Int])";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Carp).expect("valid");
    let fields = &tree.root_view().children[0].children[2];
    assert_eq!(fields.children.len(), 4);
    assert!(
        fields
            .children
            .iter()
            .all(|child| child.reader_prefixes.is_empty())
    );
}

/// Forms Carp's reader has no dispatch for are refused rather than read as
/// something else.
///
/// `#` is absent from `validCharacters` and `atom` has no branch that accepts
/// it, so every `#` form except `#"…"` is a read error upstream -- including
/// the `#;`, `#_`, `#+`, `#-`, `#.`, `#'`, `#?`, `#(`, `#[` and `#{` Carp
/// inherited from the legacy reader. An unterminated `#"` or `@"` must fail
/// loudly rather than swallow the rest of the file.
#[test]
fn carp_refuses_what_its_reader_has_no_dispatch_for() {
    for input in [
        "(f #(g))",
        "(f #{1})",
        "#+sbcl (f)",
        "#;(f)",
        "(f #\"abc)",
        "(f @\"abc)",
        "\\",
    ] {
        assert!(
            SyntaxTree::parse_with_dialect(input, Dialect::Carp).is_err(),
            "{input} should be refused"
        );
    }
}

/// Emacs Lisp reads a radix integer as one atom, exactly as Common Lisp does.
///
/// `#b1010`, `#o777`, `#x2a` and `#<radix>r<digits>` are the four spellings in
/// the Elisp reference manual's "Integer Basics". Every one of them was an
/// `unsupported reader dispatch` before, which fails the whole document: 122 of
/// the 1674 files in GNU Emacs's own `lisp/` tree -- `bookmark.el`,
/// `ansi-color.el`, `bindings.el`, `calc.el` among them -- parsed not at all,
/// so none of this workspace's commands could say anything about any of them.
#[test]
fn emacs_lisp_reads_radix_integers_as_single_atoms() {
    let input = "(a #x1f #b101 #o777 #24r1k #X1F #24R1K #x-1f #36rzz #016r10)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::EmacsLisp).expect("valid");
    let form = &tree.root_view().children[0];
    let texts: Vec<&str> = form
        .children
        .iter()
        .map(|child| child.span.slice(input))
        .collect();
    assert_eq!(
        texts,
        [
            "a", "#x1f", "#b101", "#o777", "#24r1k", "#X1F", "#24R1K", "#x-1f", "#36rzz",
            "#016r10",
        ]
    );
    // One atom each, with no reader prefix: the `#` belongs to the token, it
    // does not introduce a form.
    assert!(
        form.children
            .iter()
            .all(|child| child.kind == ExpressionKind::Atom && child.reader_prefixes.is_empty())
    );
}

/// A radix integer ends where Emacs ends it, including when the delimiter,
/// comment or string that follows touches the digits.
///
/// Emacs's `read_integer` stops at the first byte that is not ASCII
/// alphanumeric, so `#xFF)` is 255 followed by the paren -- it does not run on
/// to the next whitespace. The delimiters and the comment character are already
/// atom boundaries here, which is why recognising the literal is the whole fix
/// and no bespoke extent is needed.
///
/// A `"` touching the digits is deliberately *not* in this table. `#o777"s"`
/// reads as one atom, where Emacs reads 511 and then the string -- but so do
/// `abc"s"` and `12"s"`, both of which read as one atom on `main` today. That
/// is the atom scanner's existing model of a token adjacent to a string
/// literal, shared by every dialect, and not something the radix arm
/// introduces or is free to change unilaterally.
#[test]
fn emacs_lisp_radix_integer_ends_at_an_atom_boundary() {
    for (input, expected) in [
        ("(f #xFF)", "#xFF"),
        ("[#xff]", "#xff"),
        ("(f #x1f;c\n)", "#x1f"),
        ("(f #o777 )", "#o777"),
        ("(f #b101(g))", "#b101"),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::EmacsLisp)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let outer = &tree.root_view().children[0];
        let atom = outer
            .children
            .iter()
            .find(|child| child.span.slice(input).starts_with('#'))
            .unwrap_or_else(|| panic!("{input}: no radix atom"));
        assert_eq!(atom.span.slice(input), expected, "{input}");
    }
}

/// A malformed radix integer stays a hard parse error, because that is what
/// Emacs does with it.
///
/// `#xZZ` signals `invalid-read-syntax` and the file does not load, so refusing
/// it here agrees with the reader rather than silently reading a number Emacs
/// never would. It is also unchanged behaviour -- every one of these was
/// refused before this arm existed -- so the fix only widens what parses.
///
/// `#d99` earns its place: Common Lisp has `#d` (CLHS 2.4.8.6) and Emacs Lisp
/// does not, so copying the Common Lisp arm's byte list would have accepted it.
#[test]
fn emacs_lisp_refuses_malformed_radix_integers() {
    for input in [
        "(f #x)",
        "(f #xZZ)",
        "(f #b2)",
        "(f #o8)",
        "(f #d99)",
        "(f #D99)",
        "(f #37r1)",
        "(f #1r0)",
        "(f #39r1)",
        "(f #35rz)",
        "(f #10r9a)",
        "(f #x1f2gh)",
        "(f #o7778)",
        "(f #24r)",
        "(f #x-)",
        "(f #r1)",
        "#x",
    ] {
        assert!(
            SyntaxTree::parse_with_dialect(input, Dialect::EmacsLisp).is_err(),
            "{input} is invalid-read-syntax in Emacs and must be refused"
        );
    }
}

/// The `#` forms Emacs Lisp has that are *not* radix integers stay exactly as
/// they were.
///
/// `#s(...)` records, `#[...]` byte-code objects, `#(...)` propertized strings,
/// `#^[...]` char-tables, `#&N"..."` bool-vectors, `##` (the empty symbol) and
/// `#:foo` (uninterned) are all real Emacs Lisp reader syntax that this arm
/// deliberately does not touch. They remain refused, as they were before, and
/// they account for 33 of the 86 files still failing over Emacs's `lisp/` tree.
/// Pinning them here records that the gap is known and scoped out rather than
/// accidentally closed or accidentally widened.
#[test]
fn emacs_lisp_radix_arm_leaves_the_other_sharp_forms_refused() {
    for input in [
        "(f #s(foo 1))",
        "(f #[1 2 3])",
        "(f #(\"ab\" 0 1 (face bold)))",
        "(f #^[1 2])",
        "(f #&5\"x\")",
        "(f ##)",
        "(f #:foo)",
        "(f #1=(a))",
    ] {
        assert!(
            SyntaxTree::parse_with_dialect(input, Dialect::EmacsLisp).is_err(),
            "{input} is outside this fix and must still be refused"
        );
    }
    // `#'` is the one `#` form Emacs Lisp already had, and it keeps its
    // two-byte function prefix rather than being mistaken for a radix.
    let input = "(f #'g)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::EmacsLisp).expect("valid");
    let argument = &tree.root_view().children[0].children[1];
    assert_eq!(argument.reader_prefixes, vec![ReaderPrefix::Function]);
}

/// The radix arm is scoped to Emacs Lisp and changes no other dialect.
///
/// `#x`/`#b`/`#o` are Common Lisp and Scheme radix syntax too, and `#<n>r` is
/// Common Lisp's and LFE's, so this is the pin that would catch the arm being
/// widened past the dialect it was written for. Each expectation below is what
/// the dialect's own reader does, and every one of them was verified
/// byte-identical between binaries built before and after this change:
///
/// * Common Lisp already returned `None` for these bytes (CLHS 2.4.8.6-2.4.8.9),
///   so it always read them as single atoms. Emacs Lisp now matches it exactly.
/// * LFE, Hy and the permissive legacy reader have no `#`-radix dispatch, so
///   the atom scanner already took the whole token.
/// * Scheme and Racket accept `#x`/`#b`/`#o` (R7RS 6.2.5 has no `#<n>r`), so
///   `#24r1k` is an unsupported dispatch there and stays one.
/// * Clojure has none of them.
#[test]
fn emacs_lisp_radix_arm_does_not_change_other_dialects() {
    let letter_bases = "(a #x1f #b101 #o777)";
    for dialect in [
        Dialect::CommonLisp,
        Dialect::Lfe,
        Dialect::Hy,
        Dialect::Unknown,
        Dialect::Scheme,
        Dialect::Racket,
    ] {
        let tree = SyntaxTree::parse_with_dialect(letter_bases, dialect)
            .unwrap_or_else(|error| panic!("{dialect:?}: {error}"));
        let texts: Vec<&str> = tree.root_view().children[0]
            .children
            .iter()
            .map(|child| child.span.slice(letter_bases))
            .collect();
        assert_eq!(texts, ["a", "#x1f", "#b101", "#o777"], "{dialect:?}");
    }

    // `#<n>r` is Common Lisp's and LFE's, and nobody else's.
    let general_radix = "(a #24r1k)";
    for dialect in [
        Dialect::CommonLisp,
        Dialect::Lfe,
        Dialect::Hy,
        Dialect::Unknown,
    ] {
        let tree = SyntaxTree::parse_with_dialect(general_radix, dialect)
            .unwrap_or_else(|error| panic!("{dialect:?}: {error}"));
        assert_eq!(
            tree.root_view().children[0].children[1]
                .span
                .slice(general_radix),
            "#24r1k",
            "{dialect:?}"
        );
    }
    for dialect in [Dialect::Scheme, Dialect::Racket, Dialect::Clojure] {
        assert!(
            SyntaxTree::parse_with_dialect(general_radix, dialect).is_err(),
            "{dialect:?} has no #<n>r radix and must keep refusing it"
        );
    }

    // A spelling Common Lisp accepts and Emacs Lisp refuses, so the two arms
    // cannot be collapsed into one shared table.
    assert!(SyntaxTree::parse_with_dialect("(a #d99)", Dialect::CommonLisp).is_ok());
    assert!(SyntaxTree::parse_with_dialect("(a #d99)", Dialect::EmacsLisp).is_err());
}

/// Formatting a document containing radix integers round-trips.
///
/// `treefmt` formats this repository's own Lisp with paredit, so a formatter
/// regression here is a build break rather than a cosmetic one. The literal
/// must survive as written -- neither re-cased nor split -- and a second pass
/// must be a fixed point.
#[test]
fn emacs_lisp_radix_integers_survive_a_format_round_trip() {
    let input = "(defconst c\n  (list #x1f #B101 #o777 #24R1K #x-1f))\n";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::EmacsLisp).expect("valid");
    let formatter = Formatter::with_dialect(2, Dialect::EmacsLisp);
    let formatted = formatter.format(&tree);
    for literal in ["#x1f", "#B101", "#o777", "#24R1K", "#x-1f"] {
        assert!(
            formatted.contains(literal),
            "{literal} lost from {formatted:?}"
        );
    }
    let again = SyntaxTree::parse_with_dialect(&formatted, Dialect::EmacsLisp).expect("valid");
    assert_eq!(formatter.format(&again), formatted);
}

// ---------------------------------------------------------------------------
// Racket's `#`-dispatch table (`racket/src/expander/read/main.rkt`).
//
// Racket shared `classify_scheme` until 1777 of 4492 real `.rkt` files (39.6%)
// over `racket/racket` plus `racket/typed-racket` were measured failing to
// parse, every one an `UnsupportedReaderDispatch { dispatch: "#" }`.
// ---------------------------------------------------------------------------

/// `#%app` is a *symbol*, not a dispatch.
///
/// `read-dispatch` sends `%` to `read-symbol-or-number` with
/// `#:extra-prefix dispatch-c`, which seeds the accumulator with the `#`. So
/// the fix belongs in the atom scanner, and the reader table's job is only to
/// stay out of its way.
#[test]
fn racket_hash_percent_is_an_identifier_not_a_dispatch() {
    for (input, expected) in [
        ("(#%app f x)", "#%app"),
        ("(#%plain-lambda () 1)", "#%plain-lambda"),
        ("('#%kernel)", "#%kernel"),
        ("(#%)", "#%"),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let form = &tree.root_view().children[0];
        let head = &form.children[0];
        assert_eq!(
            head.span.slice(input).trim_start_matches('\''),
            expected,
            "{input}"
        );
    }
}

/// `#'x` is `(syntax x)` and takes exactly one following form, so `x` stays a
/// visible child rather than disappearing into an opaque span.
#[test]
fn racket_syntax_quote_is_a_prefix_on_one_form() {
    for (input, prefixed) in [
        ("(f #'x)", "#'x"),
        ("(f #'(a b))", "#'(a b)"),
        ("(f #'#'x)", "#'#'x"),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 2, "{input}");
        assert_eq!(form.children[1].span.slice(input), prefixed, "{input}");
        assert!(
            form.children[1]
                .reader_prefixes
                .contains(&ReaderPrefix::Function),
            "{input}"
        );
    }
    let tree = SyntaxTree::parse_with_dialect("#'(a b)", Dialect::Racket).expect("valid");
    assert_eq!(tree.root_view().children[0].children.len(), 2);
}

/// `` #` ``, `#,` and `#,@` each take exactly one following form. `#,@` is
/// three bytes, not two: `read-dispatch`'s `#\,` arm peeks for an `@` and
/// consumes it before delegating to `read-quote`.
#[test]
fn racket_quasisyntax_family_spans_its_dispatch_and_one_form() {
    for (input, expected) in [
        ("(f #`x)", "#`x"),
        ("(f #`(a b))", "#`(a b)"),
        ("(f #,x)", "#,x"),
        ("(f #,@x)", "#,@x"),
        ("(f #,@(a b))", "#,@(a b)"),
        ("(f #`(a #,b))", "#`(a #,b)"),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 2, "{input}");
        assert_eq!(form.children[1].span.slice(input), expected, "{input}");
    }
}

/// A `#rx`/`#px` payload is an *ordinary* string literal — `read-regexp` calls
/// the same `read-string` the `"` dispatch does — so `\"` escapes rather than
/// closes, and `#rx#"…"` is the byte-regexp spelling. `#rx` with no literal
/// after it is Racket's own "expected `\"` or `#`" error.
#[test]
fn racket_regexp_literal_is_one_span() {
    for (input, expected) in [
        (r#"(f #rx"[a-z]+")"#, r#"#rx"[a-z]+""#),
        (r#"(f #rx"\"quoted\"")"#, r#"#rx"\"quoted\"""#),
        (r#"(f #px"^\\d+$")"#, r#"#px"^\\d+$""#),
        (r#"(f #rx#"bytes")"#, r#"#rx#"bytes""#),
        (r#"(f #px#"bytes")"#, r#"#px#"bytes""#),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 2, "{input}");
        assert_eq!(form.children[1].span.slice(input), expected, "{input}");
    }
    for input in ["(f #rx x)", "(f #rxy)", "(f #reader m)"] {
        assert!(
            SyntaxTree::parse_with_dialect(input, Dialect::Racket).is_err(),
            "{input} should be refused"
        );
    }
}

/// `#"…"` is `read-string` in `'|byte string|` mode: the same lexical extent
/// as a string, so the `#` is a prefix on the literal that follows.
#[test]
fn racket_byte_string_is_one_span() {
    for (input, expected) in [
        (r#"(f #"abc")"#, r#"#"abc""#),
        (r#"(f #"a\"b")"#, r#"#"a\"b""#),
        (r#"(f #"a)b")"#, r#"#"a)b""#),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 2, "{input}");
        assert_eq!(form.children[1].span.slice(input), expected, "{input}");
        assert!(
            form.children[1]
                .reader_prefixes
                .contains(&ReaderPrefix::HashLiteral),
            "{input}"
        );
    }
}

/// Vectors, sized vectors, fixnum/flonum vectors, prefab structs and boxes.
///
/// `#(…)` keeps its elements visible through `HashLiteral`; the spellings that
/// carry a payload in the dispatch itself (`#3`, `#fl6`, `#s`, `#&`) become one
/// opaque reader form, because `ReaderPrefix` has no spelling for them.
#[test]
fn racket_vector_struct_and_box_dispatches() {
    for (input, expected, children) in [
        ("(f #(1 2))", "#(1 2)", 2usize),
        ("(f #[1 2])", "#[1 2]", 2),
        ("(f #{1 2})", "#{1 2}", 2),
        ("(f #3(0))", "#3(0)", 0),
        ("(f #fl6(0.0))", "#fl6(0.0)", 0),
        ("(f #fx(1))", "#fx(1)", 0),
        ("(f #s(pt 1 2))", "#s(pt 1 2)", 0),
        ("(f #&x)", "#&x", 0),
        ("(f #&(a b))", "#&(a b)", 0),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 2, "{input}");
        assert_eq!(form.children[1].span.slice(input), expected, "{input}");
        assert_eq!(form.children[1].children.len(), children, "{input}");
    }
}

/// `read-hash` chains case-insensitive `get-next!` calls and then requires an
/// opener with no whitespace before it, so `#hash (…)` is its own "bad syntax"
/// error and `#HASH(` is as valid as `#hash(`. The longest spelling has to win:
/// `#hasheqv(` is not `#hasheq` followed by a stray `v`.
#[test]
fn racket_hash_table_literal_dispatch() {
    for (input, expected) in [
        ("(f #hash((a . 1)))", "#hash((a . 1))"),
        ("(f #hasheq((a . 1)))", "#hasheq((a . 1))"),
        ("(f #hasheqv((a . 1)))", "#hasheqv((a . 1))"),
        ("(f #hashalw((a . 1)))", "#hashalw((a . 1))"),
        ("(f #HASH((a . 1)))", "#HASH((a . 1))"),
        ("(f #hash[(a . 1)])", "#hash[(a . 1)]"),
        ("(f #hash())", "#hash()"),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 2, "{input}");
        assert_eq!(form.children[1].span.slice(input), expected, "{input}");
    }
    for input in ["(f #hash ((a . 1)))", "(f #hashx((a . 1)))", "(f #h)"] {
        assert!(
            SyntaxTree::parse_with_dialect(input, Dialect::Racket).is_err(),
            "{input} should be refused"
        );
    }
}

/// A here string is one atom spanning `#<<`, its tag, the content, the
/// terminator line and the newline after it.
///
/// The terminator is the whole rest of the opening line and must sit alone on
/// its own line: `read-here-string` fails the match on leading whitespace and
/// falls out of the `(char=? c #\newline)` test on trailing whitespace, so both
/// spellings stay ordinary content. A tag matched at the very first content
/// byte is the empty string, and a tag matched at EOF with no newline after it
/// still terminates — the EOF branch is `(unless (null? terminator) ...)`.
#[test]
fn racket_here_string_extent_matches_read_here_string() {
    use crate::sexpr::reader_policy::HereStringExtent;

    let policy = DialectReaderPolicy::new(Dialect::Racket);
    for (input, expected) in [
        // Ordinary.
        ("#<<END\nbody\nEND\n", Some(16usize)),
        // The terminator may match at the very first content byte.
        ("#<<END\nEND\n", Some(11)),
        // No newline after the terminator: EOF ends it.
        ("#<<END\nbody\nEND", Some(15)),
        // An empty tag terminates on the first empty line.
        ("#<<\n\n", Some(5)),
        // A tag may contain spaces, and then only that exact line ends it.
        ("#<<E ND\nx\nE ND\n", Some(15)),
        // Leading whitespace on the terminator line is content.
        ("#<<END\nx\n  END\nEND\n", Some(19)),
        // Trailing whitespace on the terminator line is content.
        ("#<<END\nx\nEND \nEND\n", Some(18)),
        // A prefix of the tag is content.
        ("#<<END\nEN\nEND\n", Some(14)),
    ] {
        assert_eq!(
            policy.here_string_extent(input.as_bytes(), 0),
            expected.map(|width| HereStringExtent::Closed { width }),
            "{input:?}"
        );
    }
    for input in [
        // The tag never reappears.
        "#<<END\nbody\n",
        // Only as a prefix of a longer line.
        "#<<END\nENDING\n",
        // EOF before the newline that ends the opening line.
        "#<<END",
        "#<<",
    ] {
        assert_eq!(
            policy.here_string_extent(input.as_bytes(), 0),
            Some(HereStringExtent::Unterminated),
            "{input:?}"
        );
    }
    // No other dialect has the form.
    for dialect in [Dialect::Scheme, Dialect::CommonLisp, Dialect::Clojure] {
        assert_eq!(
            DialectReaderPolicy::new(dialect).here_string_extent(b"#<<END\nx\nEND\n", 0),
            None,
            "{dialect:?}"
        );
    }
}

/// A here string is one node, so delimiters inside its content cannot unbalance
/// the enclosing list, and an unterminated one fails loudly rather than
/// swallowing the rest of the file.
#[test]
fn racket_here_string_is_one_atom_inside_a_list() {
    let input = "(list #<<END\n)))) not code (\nEND\n)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("valid");
    let form = &tree.root_view().children[0];
    assert_eq!(form.children.len(), 2);
    assert_eq!(
        form.children[1].span.slice(input),
        "#<<END\n)))) not code (\nEND\n"
    );
    assert!(form.children[1].children.is_empty());

    for input in ["(list #<<END\nbody\n)", "(list #<<END)", "#; #<<END\nx\n"] {
        assert!(
            SyntaxTree::parse_with_dialect(input, Dialect::Racket).is_err(),
            "{input} should be refused"
        );
    }
    // A `#;` discards a complete here string through the same scanner.
    let input = "#; #<<END\nx\nEND\nkept";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("valid");
    assert_eq!(tree.root_view().children.len(), 1);
}

/// Forms Racket's reader has no fixed extent for stay loud refusals rather than
/// becoming a guess about where the form ends.
///
/// `#S(` is included deliberately: `read-struct` is reached from a `case` with
/// a `#\s` clause and no `#\S` clause, so an upper-case struct really is "bad
/// syntax" in Racket even though Common Lisp accepts it.
#[test]
fn racket_refuses_what_its_reader_has_no_dispatch_for() {
    for input in [
        "(f #reader m x)",
        "(f #~compiled)",
        "#2dmatch\nx",
        "(f #S(pt 1))",
        "(f #<x)",
        "(f #u8(1 2))",
        "(f #)",
        "#\\",
    ] {
        assert!(
            SyntaxTree::parse_with_dialect(input, Dialect::Racket).is_err(),
            "{input} should be refused"
        );
    }
}

/// The forms Racket's own table keeps: `#lang` stays trivia, `#:kw` and the
/// booleans and number prefixes stay atoms, `#\c` stays a character literal,
/// `#|…|#` stays a block comment and `#;` still discards.
#[test]
fn racket_keeps_the_forms_it_already_read() {
    for (input, children) in [
        ("#lang racket/base\n(f x)", 2usize),
        ("(f #:mode 1)", 3),
        ("(f #t #f #true #false)", 5),
        ("(f #x1f #o17 #b101 #e1.0 #i1 #d9)", 7),
        ("(f #\\a #\\space #\\( #\\))", 5),
        ("(f #| block |# x)", 2),
        ("(f #;discarded kept)", 2),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let root = tree.root_view();
        let form = root.children.last().expect("at least one form");
        assert_eq!(form.children.len(), children, "{input}");
    }
}

/// `#!` plus a space or a `/` is a Unix line comment, skipped in
/// `read-char/skip-whitespace-and-comments` beside `;` and `#|`.
///
/// Being in the whitespace skipper rather than in `read-dispatch` has two
/// consequences the Emacs Lisp and Hy shebang arms do not share: it is not
/// restricted to offset 0, and `skip-unix-line-comment!` continues onto the
/// next line when the byte before the newline is a `\`. Without a space or a
/// `/` after the `!` there is no comment at all — `#!racket` goes to
/// `read-lang` — so it stays an atom.
#[test]
fn racket_shebang_is_a_line_comment() {
    for (input, forms) in [
        ("#!/bin/sh\n(a b)\n", 1usize),
        ("#! /usr/bin/env racket\n(a b)\n", 1),
        // The `\` continuation swallows the next line too.
        ("#! /bin/sh \\\n(not code)\n(a b)\n", 1),
        ("#! /bin/sh \\\n(x) \\\n(y)\n(a b)\n", 1),
        // Not restricted to offset 0, and not restricted to top level.
        ("(a\n#! /bin/sh\nb)\n", 1),
        ("(a b)\n#!/bin/sh\n", 1),
        // No space and no slash: not a comment.
        ("#!racket\n(a b)\n", 2),
        ("(f #!eof)\n", 1),
        // At end of input with no newline to end it.
        ("(a b)\n#! /bin/sh", 1),
        // `#` and `!` are ordinary symbol constituents, so a `#!` *inside* a
        // token is not a comment however the token continues. Racket's own
        // reader is full of `(read-extension-#! x)`, and splitting it stopped
        // `read/main.rkt` and `read/language.rkt` parsing at all.
        ("(read-extension-#! x)\n", 1),
        ("(f a#!/b c)\n", 1),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket)
            .unwrap_or_else(|error| panic!("{input:?}: {error}"));
        assert_eq!(tree.root_view().children.len(), forms, "{input:?}");
    }
    // The comment keeps every later byte offset unchanged.
    let input = "#!/bin/sh\n(a b)\n";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("valid");
    assert_eq!(tree.root_view().children[0].span.slice(input), "(a b)");
    // A `#!` inside a token keeps the whole token.
    let input = "(read-extension-#! x)\n";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("valid");
    let form = &tree.root_view().children[0];
    assert_eq!(form.children.len(), 2);
    assert_eq!(form.children[0].span.slice(input), "read-extension-#!");
    // Scheme keeps its own reading: `#!` there is `#!eof`/`#!fold-case`.
    let input = "#!/bin/sh\n(a b)\n";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Scheme).expect("valid");
    assert_eq!(tree.root_view().children.len(), 2);
}

/// `"` is in `char-delimiter?`, so it ends the token before it.
///
/// `(format"~a" x)` is a symbol and a string with no space between them, and
/// Racket's own benchmark suite writes it that way. Reading it as one token
/// made the atom swallow the opening quote, stop at the space inside the
/// literal, and turn the rest of the string into sibling atoms — which
/// `edit format` then re-emitted with a line break inside string data.
#[test]
fn racket_string_terminates_the_token_before_it() {
    let input = r#"(format"~a ~a" x)"#;
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("valid");
    let form = &tree.root_view().children[0];
    assert_eq!(form.children.len(), 3);
    assert_eq!(form.children[0].span.slice(input), "format");
    assert_eq!(form.children[1].span.slice(input), r#""~a ~a""#);
    // A `#\"` character literal still takes the quote as its payload.
    let input = r#"(f #\" x)"#;
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("valid");
    assert_eq!(tree.root_view().children[0].children.len(), 3);
    // The `#`-prefixed literals keep their own scanners.
    for (input, expected) in [
        (r#"(f #"b")"#, r#"#"b""#),
        (r#"(f #rx"a")"#, r#"#rx"a""#),
        (r#"(f #rx#"a")"#, r#"#rx#"a""#),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        let form = &tree.root_view().children[0];
        assert_eq!(form.children.len(), 2, "{input}");
        assert_eq!(form.children[1].span.slice(input), expected, "{input}");
    }
    // Eight other dialects keep the reading they had. Hy is the reason the rule
    // cannot be unconditional: there `r"a"` really is a prefixed literal.
    //
    // Emacs Lisp used to be in this list and is now asserted with Racket
    // instead. That is not a weakening: `read0` breaks a symbol on any of
    // `"';#()[]` and `,`, so `(format"x" 1)` really is a symbol followed by a
    // string, and reading it as one token made `edit format` insert a line
    // break *inside* the string literal -- silent corruption in `table.el`,
    // `autoarg.el` and `tpu-mapper.el`.
    for dialect in [
        Dialect::Scheme,
        Dialect::CommonLisp,
        Dialect::Clojure,
        Dialect::Lfe,
        Dialect::Carp,
        Dialect::Janet,
        Dialect::Fennel,
        Dialect::Unknown,
    ] {
        assert!(
            !DialectReaderPolicy::new(dialect).is_atom_boundary(b"a\"b", 1),
            "{dialect:?}"
        );
    }
    assert!(!DialectReaderPolicy::new(Dialect::Hy).is_atom_boundary(b"r\"b", 1));
    for dialect in [Dialect::Racket, Dialect::EmacsLisp] {
        assert!(
            DialectReaderPolicy::new(dialect).is_atom_boundary(b"a\"b", 1),
            "{dialect:?}"
        );
    }
}

/// Splitting Racket out of `classify_scheme` must leave Scheme byte-identical.
///
/// Every form Racket gained above is still exactly as Scheme read it before —
/// refused where it was refused, read where it was read. `#'`, `` #` ``, `#,`
/// and `#,@` are R6RS lexical syntax that Guile, Chez and Chicken all accept,
/// so this pins a *known gap* rather than an intended reading; closing it is a
/// change to Scheme's reader needing its own Scheme corpus audit.
#[test]
fn scheme_reader_is_unchanged_by_the_racket_split() {
    for input in [
        "(f #'x)",
        "(f #`x)",
        "(f #,x)",
        "(f #\"bytes\")",
        "(f #rx\"a\")",
        "(f #px\"a\")",
        "(f #hash((a . 1)))",
        "(f #s(pt 1))",
        "(f #&x)",
        "(f #{1})",
        // `#[` is a vector in Racket but not in Scheme: `classify_scheme`'s
        // dispatch table admits `#(` alone.
        "(f #[1 2])",
        "(f #3(0))",
        "#<<END\nx\nEND\n",
    ] {
        assert!(
            SyntaxTree::parse_with_dialect(input, Dialect::Scheme).is_err(),
            "{input} should still be refused for Scheme"
        );
    }
    // `#u8(…)` is R7RS and stays; Racket's own `read-dispatch` has no `#\u`
    // clause, so it is refused there (pinned above) and kept here.
    let input = "(f #u8(1 2))";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Scheme).expect("valid");
    assert_eq!(tree.root_view().children[0].children.len(), 2);
    // `#(` keeps its Scheme reading, and `#;`, `#\` and `#!` too.
    for (input, children) in [
        ("(f #(1 2))", 2usize),
        ("(f #;discarded kept)", 2),
        ("(f #\\a #!eof)", 3),
    ] {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Scheme)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        assert_eq!(
            tree.root_view().children[0].children.len(),
            children,
            "{input}"
        );
    }
}
