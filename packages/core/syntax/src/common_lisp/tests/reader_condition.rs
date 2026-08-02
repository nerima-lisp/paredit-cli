use super::*;
use crate::common_lisp::{
    CommonLispReaderConditionalKind, common_lisp_reader_conditional_dispatches,
    common_lisp_reader_conditional_forms,
};

/// An incomplete conditional is refused, in both readers.
///
/// This test used to assert that `#+ #-` *parses*, into two bare dispatch
/// atoms, and that the report lists both. That was only ever true of
/// `SyntaxTree::parse` — the permissive `Dialect::Unknown` reader — and
/// `Dialect::CommonLisp` has always refused the same input outright.
///
/// The intent behind the old assertion was stated in
/// `common_lisp_reader_conditional_dispatches`' own doc comment: "a bare
/// legacy dispatch is still reported so callers can reject incomplete input
/// safely before attempting a structural refactor". Refusing the document at
/// parse time is a strictly stronger form of that guarantee — no caller can
/// forget to check — so the intent is preserved, one layer down.
#[test]
fn an_incomplete_conditional_is_refused_by_both_readers() {
    for dialect in [Dialect::Unknown, Dialect::CommonLisp] {
        assert!(
            SyntaxTree::parse_with_dialect("#+ #-", dialect).is_err(),
            "{dialect:?} accepted a conditional with no feature expression"
        );
    }
}

#[test]
fn detects_simple_and_compound_feature_conditions() {
    let tree = SyntaxTree::parse("#+sbcl (compile-file source) #-(and sbcl x86-64) (load source)")
        .expect("parse succeeds");

    let dispatches = common_lisp_reader_conditional_dispatches(&tree);

    assert_eq!(dispatches.len(), 2);
    assert_eq!(dispatches[0].kind, CommonLispReaderConditionalKind::Include);
    assert_eq!(dispatches[1].kind, CommonLispReaderConditionalKind::Exclude);
    assert_eq!(
        dispatches[0]
            .span
            .slice("#+sbcl (compile-file source) #-(and sbcl x86-64) (load source)"),
        "#+"
    );
    assert_eq!(
        dispatches[1]
            .span
            .slice("#+sbcl (compile-file source) #-(and sbcl x86-64) (load source)"),
        "#-"
    );
}

/// A conditional behind a reader prefix reads the same in both readers.
///
/// Both find nothing, and that is a known gap rather than the behaviour this
/// test is here to endorse: `push_opaque_reader_form` records no reader
/// prefixes and a `symbol_offset` of `0`, so an opaque form's text begins at
/// the `'` rather than at the `#+`, and `reader_conditional` does not
/// recognise it. `Dialect::CommonLisp` has always behaved this way — `inspect
/// read-conditionals` reports zero findings for this input on a `.lisp` file —
/// so it is a pre-existing defect in the report, not something introduced by
/// making `Dialect::Unknown` agree with it.
///
/// It is pinned as *parity* rather than as an expected count so that fixing
/// the gap has to fix both readers at once, and so the fix shows up here as a
/// deliberate change rather than as a silent divergence.
#[test]
fn a_conditional_behind_a_reader_prefix_reads_the_same_in_both_readers() {
    let input = "'#+sbcl selected #'#-sbcl rejected";

    let unknown = SyntaxTree::parse_with_dialect(input, Dialect::Unknown).expect("parse succeeds");
    let common_lisp =
        SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse succeeds");

    let kinds = |tree: &SyntaxTree| {
        common_lisp_reader_conditional_forms(tree)
            .iter()
            .map(|form| form.kind)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        kinds(&unknown),
        kinds(&common_lisp),
        "the two readers disagree about a prefixed conditional"
    );
}

/// Sibling conditionals are all found; one nested inside a guarded form is not.
///
/// The old expectation was three dispatches, including the `#-inner` nested
/// inside what `#+outer` guards, at path `0.3.1`. Those paths are the tell:
/// `0.3` and `0.4` only exist in a tree where `#+outer`, its feature
/// expression and its guarded form are separate siblings.
///
/// A guarded form is *opaque* — it has no children to walk — so a conditional
/// inside one is not reported. `Dialect::CommonLisp` has always behaved this
/// way; `inspect read-conditionals` on this input as a `.lisp` file reports
/// `outer` and `fallback` and not `inner`. Only the permissive reader ever
/// reported three, and only because it had torn the conditional apart.
///
/// Not descending is the safe direction for the callers that matter: the
/// mutation-safety gate treats a conditional's whole extent as untouchable, so
/// missing an inner one cannot authorise an edit it should have refused.
#[test]
fn sibling_conditionals_are_found_and_a_guarded_one_stays_opaque() {
    let input = "(progn #+outer (progn #-inner skipped) #-fallback (run))";

    for dialect in [Dialect::Unknown, Dialect::CommonLisp] {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse succeeds");
        let forms = common_lisp_reader_conditional_forms(&tree);

        assert_eq!(
            forms.iter().map(|form| form.kind).collect::<Vec<_>>(),
            vec![
                CommonLispReaderConditionalKind::Include,
                CommonLispReaderConditionalKind::Exclude,
            ],
            "{dialect:?}"
        );
        assert_eq!(
            forms
                .iter()
                .map(|form| form.span.slice(input))
                .collect::<Vec<_>>(),
            vec!["#+outer (progn #-inner skipped)", "#-fallback (run)"],
            "{dialect:?}"
        );
    }
}

