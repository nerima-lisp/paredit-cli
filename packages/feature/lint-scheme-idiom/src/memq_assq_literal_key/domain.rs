//! `scheme-memq-assq-literal-key` detection: `memq` or `assq` searching for a
//! number or character literal, where R7RS does not say what the answer is.
//!
//! R7RS 6.4 defines `memq`, `memv` and `member` as the same search differing
//! only in the equivalence they use — `memq` uses `eq?`, `memv` uses `eqv?` —
//! and then makes the consequence concrete with two worked examples side by
//! side: `(memq 101 '(100 101 102))` is ⟹ *unspecified*, while
//! `(memv 101 '(100 101 102))` ⟹ `(101 102)`. `assq` and `assv` carry the same
//! pairing for association lists. The report says this outright because `eq?`
//! on a number or a character is exactly what R7RS 6.1 declines to specify: an
//! implementation may box either type, and then the search silently fails to
//! find an element that is present.
//!
//! The repair is mechanical and cannot break a working search. `eqv?` agrees
//! with `eq?` everywhere R7RS 6.1 guarantees `eq?` an answer at all, so
//! `memq`→`memv` and `assq`→`assv` replace an unspecified result with a
//! specified one and leave every specified result unchanged. That is what makes
//! this rule fixable where a comparison rule would not be.
//!
//! Only *literal* keys are reported. `(memq x lst)` says nothing about what `x`
//! holds, and a syntactic rule cannot find out; a literal is the one case where
//! the source itself settles the type.
//!
//! **Scheme only, deliberately.** Racket is excluded even though its `memq` is
//! also `eq?`-based, because Racket *does* specify the two cases R7RS leaves
//! open: fixnums compare `eq?` by guarantee, and characters have been
//! normatively `eq?` since 9.0.0.10. Every finding this rule could produce on
//! Racket would therefore be a complaint about code the language promises will
//! work. This is the concrete reason the package's earlier
//! `eq?`-on-a-literal rule was dropped: measured over 3 MB of real Scheme it
//! produced 15 findings against 462 candidate `eq?` forms and not one of them
//! was a defect, because every one compared a character or a small fixnum.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{LiteralKind, is_unevaluated_at, literal_kind, scheme_atom, scheme_head};

/// The heads this rule anchors on, shared with its `HeadFilter`.
pub const HEADS: [&str; 2] = ["memq", "assq"];

/// The dialects this rule models. Scheme only — see the module documentation.
pub const DIALECTS: [Dialect; 1] = [Dialect::Scheme];

/// The `eqv?`-based operator that specifies what its `eq?`-based counterpart
/// leaves open.
#[must_use]
pub fn replacement_for(head: &str) -> Option<&'static str> {
    match head {
        "memq" => Some("memv"),
        "assq" => Some("assv"),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct MemqAssqLiteralKeyItem {
    /// The span of the whole `(memq … …)` or `(assq … …)` form.
    pub span: ByteSpan,
    /// The span of the head symbol, which a fix replaces.
    pub head_span: ByteSpan,
    /// The operator as written.
    pub head: String,
    /// The operator a fix writes instead.
    pub replacement: &'static str,
    /// Which literal type made the search unspecified.
    pub kind: LiteralKind,
}

impl Finding for MemqAssqLiteralKeyItem {
    fn kind(&self) -> &'static str {
        "scheme-memq-assq-literal-key"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("operator={}", self.head),
            format!("literal={}", self.kind.as_str()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.head)),
            ("replacement", json!(self.replacement)),
            ("literal_kind", json!(self.kind.as_str())),
        ]
    }

    fn message(&self) -> String {
        message_for(&self.head, self.replacement, self.kind)
    }
}

/// The one sentence both the standalone report and the lint rule write.
#[must_use]
pub fn message_for(head: &str, replacement: &str, kind: LiteralKind) -> String {
    format!(
        "{head} on a {} literal is unspecified in R7RS 6.4; {replacement} is the specified search",
        kind.as_str()
    )
}

/// Examines one node.
pub fn examine_memq_assq(
    tree: &SyntaxTree,
    view: &ExpressionView,
    search_form_count: &mut usize,
    violations: &mut Vec<MemqAssqLiteralKeyItem>,
) {
    let Some(head_text) = scheme_head(view) else {
        return;
    };
    let Some(replacement) = replacement_for(head_text) else {
        return;
    };
    // `(memq obj list)` and `(assq obj alist)`: R7RS 6.4 gives both exactly two
    // operands. Any other arity is malformed, which is a different rule's
    // subject.
    let [head, key, _haystack] = view.children.as_slice() else {
        return;
    };
    *search_form_count += 1;

    // The parser keeps a reader prefix in the atom's own text, so `'5` arrives
    // as the two characters `'5`. Quoting a self-evaluating datum changes
    // nothing about it (R7RS 4.1.2), so the quote is stripped before
    // classifying rather than used to dismiss the key.
    let Some(kind) = scheme_atom(key).and_then(|text| literal_kind(text.trim_start_matches('\'')))
    else {
        return;
    };
    // Last: the only non-node-local check. See `crate::support::node_context`.
    if is_unevaluated_at(tree, view.span) {
        return;
    }

    violations.push(MemqAssqLiteralKeyItem {
        span: view.span,
        head_span: head.span,
        head: head_text.to_owned(),
        replacement,
        kind,
    });
}

