//! What each LSP request means in terms of this tool's existing analyses.
//!
//! Almost nothing here is new work: diagnostics are the lint pass, document
//! symbols are `inspect outline`, formatting is `edit format`, rename is
//! `refactor rename-at`, and code actions are the auto-fixes `inspect lint
//! --fix` would apply. The value is in the mapping, not in the analysis.
//!
//! The exception is `selectionRange`, which has no CLI equivalent and is the
//! request this tool is best placed of any language server to answer: expanding
//! a selection outward through balanced expressions is exactly the tree it
//! already has, and for a Lisp it is the primary way a person navigates.

use serde_json::{Value, json};

use paredit_core_syntax::sexpr::{
    ByteOffset, ByteSpan, ExpressionKind, ExpressionView, SyntaxTree,
};

use super::documents::{Document, PositionEncoding};
use crate::application::usecase::lint_report::{
    LintPassRequest, RuleFilter, Severity, resolve_active_rules, rule_category, rule_description,
    rule_severity, run_lint_pass,
};

/// An LSP `Range` for a byte span.
pub(crate) fn range(document: &Document, span: ByteSpan, encoding: PositionEncoding) -> Value {
    let (start_line, start_character) = document.position_of(span.start().get(), encoding);
    let (end_line, end_character) = document.position_of(span.end().get(), encoding);
    json!({
        "start": { "line": start_line, "character": start_character },
        "end": { "line": end_line, "character": end_character },
    })
}

// ---------------------------------------------------------- diagnostics

/// The document's diagnostics: a parse failure, or the lint findings.
///
/// A parse failure suppresses the lint findings rather than joining them.
/// Nothing downstream of an unbalanced document is trustworthy — the tree the
/// rules would walk is the recovered one, not the one the author is typing —
/// and a screen of spurious findings while a paren is momentarily open is the
/// behaviour that makes people turn a language server off.
pub(crate) fn diagnostics(
    document: &Document,
    path: &std::path::Path,
    encoding: PositionEncoding,
) -> Vec<Value> {
    let tree = match SyntaxTree::parse_with_dialect(&document.text, document.dialect) {
        Ok(tree) => tree,
        Err(error) => {
            return vec![json!({
                "range": range(
                    document,
                    point_at(parse_error_offset(&error, document.text.len())),
                    encoding,
                ),
                "severity": 1,
                "source": "paredit",
                "code": "parse",
                "message": error.to_string(),
            })];
        }
    };

    let active = resolve_active_rules(&RuleFilter::default()).unwrap_or_default();
    let Ok(pass) = run_lint_pass(
        path,
        document.dialect,
        &tree,
        &document.text,
        LintPassRequest {
            active: &active,
            settings: None,
            measure: false,
        },
    ) else {
        return Vec::new();
    };

    pass.findings
        .into_iter()
        .filter(|finding| active.contains(&finding.rule))
        .map(|finding| {
            json!({
                "range": range(document, finding.span, encoding),
                "severity": match rule_severity(finding.rule) {
                    Severity::Error => 1,
                    Severity::Warning => 2,
                },
                "source": "paredit",
                "code": finding.rule,
                "codeDescription": {
                    "href": format!(
                        "https://nerima-lisp.github.io/paredit-cli/commands/#{}",
                        finding.rule,
                    ),
                },
                "message": finding.message,
                "data": { "category": rule_category(finding.rule) },
            })
        })
        .collect()
}

/// A zero-width span at one offset, for a diagnostic that points rather than
/// covers.
const fn point_at(offset: usize) -> ByteSpan {
    ByteSpan::new(ByteOffset::new(offset), ByteOffset::new(offset))
}

/// Where a parse error happened, defaulting to the end of the document.
const fn parse_error_offset(
    error: &paredit_core_syntax::sexpr::ParseError,
    length: usize,
) -> usize {
    use paredit_core_syntax::sexpr::ParseError;
    match error {
        ParseError::UnexpectedClose { position, .. }
        | ParseError::MismatchedClose { position, .. }
        | ParseError::ResourceLimitExceeded { position, .. }
        | ParseError::UnsupportedReaderDispatch { position, .. } => *position,
        ParseError::UnclosedList(position)
        | ParseError::UnterminatedString(position)
        | ParseError::UnterminatedBlockComment(position)
        | ParseError::UnterminatedSymbol(position)
        | ParseError::DanglingSingleEscape(position) => *position,
        _ => length,
    }
}