#[test]
fn does_not_confuse_clojure_conditionals_or_reader_comments_with_common_lisp_dispatches() {
    for input in ["#?(:clj selected :cljs ignored)", "#; #+sbcl ignored"] {
        let tree = SyntaxTree::parse(input).expect("parse succeeds");

        assert!(common_lisp_reader_conditional_dispatches(&tree).is_empty());
    }
}

/// A form's `span` covers the guarded datum, whatever shape either half takes.
///
/// This replaces `collects_complete_forms_from_legacy_and_dialect_aware_trees`,
/// which parsed the same input twice — once as "legacy" via `SyntaxTree::parse`
/// and once as `Dialect::CommonLisp` — and asserted the two agreed. That was a
/// real assertion while the permissive reader tore a conditional into three
/// siblings and this module had to stitch it back together; once both readers
/// produce one node it asserted a tautology, and
/// `the_unknown_dialect_reads_feature_conditionals_exactly_as_common_lisp_does`
/// in `sexpr::tests::parser` pins the parity properly anyway.
///
/// What is worth pinning here instead is the *extent*, which nothing else
/// states: `mutation_safety::reader_condition` refuses any edit overlapping a
/// returned `span`, so a span that stopped at the feature expression would let
/// an edit cut a conditional in half. The four cases below are the four
/// combinations of a bare or compound feature expression with an atom or list
/// as the guarded datum — the shapes that used to be stitched together by
/// counting siblings, and so the shapes most likely to regress.
#[test]
fn a_form_span_covers_the_guarded_datum_for_every_feature_and_datum_shape() {
    let cases = [
        ("#+sbcl (compile-file source)", "#+"),
        ("#-(and sbcl x86-64) (load source)", "#-"),
        ("#+sbcl selected", "#+"),
        ("#-(or ecl abcl) rejected", "#-"),
    ];

    for (input, dispatch) in cases {
        for dialect in [Dialect::Unknown, Dialect::CommonLisp] {
            let tree = SyntaxTree::parse_with_dialect(input, dialect)
                .unwrap_or_else(|error| panic!("{dialect:?}: {input}: {error}"));
            let forms = common_lisp_reader_conditional_forms(&tree);

            assert_eq!(forms.len(), 1, "{dialect:?}: {input}");
            assert_eq!(forms[0].dispatch_span.slice(input), dispatch);
            assert_eq!(
                forms[0].span.slice(input),
                input,
                "{dialect:?}: the span stops short of the guarded datum"
            );
            assert_eq!(
                forms[0].kind,
                if dispatch == "#+" {
                    CommonLispReaderConditionalKind::Include
                } else {
                    CommonLispReaderConditionalKind::Exclude
                },
                "{dialect:?}: {input}"
            );
        }
    }
}

/// Two conditionals in a row are two forms, each ending where its own guarded
/// datum ends rather than running into the next dispatch.
#[test]
fn adjacent_conditionals_do_not_bleed_into_each_other() {
    let input = "#+sbcl (compile-file source) #-(and sbcl x86-64) (load source)";

    for dialect in [Dialect::Unknown, Dialect::CommonLisp] {
        let tree = SyntaxTree::parse_with_dialect(input, dialect)
            .unwrap_or_else(|error| panic!("{dialect:?}: {error}"));
        let forms = common_lisp_reader_conditional_forms(&tree);

        assert_eq!(
            forms
                .iter()
                .map(|form| form.span.slice(input))
                .collect::<Vec<_>>(),
            vec![
                "#+sbcl (compile-file source)",
                "#-(and sbcl x86-64) (load source)"
            ],
            "{dialect:?}"
        );
        assert_eq!(
            forms.iter().map(|form| form.kind).collect::<Vec<_>>(),
            vec![
                CommonLispReaderConditionalKind::Include,
                CommonLispReaderConditionalKind::Exclude,
            ],
            "{dialect:?}"
        );
    }
}

#[test]
fn reports_dispatch_spans_from_dialect_aware_opaque_forms() {
    let input = "#+sbcl selected #-sbcl rejected";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp)
        .expect("Common Lisp parse succeeds");

    let dispatches = common_lisp_reader_conditional_dispatches(&tree);

    assert_eq!(dispatches.len(), 2);
    assert_eq!(dispatches[0].span.slice(input), "#+");
    assert_eq!(dispatches[1].span.slice(input), "#-");
}
