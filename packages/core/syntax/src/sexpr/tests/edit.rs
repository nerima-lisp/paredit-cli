use super::*;
use crate::dialect::Dialect;

#[test]
fn replaces_expression() {
    let input = "(alpha beta gamma)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.1")).expect("selection");
    assert_eq!(
        Edit::replace(input, selection, "delta").unwrap(),
        "(alpha delta gamma)"
    );
}

#[test]
fn edit_rejects_input_that_does_not_match_selection_source() {
    let source = "(alpha beta)";
    let tree = SyntaxTree::parse(source).expect("valid");
    let selection = tree.select_path(&parse_path("0.1")).expect("selection");

    assert!(Edit::replace("(alpha zeta)", selection, "delta").is_err());
    assert!(Edit::wrap("(alpha zeta)", &tree, selection, Delimiter::Paren).is_err());
}

#[test]
fn edit_rejects_input_with_incompatible_utf8_boundaries() {
    let source = "(a x)";
    let tree = SyntaxTree::parse(source).expect("valid");
    let selection = tree.select_path(&parse_path("0.1")).expect("selection");

    assert!(Edit::replace("(é x)", selection, "delta").is_err());
}

#[test]
fn edit_rejects_corrupted_selection_span_without_panicking() {
    let source = "(alpha beta)";
    let mut tree = SyntaxTree::parse(source).expect("valid");
    let selected_id = tree
        .select_path(&parse_path("0.1"))
        .expect("selection")
        .node_id;
    tree.nodes[selected_id.get()].span = ByteSpan::new(ByteOffset::new(9), ByteOffset::new(7));
    let selection = Selection {
        tree: &tree,
        node_id: selected_id,
    };

    assert!(Edit::replace(source, selection, "delta").is_err());
    assert!(Edit::kill(source, &tree, selection).is_err());
}

#[test]
fn edit_rejects_selection_from_a_different_tree() {
    let source = "(alpha beta)";
    let first = SyntaxTree::parse(source).expect("valid");
    let second = SyntaxTree::parse(source).expect("valid");
    let selection = first.select_path(&parse_path("0.1")).expect("selection");

    assert!(Edit::kill(source, &second, selection).is_err());
}

#[test]
fn wraps_expression() {
    let input = "(alpha beta)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.1")).expect("selection");
    assert_eq!(
        Edit::wrap(input, &tree, selection, Delimiter::Paren).unwrap(),
        "(alpha (beta))"
    );
}

#[test]
fn wraps_expression_with_square_and_curly_delimiters() {
    let input = "(alpha beta)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.1")).expect("selection");
    assert_eq!(
        Edit::wrap(input, &tree, selection, Delimiter::Bracket).unwrap(),
        "(alpha [beta])"
    );
    assert_eq!(
        Edit::wrap(input, &tree, selection, Delimiter::Brace).unwrap(),
        "(alpha {beta})"
    );
}

#[test]
fn splices_list() {
    let input = "(alpha (beta gamma) delta)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.1")).expect("selection");
    assert_eq!(
        Edit::splice(input, &tree, selection).unwrap(),
        "(alpha beta gamma delta)"
    );
}

#[test]
fn raises_expression() {
    let input = "(alpha (beta gamma) delta)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.1.1")).expect("selection");
    assert_eq!(
        Edit::raise(input, &tree, selection).unwrap(),
        "(alpha gamma delta)"
    );
}

#[test]
fn transposes_expression_forward_while_keeping_trivia_in_place() {
    let input = "(alpha  ;; slot comment\n beta gamma)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.0")).expect("selection");
    let result = Edit::transpose_forward(input, &tree, selection).unwrap();
    assert_eq!(result, "(beta  ;; slot comment\n alpha gamma)");
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn transposes_expression_backward_while_keeping_trivia_in_place() {
    let input = "(alpha  ;; slot comment\n beta gamma)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.1")).expect("selection");
    let result = Edit::transpose_backward(input, &tree, selection).unwrap();
    assert_eq!(result, "(beta  ;; slot comment\n alpha gamma)");
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn transpose_rejects_sibling_boundaries() {
    let input = "(alpha beta)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let first = tree.select_path(&parse_path("0.0")).expect("selection");
    let last = tree.select_path(&parse_path("0.1")).expect("selection");
    assert!(Edit::transpose_backward(input, &tree, first).is_err());
    assert!(Edit::transpose_forward(input, &tree, last).is_err());
}

#[test]
fn slurps_forward() {
    let input = "(alpha beta) gamma";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0")).expect("selection");
    assert_eq!(
        Edit::slurp_forward(input, &tree, selection).unwrap(),
        "(alpha beta gamma)"
    );
}

#[test]
fn slurps_forward_preserves_trailing_newline() {
    let input = "(foo) bar\n";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0")).expect("selection");
    assert_eq!(
        Edit::slurp_forward(input, &tree, selection).unwrap(),
        "(foo bar)\n"
    );
}

