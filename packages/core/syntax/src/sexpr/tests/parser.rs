use super::*;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::parser::MAX_DISCARDED_FORM_STACK_FRAMES;

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

#[test]
fn feature_dispatch_scans_separately_from_feature_expression() {
    // `#+`/`#-` must scan as their own token so `#+sbcl` and
    // `#+(and sbcl x86-64)` produce the same tree shape: dispatch, feature
    // expression, guarded datum as three siblings. Otherwise a bare feature
    // symbol glues onto `#+` into one opaque atom while a compound feature
    // expression stays a separate list, hiding the feature symbol from
    // find/rename in the bare spelling only.
    let simple_input = "(defun f () #+sbcl (declare (optimize speed)) 1)";
    let simple = SyntaxTree::parse(simple_input).expect("valid");
    let form = simple.select_path(&parse_path("0")).expect("form").view();
    assert_eq!(form.children[3].text.as_deref(), Some("#+"));
    assert_eq!(form.children[4].text.as_deref(), Some("sbcl"));
    assert_eq!(
        form.children[5].span.slice(simple_input),
        "(declare (optimize speed))"
    );

    let compound_input = "(defun f () #+(and sbcl x86-64) (declare (optimize speed)) 1)";
    let compound = SyntaxTree::parse(compound_input).expect("valid");
    let form = compound.select_path(&parse_path("0")).expect("form").view();
    assert_eq!(form.children[3].text.as_deref(), Some("#+"));
    assert_eq!(
        form.children[4].span.slice(compound_input),
        "(and sbcl x86-64)"
    );
    assert_eq!(
        form.children[5].span.slice(compound_input),
        "(declare (optimize speed))"
    );
}

#[test]
fn feature_dispatch_negative_variant_scans_separately_too() {
    let input = "(declare #-sbcl (optimize speed))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let form = tree.select_path(&parse_path("0")).expect("form").view();
    assert_eq!(form.children[1].text.as_deref(), Some("#-"));
    assert_eq!(form.children[2].text.as_deref(), Some("sbcl"));
    assert_eq!(form.children[3].span.slice(input), "(optimize speed)");
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