/// Collects every unspecified `memq`/`assq` search in one file, with the number
/// of two-operand `memq`/`assq` forms scanned as the denominator beside them.
pub fn build_memq_assq_literal_key_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MemqAssqLiteralKeyItem>> {
    if !DIALECTS.contains(&dialect) {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("search_form_count", json!(0))],
        ));
    }

    let mut search_form_count = 0;
    let mut violations = Vec::new();
    let root = tree.root_view();
    let mut stack: Vec<&ExpressionView> = root.children.iter().rev().collect();
    while let Some(view) = stack.pop() {
        examine_memq_assq(tree, view, &mut search_form_count, &mut violations);
        stack.extend(view.children.iter().rev());
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("search_form_count", json!(search_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MemqAssqLiteralKeyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Scheme).expect("parse input");
        build_memq_assq_literal_key_report(Path::new("test.scm"), Dialect::Scheme, &tree)
            .expect("build memq/assq report")
    }

    fn findings(input: &str) -> Vec<MemqAssqLiteralKeyItem> {
        report(input).findings
    }

    fn scanned(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "search_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("search_form_count in the summary")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    /// R7RS 6.4's own worked example.
    #[test]
    fn flags_the_report_s_own_example() {
        let source = "(memq 101 '(100 101 102))";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, LiteralKind::Number);
        assert_eq!(found[0].replacement, "memv");
        assert_eq!(slice(source, found[0].head_span), "memq");
    }

    /// Reduced from `scripts/punify.scm:65` in Guile 3.0.11 — the one finding
    /// this rule produced over its corpus, and a real latent portability bug.
    #[test]
    fn flags_a_character_key_from_the_corpus() {
        let found = findings("(not (memq #\\space ls))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, LiteralKind::Character);
    }

    #[test]
    fn flags_assq_and_offers_assv() {
        let found = findings("(assq 42 table)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].head, "assq");
        assert_eq!(found[0].replacement, "assv");
    }

    #[test]
    fn flags_every_numeric_spelling() {
        for literal in ["5", "-5", "1/2", "#xff", "1e10", "+inf.0", "5.0"] {
            assert_eq!(
                findings(&format!("(memq {literal} lst)")).len(),
                1,
                "{literal} was not recognized"
            );
        }
    }

    /// A quoted self-evaluating datum is itself (R7RS 4.1.2), so `'5` is as
    /// unspecified a key as `5`.
    #[test]
    fn flags_a_quoted_numeric_or_character_key() {
        assert_eq!(findings("(memq '5 lst)").len(), 1);
        assert_eq!(findings("(memq '#\\a lst)")[0].kind, LiteralKind::Character);
    }

    /// Symbols are the case `memq` exists for, and R7RS 6.1 guarantees them.
    #[test]
    fn does_not_flag_the_types_r7rs_guarantees() {
        for key in ["'sym", "#t", "#f", "'()", "x"] {
            assert!(
                findings(&format!("(memq {key} lst)")).is_empty(),
                "{key} must not be reported"
            );
        }
    }

    #[test]
    fn does_not_flag_a_computed_key() {
        assert!(findings("(memq (car x) lst)").is_empty());
        assert_eq!(scanned("(memq (car x) lst)"), 1);
    }

    #[test]
    fn does_not_flag_memv_assv_member_or_assoc() {
        for head in ["memv", "member", "assv", "assoc"] {
            assert_eq!(scanned(&format!("({head} 1 lst)")), 0);
        }
    }

    #[test]
    fn does_not_flag_a_wrong_arity_call() {
        assert_eq!(scanned("(memq 1)"), 0);
        assert_eq!(scanned("(memq 1 lst extra)"), 0);
    }

    #[test]
    fn does_not_flag_a_quoted_shape() {
        assert!(findings("'(memq 1 lst)").is_empty());
        assert!(findings("`(a (memq 1 lst))").is_empty());
    }

    #[test]
    fn does_not_flag_a_vector_constant_that_looks_like_a_call() {
        assert_eq!(scanned("#(memq 1 lst)"), 0);
    }

    #[test]
    fn does_not_case_fold_the_head() {
        assert_eq!(scanned("(MEMQ 1 lst)"), 0);
    }

    #[test]
    fn does_not_flag_a_qualified_head_that_merely_ends_in_memq() {
        assert_eq!(scanned("(srfi:memq 1 lst)"), 0);
    }

    #[test]
    fn a_bracketed_form_is_found() {
        assert_eq!(findings("(cond [(memq 1 lst) #t])").len(), 1);
    }

    /// Racket specifies both cases R7RS leaves open — fixnums by guarantee and
    /// characters since 9.0.0.10 — so every finding there would be a complaint
    /// about code the language promises will work.
    #[test]
    fn racket_is_deliberately_not_modelled() {
        let tree = SyntaxTree::parse_with_dialect("(memq 101 '(100 101 102))", Dialect::Racket)
            .expect("parse");
        let report = build_memq_assq_literal_key_report(Path::new("a.rkt"), Dialect::Racket, &tree)
            .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_dialect_this_rule_does_not_model_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(memq 1 lst)", Dialect::CommonLisp).expect("parse");
        let report =
            build_memq_assq_literal_key_report(Path::new("a.lisp"), Dialect::CommonLisp, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_search_scanned_not_only_the_flagged_ones() {
        assert_eq!(scanned("(memq 1 a)\n(memq 'x a)\n(assq 2 b)\n"), 3);
        assert_eq!(findings("(memq 1 a)\n(memq 'x a)\n(assq 2 b)\n").len(), 2);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("(define (f lst)\n  (memq 101 lst))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "scheme-memq-assq-literal-key");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!("memq")),
                ("replacement", json!("memv")),
                ("literal_kind", json!("number")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["operator=memq".to_owned(), "literal=number".to_owned()]
        );
        assert_eq!(
            finding.message(),
            message_for("memq", "memv", LiteralKind::Number)
        );
    }

    #[test]
    fn the_replacement_table_covers_exactly_the_anchored_heads() {
        for head in HEADS {
            assert!(replacement_for(head).is_some(), "{head} has no replacement");
        }
        assert_eq!(replacement_for("member"), None);
    }
}