#[test]
fn slurps_forward_keeps_following_sibling_separator() {
    let input = "(foo) bar baz\n";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0")).expect("selection");
    assert_eq!(
        Edit::slurp_forward(input, &tree, selection).unwrap(),
        "(foo bar) baz\n"
    );
}

#[test]
fn barfs_forward() {
    let input = "(alpha beta gamma)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0")).expect("selection");
    assert_eq!(
        Edit::barf_forward(input, &tree, selection).unwrap(),
        "(alpha beta) gamma"
    );
}

#[test]
fn slurps_backward() {
    let input = "alpha (beta gamma)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("1")).expect("selection");
    assert_eq!(
        Edit::slurp_backward(input, &tree, selection).unwrap(),
        "(alpha beta gamma)"
    );
}

#[test]
fn barfs_backward() {
    let input = "(alpha beta gamma)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0")).expect("selection");
    assert_eq!(
        Edit::barf_backward(input, &tree, selection).unwrap(),
        "alpha (beta gamma)"
    );
}

#[test]
fn kills_last_child() {
    let input = "(defun f (x)\n  (* x x))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.3")).expect("selection");
    assert_eq!(
        Edit::kill(input, &tree, selection).unwrap(),
        "(defun f (x))"
    );
}

#[test]
fn kills_last_child_without_swallowing_preceding_comment_newline() {
    let input = "(defun f (x)\n  ;; important comment\n  (* x x))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.3")).expect("selection");
    let result = Edit::kill(input, &tree, selection).unwrap();
    assert_eq!(result, "(defun f (x)\n  ;; important comment\n)");
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn slurps_forward_without_swallowing_preceding_comment_newline() {
    let input = "(let ((a 1))\n  (foo a)\n  ;; note\n  (bar a))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.2")).expect("selection");
    let result = Edit::slurp_forward(input, &tree, selection).unwrap();
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn barfs_forward_without_swallowing_preceding_comment_newline() {
    let input = "(list a\n  ;; last item\n  b)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0")).expect("selection");
    let result = Edit::barf_forward(input, &tree, selection).unwrap();
    assert_eq!(result, "(list a\n  ;; last item\n) b");
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn normalizes_trailing_trivia_only_on_changed_lines() {
    let input = "(alpha beta) \n(unchanged) \n";
    let rewritten = "(alpha gamma) \n(unchanged) \n".to_owned();

    assert_eq!(
        Edit::normalize_changed_line_trivia(input, rewritten, Dialect::CommonLisp).unwrap(),
        "(alpha gamma)\n(unchanged) \n"
    );
}

#[test]
fn preserves_trailing_spaces_inside_multiline_atoms_and_block_comments() {
    let input = "(print \"old  \nvalue\")\n#| old  \ncomment |#\n";
    let rewritten = "(print \"new  \nvalue\")\n#| new  \ncomment |#\n".to_owned();

    assert_eq!(
        Edit::normalize_changed_line_trivia(input, rewritten.clone(), Dialect::CommonLisp).unwrap(),
        rewritten
    );
}

#[test]
fn preserves_changed_line_trivia_inside_clojure_discard() {
    let input = "#_(old  \n  \\)) kept\n";
    let rewritten = "#_(new  \n  \\)) kept\n".to_owned();

    assert_eq!(
        Edit::normalize_changed_line_trivia(input, rewritten.clone(), Dialect::Clojure).unwrap(),
        rewritten
    );
}

#[test]
fn preserves_changed_line_trivia_inside_clojure_tagged_literal() {
    let input = "#inst  \n\"1985-04-12T23:20:50.52-00:00\"\n";
    let rewritten = "#inst   \n\"1986-04-12T23:20:50.52-00:00\"\n".to_owned();

    assert_eq!(
        Edit::normalize_changed_line_trivia(input, rewritten.clone(), Dialect::Clojure).unwrap(),
        rewritten
    );
}

#[test]
fn preserves_changed_line_trivia_inside_common_lisp_dispatch() {
    let input = "#S  \n(point :x 1)\n";
    let rewritten = "#S   \n(point :x 2)\n".to_owned();

    assert_eq!(
        Edit::normalize_changed_line_trivia(input, rewritten.clone(), Dialect::CommonLisp).unwrap(),
        rewritten
    );
}