// ------------------------------------------------------- document symbols

/// `inspect outline`, as a flat list of LSP `SymbolInformation`-shaped
/// `DocumentSymbol`s.
pub(crate) fn document_symbols(
    document: &Document,
    tree: &SyntaxTree,
    encoding: PositionEncoding,
) -> Vec<Value> {
    tree.outline(|head| document.dialect.is_definition_head(head))
        .into_iter()
        .map(|entry| {
            let span = range(document, entry.span, encoding);
            json!({
                "name": entry.head.clone().unwrap_or_else(|| "<form>".to_owned()),
                // 12 is `Function`, 13 is `Variable`. A definition-like head is
                // reported as a function because that is what the vast majority
                // of them are and the outline does not distinguish further; a
                // non-definition top-level form is a variable so the two are
                // visually separable in an outline pane.
                "kind": if entry.definition_like { 12 } else { 13 },
                "range": span,
                "selectionRange": span,
                "detail": entry.path.to_string(),
            })
        })
        .collect()
}

// -------------------------------------------------------- selection range

/// The chain of enclosing expressions at an offset, outermost last.
///
/// This is the request this tool answers better than a generic language server
/// can: for a Lisp, "expand selection" is "go up one list", and the tree
/// already knows. The chain starts at the smallest expression containing the
/// offset and ends at the whole document.
pub(crate) fn selection_chain(tree: &SyntaxTree, offset: usize) -> Vec<ByteSpan> {
    let root = tree.root_view();
    let mut chain = Vec::new();
    collect_containing(&root, offset, &mut chain);
    // The root's span is the document, which is a legitimate final rung: a
    // reader expanding past the outermost form wants the buffer.
    chain.reverse();
    chain
}

fn collect_containing(view: &ExpressionView, offset: usize, chain: &mut Vec<ByteSpan>) {
    if !contains(view.span, offset) {
        return;
    }
    chain.push(view.span);
    for child in &view.children {
        collect_containing(child, offset, chain);
    }
}

/// End-inclusive containment.
///
/// A caret sitting immediately after a form's closing paren is, to a person,
/// on that form — and refusing to expand from there is the single most
/// noticeable way a selection-range implementation feels broken.
const fn contains(span: ByteSpan, offset: usize) -> bool {
    offset >= span.start().get() && offset <= span.end().get()
}

/// Nests the chain into LSP's linked `SelectionRange` shape.
pub(crate) fn selection_range_value(
    document: &Document,
    chain: &[ByteSpan],
    encoding: PositionEncoding,
) -> Value {
    let mut parent = Value::Null;
    for span in chain.iter().rev() {
        let mut node = json!({ "range": range(document, *span, encoding) });
        if !parent.is_null() {
            node["parent"] = parent;
        }
        parent = node;
    }
    parent
}

// --------------------------------------------------------- folding ranges

/// Every multi-line list, as a folding range.
///
/// Atoms and single-line lists are excluded: a fold that hides nothing is
/// clutter in the gutter, and an editor renders a marker for every range it is
/// given.
pub(crate) fn folding_ranges(
    document: &Document,
    tree: &SyntaxTree,
    encoding: PositionEncoding,
) -> Vec<Value> {
    let mut ranges = Vec::new();
    let mut stack = vec![tree.root_view()];
    while let Some(view) = stack.pop() {
        for child in &view.children {
            stack.push(child.clone());
        }
        if view.kind != ExpressionKind::List {
            continue;
        }
        let (start_line, _) = document.position_of(view.span.start().get(), encoding);
        let (end_line, _) = document.position_of(view.span.end().get(), encoding);
        if end_line <= start_line {
            continue;
        }
        ranges.push(json!({
            "startLine": start_line,
            // End-inclusive and one line short of the close, so the closing
            // delimiter stays visible when the range is folded. A fold that
            // hides its own `)` reads as unbalanced.
            "endLine": end_line - 1,
            "kind": "region",
        }));
    }
    ranges.sort_by_key(|value| {
        (
            value["startLine"].as_u64().unwrap_or(0),
            value["endLine"].as_u64().unwrap_or(0),
        )
    });
    ranges
}

