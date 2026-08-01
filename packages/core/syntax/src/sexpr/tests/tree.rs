use super::*;

#[test]
fn source_round_trips_the_exact_parsed_text() {
    for input in [
        "(defun add (x y) (+ x y))",
        "(message \"foo\") ; trailing comment\n(foo foo)",
        "'(a b c) #'f #(1 2)",
    ] {
        let tree = SyntaxTree::parse(input).expect("valid");
        assert_eq!(tree.source(), input);
    }
}

#[test]
fn selects_by_path() {
    let input = "(defun add (x y) (+ x y))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.2")).expect("selection");
    assert_eq!(selection.text(), "(x y)");
}

#[test]
fn selects_by_offset() {
    let input = "(alpha (beta gamma))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_at(9).expect("selection");
    assert_eq!(selection.text(), "beta");
}

/// `--at` is a number the user types, so it is not bounded by the document.
///
/// The offset below is one past `u32::MAX`, which `ByteOffset::new` asserts
/// against: reaching it aborted the process with a Rust panic message and exit
/// code 101, where every other unreachable offset produces
/// `selection.offset-not-found` and exit code 1.
#[test]
fn an_offset_beyond_the_document_is_refused_rather_than_panicking() {
    let input = "(alpha (beta gamma))";
    let tree = SyntaxTree::parse(input).expect("valid");

    for offset in [
        input.len() + 1,
        9_999,
        usize::try_from(u32::MAX).expect("64-bit"),
        usize::try_from(u32::MAX).expect("64-bit") + 1,
        4_294_967_296,
        usize::MAX,
    ] {
        let error = tree
            .select_at(offset)
            .expect_err("no expression contains an offset past the source");
        assert!(
            matches!(
                error,
                SexprError::Selection(SelectionError::NoExpressionAtOffset { offset: reported })
                    if reported == offset
            ),
            "offset {offset} should be reported as not found, got {error:?}"
        );
    }
}

/// The guard added above must not reject an offset that does resolve.
#[test]
fn offsets_inside_the_document_still_select() {
    let input = "(alpha (beta gamma))";
    let tree = SyntaxTree::parse(input).expect("valid");

    assert_eq!(tree.select_at(1).expect("selection").text(), "alpha");
    assert_eq!(tree.select_at(9).expect("selection").text(), "beta");
    assert_eq!(tree.select_at(7).expect("selection").text(), "(beta gamma)");
    // The last byte of the source, and one past it: `len()` is the exclusive
    // end of every span, so it selects nothing, but it is on the allowed side
    // of the bound and must reach the search rather than the guard.
    assert_eq!(
        tree.select_at(input.len() - 1).expect("selection").text(),
        "(alpha (beta gamma))"
    );
    assert!(tree.select_at(input.len()).is_err());
}

#[test]
fn outlines_top_level_forms() {
    let input = "(defun add (x y) (+ x y))\n(defvar *x* 1)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let outline = tree.outline(|head| head.starts_with("def"));
    assert_eq!(outline.len(), 2);
    assert_eq!(outline[0].path.to_string(), "0");
    assert_eq!(outline[0].head.as_deref(), Some("defun"));
    assert!(outline[0].definition_like);
}

#[test]
fn finds_atoms_without_comments_or_string_contents() {
    let input = "(message \"foo\") ; foo\n(foo foo)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let paths = tree
        .atom_occurrences()
        .into_iter()
        .filter(|occurrence| occurrence.text == "foo")
        .map(|occurrence| occurrence.path.to_string())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec!["1.0", "1.1"]);
}

#[test]
fn renames_symbols_without_touching_strings_or_comments() {
    let input = "(message \"foo\") ; foo\n(foo foo)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let output = tree.rename_symbol(
        &SymbolName::new("foo").unwrap(),
        &SymbolName::new("bar").unwrap(),
    );
    assert_eq!(output, "(message \"foo\") ; foo\n(bar bar)");
}

#[test]
fn renames_unqualified_occurrences_of_package_qualified_symbol() {
    let input = "(defun cl-user:foo () foo)\n(foo cl-user:foo)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let output = tree.rename_symbol(
        &SymbolName::new("cl-user:foo").unwrap(),
        &SymbolName::new("bar").unwrap(),
    );
    assert_eq!(output, "(defun bar () bar)\n(bar bar)");
}

#[test]
fn treats_reader_prefix_as_part_of_selection_span() {
    let input = "'(alpha beta)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_at(0).expect("selection");
    assert_eq!(selection.text(), "'(alpha beta)");
}

#[test]
fn atom_occurrences_excludes_bare_quoted_symbol_designators() {
    // Low-level `atom_occurrences` treats a bare `'foo` as inert data and
    // excludes it; consumers that need quote-awareness (unused-definition,
    // impact, analysis reports) use their own, more precise reference
    // collectors instead of relying on this blanket exclusion.
    let input = "'foo foo #'foo";
    let tree = SyntaxTree::parse(input).expect("valid");
    let texts = tree
        .atom_occurrences()
        .into_iter()
        .map(|occurrence| occurrence.text)
        .collect::<Vec<_>>();
    assert_eq!(texts, vec!["foo", "foo"]);
}