#[test]
fn splits_enclosing_list_before_selection() {
    let input = "(foo bar baz qux)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.2")).expect("selection");
    let result = Edit::split(input, &tree, selection).unwrap();
    assert_eq!(result, "(foo bar) (baz qux)");
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn split_preserves_delimiter_and_gap_comment() {
    let input = "[a b ;; note\n c]";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.2")).expect("selection");
    let result = Edit::split(input, &tree, selection).unwrap();
    assert_eq!(result, "[a b] ;; note\n [c]");
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn split_rejects_first_element_and_top_level() {
    let input = "(a b c)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let first = tree.select_path(&parse_path("0.0")).expect("selection");
    assert!(Edit::split(input, &tree, first).is_err());
    let top = tree.select_path(&parse_path("0")).expect("selection");
    assert!(Edit::split(input, &tree, top).is_err());
}

#[test]
fn joins_selected_list_with_next_sibling_list() {
    let input = "(foo bar) (baz qux)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0")).expect("selection");
    let result = Edit::join(input, &tree, selection).unwrap();
    assert_eq!(result, "(foo bar baz qux)");
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn join_inserts_separator_between_adjacent_lists() {
    let input = "(a)(b)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0")).expect("selection");
    assert_eq!(Edit::join(input, &tree, selection).unwrap(), "(a b)");
}

#[test]
fn joins_adjacent_string_literals() {
    let input = "(concat \"foo\" \"bar\")";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.1")).expect("selection");
    let result = Edit::join(input, &tree, selection).unwrap();
    assert_eq!(result, "(concat \"foobar\")");
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn join_preserves_escaped_quotes_when_merging_strings() {
    let input = "(list \"a\\\"\" \"b\")";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.1")).expect("selection");
    assert_eq!(
        Edit::join(input, &tree, selection).unwrap(),
        "(list \"a\\\"b\")"
    );
}

#[test]
fn join_refuses_to_fuse_bare_symbols() {
    let input = "(foo bar)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.0")).expect("selection");
    assert!(Edit::join(input, &tree, selection).is_err());
}

#[test]
fn join_rejects_mismatched_delimiters_and_non_list_siblings() {
    let mismatched = "(a) [b]";
    let tree = SyntaxTree::parse(mismatched).expect("valid");
    let selection = tree.select_path(&parse_path("0")).expect("selection");
    assert!(Edit::join(mismatched, &tree, selection).is_err());

    let non_list = "(a) b";
    let tree = SyntaxTree::parse(non_list).expect("valid");
    let selection = tree.select_path(&parse_path("0")).expect("selection");
    assert!(Edit::join(non_list, &tree, selection).is_err());
}

#[test]
fn split_then_join_round_trips() {
    let input = "(foo bar baz qux)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.2")).expect("selection");
    let split = Edit::split(input, &tree, selection).unwrap();
    assert_eq!(split, "(foo bar) (baz qux)");

    let split_tree = SyntaxTree::parse(&split).expect("valid");
    let first = split_tree.select_path(&parse_path("0")).expect("selection");
    assert_eq!(Edit::join(&split, &split_tree, first).unwrap(), input);
}

#[test]
fn splice_killing_backward_keeps_selection_and_following_siblings() {
    let input = "(let ((x 5)) (foo x) bar)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.2")).expect("selection");
    let result = Edit::splice_killing_backward(input, &tree, selection).unwrap();
    assert_eq!(result, "(foo x) bar");
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn splice_killing_forward_keeps_preceding_siblings() {
    let input = "(foo (bar) baz qux)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.2")).expect("selection");
    let result = Edit::splice_killing_forward(input, &tree, selection).unwrap();
    assert_eq!(result, "foo (bar)");
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn splice_killing_forward_rejects_first_child() {
    let input = "(foo bar)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.0")).expect("selection");
    assert!(Edit::splice_killing_forward(input, &tree, selection).is_err());
}

#[test]
fn convolutes_two_enclosing_lists() {
    let input = "(let ((x 1)) (foo (bar baz) quux))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let selection = tree.select_path(&parse_path("0.2.1")).expect("selection");
    let result = Edit::convolute(input, &tree, selection).unwrap();
    assert_eq!(result, "(foo (let ((x 1)) (bar baz)) quux)");
    SyntaxTree::parse(&result).expect("result stays balanced");
}

#[test]
fn convolute_rejects_shallow_nesting_and_gap_comments() {
    let shallow = "(foo (bar))";
    let tree = SyntaxTree::parse(shallow).expect("valid");
    let selection = tree.select_path(&parse_path("0.1")).expect("selection");
    assert!(Edit::convolute(shallow, &tree, selection).is_err());

    let commented = "(outer ;; keep me\n (mid (inner)))";
    let tree = SyntaxTree::parse(commented).expect("valid");
    let selection = tree.select_path(&parse_path("0.1.1")).expect("selection");
    assert!(Edit::convolute(commented, &tree, selection).is_err());
}

#[test]
fn unknown_dialect_keeps_generic_compatibility_and_rejects_malformed_changes() {
    let normalized = Edit::normalize_changed_line_trivia(
        "(alpha beta)  \n",
        "(alpha gamma)  \n".to_owned(),
        Dialect::Unknown,
    )
    .expect("generic changed document remains supported");
    assert_eq!(normalized, "(alpha gamma)\n");

    assert!(
        Edit::normalize_changed_line_trivia(
            "(alpha beta)",
            "(alpha gamma".to_owned(),
            Dialect::Unknown,
        )
        .is_err()
    );
}