// ------------------------------------------------------------ code actions

/// The auto-fixes available inside a range, as `quickfix` code actions.
///
/// Only the fixes whose finding overlaps the requested range: an editor asks
/// for actions at the caret, and answering with every fix in the file would put
/// unrelated repairs behind the lightbulb.
pub(crate) fn code_actions(
    document: &Document,
    path: &std::path::Path,
    uri: &str,
    selected: ByteSpan,
    encoding: PositionEncoding,
) -> Vec<Value> {
    let Ok(tree) = SyntaxTree::parse_with_dialect(&document.text, document.dialect) else {
        return Vec::new();
    };
    let active = resolve_active_rules(&RuleFilter::default()).unwrap_or_default();
    let Ok(pass) = run_lint_pass(
        path,
        document.dialect,
        &tree,
        &document.text,
        LintPassRequest {
            active: &active,
            settings: None,
            measure: false,
        },
    ) else {
        return Vec::new();
    };

    pass.fixes
        .into_iter()
        .filter(|(_, span, _)| overlaps(*span, selected))
        .map(|(rule, span, fix)| {
            let edits = fix
                .replacements()
                .map(|replacement| {
                    json!({
                        "range": range(document, replacement.span(), encoding),
                        "newText": replacement.text(),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "title": format!("{}: {}", rule, fix.description()),
                "kind": "quickfix",
                "diagnostics": [{
                    "range": range(document, span, encoding),
                    "code": rule,
                    "source": "paredit",
                    "message": rule_description(rule).unwrap_or_default(),
                }],
                "edit": { "changes": { uri: edits } },
            })
        })
        .collect()
}

const fn overlaps(left: ByteSpan, right: ByteSpan) -> bool {
    left.start().get() <= right.end().get() && right.start().get() <= left.end().get()
}

// --------------------------------------------------------------- formatting

/// One edit replacing the whole document with its canonical layout.
///
/// A whole-document replacement rather than a minimal edit script: the
/// formatter produces a document, not a patch, and reconstructing a minimal
/// edit from before and after would be a diff implementation whose only
/// customer is the size of the message.
pub(crate) fn formatting_edits(
    document: &Document,
    tree: &SyntaxTree,
    indent: usize,
    encoding: PositionEncoding,
) -> Vec<Value> {
    let formatted =
        paredit_core_syntax::sexpr::Formatter::with_dialect(indent, document.dialect).format(tree);
    if formatted == document.text {
        return Vec::new();
    }
    let whole = ByteSpan::new(
        paredit_core_syntax::sexpr::ByteOffset::new(0),
        paredit_core_syntax::sexpr::ByteOffset::new(document.text.len()),
    );
    vec![json!({ "range": range(document, whole, encoding), "newText": formatted })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn document(text: &str) -> Document {
        Document::new(text.to_owned(), Dialect::CommonLisp, 1)
    }

    /// Expanding a selection outward is what this server exists to do well.
    #[test]
    fn the_selection_chain_grows_one_expression_at_a_time() {
        let source = "(defun f (x) (+ x 1))";
        let tree = SyntaxTree::parse(source).expect("parses");
        let offset = source.find("+ x").expect("the plus") + 2;
        let chain = selection_chain(&tree, offset);
        let texts: Vec<&str> = chain
            .iter()
            .map(|span| &source[span.start().get()..span.end().get()])
            .collect();
        assert_eq!(texts, vec!["x", "(+ x 1)", "(defun f (x) (+ x 1))", source]);
    }

    /// A caret just past a closing paren is, to a person, on that form.
    #[test]
    fn the_chain_includes_a_form_the_caret_sits_immediately_after() {
        let source = "(f (g))";
        let tree = SyntaxTree::parse(source).expect("parses");
        let chain = selection_chain(&tree, source.find(')').expect("a close") + 1);
        let texts: Vec<&str> = chain
            .iter()
            .map(|span| &source[span.start().get()..span.end().get()])
            .collect();
        assert!(texts.contains(&"(g)"), "{texts:?}");
    }

    #[test]
    fn a_selection_range_nests_outward_through_parent_links() {
        let source = "(f (g))";
        let document = document(source);
        let tree = SyntaxTree::parse(source).expect("parses");
        // Offset 4 is the `g` atom; its parent is `(g)` at column 3, whose
        // parent is the whole form.
        let chain = selection_chain(&tree, 4);
        let value = selection_range_value(&document, &chain, PositionEncoding::Utf16);
        assert_eq!(value["range"]["start"]["character"], 4);
        assert_eq!(value["parent"]["range"]["start"]["character"], 3);
        assert_eq!(value["parent"]["parent"]["range"]["start"]["character"], 0);
    }

    /// A fold that hides its own closing paren reads as unbalanced source.
    #[test]
    fn a_folding_range_stops_one_line_above_the_closing_delimiter() {
        let source = "(defun f (x)\n  (+ x\n     1))\n";
        let document = document(source);
        let tree = SyntaxTree::parse(source).expect("parses");
        let ranges = folding_ranges(&document, &tree, PositionEncoding::Utf16);
        assert_eq!(ranges[0]["startLine"], 0);
        assert_eq!(ranges[0]["endLine"], 1);
    }

    #[test]
    fn a_single_line_form_produces_no_fold() {
        let source = "(f (g) (h))\n";
        let document = document(source);
        let tree = SyntaxTree::parse(source).expect("parses");
        assert!(folding_ranges(&document, &tree, PositionEncoding::Utf16).is_empty());
    }

    /// While a paren is momentarily open, every rule sees a recovered tree and
    /// has nothing trustworthy to say. Reporting the parse failure alone is
    /// what keeps a half-typed form from lighting up the whole file.
    #[test]
    fn an_unbalanced_document_reports_only_its_parse_failure() {
        let document = document("(defun f (x)\n  (if (not x) 1 2)\n");
        let diagnostics = diagnostics(
            &document,
            std::path::Path::new("t.lisp"),
            PositionEncoding::Utf16,
        );
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0]["code"], "parse");
        assert_eq!(diagnostics[0]["severity"], 1);
    }

    #[test]
    fn a_lint_finding_becomes_a_diagnostic_carrying_its_rule_name() {
        let document = document("(defun f (x) (if (not x) 1 2))\n");
        let diagnostics = diagnostics(
            &document,
            std::path::Path::new("t.lisp"),
            PositionEncoding::Utf16,
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic["code"] == "negated-if"),
            "{diagnostics:?}"
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic["source"] == "paredit"),
            "{diagnostics:?}"
        );
    }

    /// An editor asks for actions at the caret. Answering with every fix in the
    /// file would put unrelated repairs behind the lightbulb.
    #[test]
    fn code_actions_are_limited_to_the_requested_range() {
        let source = "(defun f (x) (if (not x) 1 2))\n(defun g (y) (progn y))\n";
        let document = document(source);
        let first_definition = ByteSpan::new(
            paredit_core_syntax::sexpr::ByteOffset::new(0),
            paredit_core_syntax::sexpr::ByteOffset::new(10),
        );
        let actions = code_actions(
            &document,
            std::path::Path::new("t.lisp"),
            "file:///t.lisp",
            first_definition,
            PositionEncoding::Utf16,
        );
        assert!(
            actions.iter().all(|action| !action["title"]
                .as_str()
                .unwrap_or_default()
                .contains("progn")),
            "{actions:?}"
        );
    }

    #[test]
    fn formatting_an_already_canonical_document_produces_no_edit() {
        let source = "(defun f (x)\n  (+ x 1))\n";
        let document = document(source);
        let tree = SyntaxTree::parse(source).expect("parses");
        let once = formatting_edits(&document, &tree, 2, PositionEncoding::Utf16);
        let formatted = once
            .first()
            .and_then(|edit| edit["newText"].as_str())
            .unwrap_or(source)
            .to_owned();

        let settled = Document::new(formatted, Dialect::CommonLisp, 2);
        let tree = SyntaxTree::parse(&settled.text).expect("parses");
        assert!(
            formatting_edits(&settled, &tree, 2, PositionEncoding::Utf16).is_empty(),
            "formatting must be idempotent, or every save marks the buffer dirty"
        );
    }
}