#[test]
fn rename_symbol_renames_bare_quoted_symbol_designators() {
    // `rename-symbol` is a blunt, tree-wide rename, and `'foo` is the
    // standard Common Lisp idiom for referencing a symbol as data (e.g.
    // `(error 'foo ...)`, `(typep x 'foo)`, `(make-instance 'foo)`), so it is
    // rewritten too -- otherwise the rename would silently leave behind a
    // reference to a definition that no longer exists. `#'foo` is a live
    // function reference per CLHS 3.1.2.1.2.4 (equivalent to `(function
    // foo)`), so it is renamed along with the bare occurrence.
    let input = "'foo foo #'foo";
    let tree = SyntaxTree::parse(input).expect("valid");
    let output = tree.rename_symbol(
        &SymbolName::new("foo").unwrap(),
        &SymbolName::new("bar").unwrap(),
    );
    assert_eq!(output, "'bar bar #'bar");
}

#[test]
fn does_not_rename_atoms_inside_reader_eval_forms() {
    let input = "#.(foo (bar foo)) foo";
    let tree = SyntaxTree::parse(input).expect("valid");
    let output = tree.rename_symbol(
        &SymbolName::new("foo").unwrap(),
        &SymbolName::new("bar").unwrap(),
    );
    assert_eq!(output, "#.(foo (bar foo)) bar");
}

#[test]
fn deeply_nested_rename_occurrences_and_expression_views_do_not_overflow() {
    const DEPTH: usize = 10_001;

    let input = format!("{}target{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
    let tree = SyntaxTree::parse(&input).expect("valid deeply nested input");
    let occurrences = tree.atom_occurrences();

    assert_eq!(tree.atom_occurrence_count(), 1);
    assert_eq!(occurrences.len(), 1);
    assert_eq!(occurrences[0].text, "target");
    assert_eq!(occurrences[0].path.indexes().len(), DEPTH + 1);
    drop(tree.root_view());

    let quoted_input = format!("{}'target{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
    let quoted_tree = SyntaxTree::parse(&quoted_input).expect("valid deeply nested quoted input");
    let quoted = quoted_tree.quoted_symbol_designator_occurrences();

    assert_eq!(quoted.len(), 1);
    assert_eq!(quoted[0].text, "target");
    assert_eq!(quoted[0].path.indexes().len(), DEPTH + 1);
    drop(quoted_tree.root_view());
}

#[test]
fn deeply_nested_expression_view_traits_do_not_overflow() {
    const DEPTH: usize = 30_000;

    let input = format!("{}target{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
    let tree = SyntaxTree::parse(&input).expect("valid deeply nested input");
    let view = tree.root_view();
    let cloned = view.clone();

    assert!(view == cloned);
    let debug = format!("{cloned:?}");
    assert!(debug.contains("target"));
}

#[test]
fn atom_occurrence_index_handles_an_atom_at_every_nested_level() {
    const DEPTH: usize = 4_096;

    let input = format!("{}leaf{}", "(target ".repeat(DEPTH), ")".repeat(DEPTH));
    let tree = SyntaxTree::parse(&input).expect("valid deeply nested input");
    let index = tree.atom_occurrence_index();

    assert_eq!(index.occurrences().len(), DEPTH + 1);
    assert_eq!(index.occurrences().first().unwrap().text, "target");
    assert_eq!(index.occurrences().last().unwrap().text, "leaf");
    let first_path = index
        .path_for_span(index.occurrences().first().unwrap().span)
        .unwrap();
    let last_path = index
        .path_for_span(index.occurrences().last().unwrap().span)
        .unwrap();
    assert_eq!(first_path.to_raw_indexes(), [0, 0]);
    let last_indexes = last_path.to_raw_indexes();
    assert_eq!(last_indexes.len(), DEPTH + 1);
    assert_eq!(last_indexes[0], 0);
    assert!(last_indexes[1..].iter().all(|index| *index == 1));
}

#[test]
fn atom_occurrence_index_handles_ten_thousand_sibling_atoms() {
    const WIDTH: usize = 10_000;

    let input = format!("({})", "target ".repeat(WIDTH));
    let tree = SyntaxTree::parse(&input).expect("valid wide input");
    let index = tree.atom_occurrence_index();

    assert_eq!(index.occurrences().len(), WIDTH);
    let first_path = index
        .path_for_span(index.occurrences().first().unwrap().span)
        .unwrap();
    let last_path = index
        .path_for_span(index.occurrences().last().unwrap().span)
        .unwrap();
    assert_eq!(first_path.to_raw_indexes(), [0, 0]);
    assert_eq!(last_path.to_raw_indexes(), [0, WIDTH - 1]);
}

#[test]
fn rename_symbol_handles_a_deep_comb_without_materializing_paths() {
    const DEPTH: usize = 10_000;

    let input = format!("{}leaf{}", "(target ".repeat(DEPTH), ")".repeat(DEPTH));
    let tree = SyntaxTree::parse(&input).expect("valid deeply nested input");
    let renamed = tree.rename_symbol(
        &SymbolName::new("target").unwrap(),
        &SymbolName::new("renamed").unwrap(),
    );

    assert_eq!(renamed.matches("renamed").count(), DEPTH);
    assert!(!renamed.contains("target"));
    SyntaxTree::parse(&renamed).expect("renamed deep tree parses again");
}
