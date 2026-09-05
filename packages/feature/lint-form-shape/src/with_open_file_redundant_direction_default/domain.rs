//! Common Lisp redundant-`:direction`-default detection: an `open` or
//! `with-open-file` given an explicit `:direction :input`.
//!
//! CLHS specifies `open`'s `:direction` as defaulting to `:input`, and
//! `with-open-file` passes its options straight through to `open`. So
//! `(with-open-file (s path :direction :input) …)` is exactly
//! `(with-open-file (s path) …)`; the explicit pair restates the default and
//! adds only noise. Verified against SBCL: opening a file with no `:direction`
//! yields a readable input stream.
//!
//! This is the same family as [`crate::make_array_default_keyword`],
//! [`crate::make_hash_table_test`], [`crate::make_list_default_element`] and
//! [`crate::parse_integer_default_radix`], and disjoint from all four by head.
//!
//! # Why only `:direction :input`
//!
//! It is the one `open` keyword whose default is both unconditional and
//! independent of the others. `:if-exists` and `:if-does-not-exist` have
//! defaults *derived* from `:direction` and from each other, `:element-type`'s
//! default is `character` but writing it explicitly is a documented style
//! choice rather than a restatement of an interaction-free default, and
//! `:external-format`'s default is implementation-defined. Only `:direction`
//! is safe to call redundant on the standard alone.
//!
//! # What this rule deliberately does not flag
//!
//! - **Any other `:direction` value.** `:output`, `:io` and `:probe` all change
//!   behaviour.
//! - **A non-literal value** — `(open p :direction mode)` says nothing.
//! - **A `:direction` in a position that is not a keyword slot.** Options are
//!   read as pairs from the first option index onward, so a *value* that
//!   happens to be the symbol `:direction` is never mistaken for the keyword.
//! - **A form reached only as quoted data.**
//!
//! Report only, unlike the four sibling rules, which do attach a deletion fix.
//! The surrounding `:if-does-not-exist` default is derived from `:direction`,
//! so the rule does not offer an automatic rewrite.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in, unqualified};
use serde_json::{Value, json};

use crate::support::{atom_is, for_each_evaluated_subview};

/// The two operators that take `open`'s keyword options.
pub const STREAM_OPENING_HEADS: [&str; 2] = ["open", "with-open-file"];

#[derive(Debug, Clone)]
pub struct WithOpenFileRedundantDirectionDefaultItem {
    /// The span of the whole `(with-open-file …)` / `(open …)` form.
    pub span: ByteSpan,
    /// The span of the redundant ` :direction :input` pair, for an editor to
    /// select. Published on the report as well as used for the message,
    /// because a consumer applying the edit itself needs the same bytes.
    pub removal_span: ByteSpan,
    /// The operator as written, normalized.
    pub operator: String,
}

impl Finding for WithOpenFileRedundantDirectionDefaultItem {
    fn kind(&self) -> &'static str {
        "with-open-file-redundant-direction-default"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.operator.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            (
                "removal_span",
                json!({
                    "start": self.removal_span.start().get(),
                    "end": self.removal_span.end().get(),
                }),
            ),
        ]
    }

    fn message(&self) -> String {
        format!(
            "explicit :direction :input restates {}'s default; drop it",
            self.operator
        )
    }
}

/// The list whose children hold the keyword options, and the index the options
/// start at.
///
/// `(open filespec option*)` carries them directly; `with-open-file` carries
/// them inside its `(stream filespec option*)` binding list. Both put the first
/// option at index 2 of their respective list.
fn option_carrier<'a>(view: &'a ExpressionView, head: &str) -> Option<&'a ExpressionView> {
    if symbol_in(head, &["open"]) {
        return Some(view);
    }
    let binding = view.children.get(1)?;
    is_paren_list(binding).then_some(binding)
}

/// Cheapest predicate first: the head comparison, then the option list's
/// existence, then a stride-2 walk of the keyword slots. Nothing allocates
/// until a finding is produced.
pub fn examine(
    view: &ExpressionView,
    call_form_count: &mut usize,
    violations: &mut Vec<WithOpenFileRedundantDirectionDefaultItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &STREAM_OPENING_HEADS) {
        return;
    }
    *call_form_count += 1;

    let Some(carrier) = option_carrier(view, head) else {
        return;
    };
    // Options are keyword/value pairs starting after the single positional
    // operand, so only the even offsets from index 2 are keyword slots.
    // Striding avoids reading a *value* that happens to spell `:direction`.
    let mut index = 2;
    while index + 1 < carrier.children.len() {
        let keyword = &carrier.children[index];
        let value = &carrier.children[index + 1];
        if keyword.reader_prefixes.is_empty()
            && atom_is(keyword, ":direction")
            && value.reader_prefixes.is_empty()
            && atom_is(value, ":input")
        {
            violations.push(WithOpenFileRedundantDirectionDefaultItem {
                span: view.span,
                // From the end of the preceding operand through the `:input`,
                // which is exactly the text a deletion would remove.
                removal_span: ByteSpan::new(
                    carrier.children[index - 1].span.end(),
                    value.span.end(),
                ),
                operator: unqualified(head).to_ascii_lowercase(),
            });
            return;
        }
        index += 2;
    }
}

/// Collects every `open`/`with-open-file` in one file that restates
/// `:direction :input`, with the number of such calls scanned as the
/// denominator beside them.
pub fn build_with_open_file_redundant_direction_default_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<WithOpenFileRedundantDirectionDefaultItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("call_form_count", json!(0))],
        ));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine(subview, &mut call_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("call_form_count", json!(call_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::is_unevaluated_at;

    fn report(input: &str) -> FileFindings<WithOpenFileRedundantDirectionDefaultItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_with_open_file_redundant_direction_default_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn fires(source: &str) -> bool {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let mut found = false;
        paredit_core_syntax::view_query::for_each_subview(&root, |view| {
            let mut count = 0;
            let mut items = Vec::new();
            examine(view, &mut count, &mut items);
            if !items.is_empty() && !is_unevaluated_at(&tree, view.span) {
                found = true;
            }
        });
        found
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_with_open_file_input_direction() {
        let source = "(with-open-file (s path :direction :input) (read-line s))";
        let violations = report(source).findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "with-open-file");
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :direction :input"
        );
    }

    #[test]
    fn flags_a_bare_open_call() {
        let source = "(open path :direction :input)";
        let violations = report(source).findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "open");
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :direction :input"
        );
    }

    #[test]
    fn the_removal_keeps_the_other_options() {
        let source = "(with-open-file (s path :direction :input :element-type 'character) x)";
        let violations = report(source).findings;
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :direction :input"
        );
    }

    #[test]
    fn case_and_package_qualifier_fold() {
        assert_eq!(
            report("(CL:WITH-OPEN-FILE (s p :DIRECTION :INPUT) x)")
                .findings
                .len(),
            1
        );
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_output_io_or_probe() {
        for value in [":output", ":io", ":probe"] {
            let source = format!("(with-open-file (s p :direction {value}) x)");
            assert!(report(&source).findings.is_empty(), "{value}");
        }
    }

    #[test]
    fn does_not_flag_a_computed_direction() {
        assert!(
            report("(with-open-file (s p :direction mode) x)")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_form_with_no_direction_at_all() {
        assert!(report("(with-open-file (s p) x)").findings.is_empty());
        assert!(report("(open p)").findings.is_empty());
    }

    /// The stride is what keeps a *value* spelled `:direction` out of a keyword
    /// slot. `:if-exists :direction` is nonsense, but it must not fire.
    #[test]
    fn does_not_read_a_value_slot_as_a_keyword() {
        assert!(
            report("(with-open-file (s p :if-exists :direction :input 1) x)")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_quoted_keyword_pair() {
        assert!(
            report("(with-open-file (s p ':direction ':input) x)")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_an_unrelated_head() {
        assert!(
            report("(with-input-from-string (s p :direction :input) x)")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_with_open_file_whose_binding_is_not_a_list() {
        assert!(report("(with-open-file s x)").findings.is_empty());
    }

    // -- the five quote shapes ------------------------------------------------

    #[test]
    fn plain_code_fires() {
        assert!(fires("(open p :direction :input)"));
    }

    #[test]
    fn a_hard_quoted_form_is_silent() {
        assert!(!fires("'(open p :direction :input)"));
    }

    #[test]
    fn a_long_hand_quote_form_is_silent() {
        assert!(!fires("(quote (open p :direction :input))"));
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_silent() {
        assert!(!fires("'(a ,(open p :direction :input))"));
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_fires() {
        assert!(fires("`(a ,(open p :direction :input))"));
    }

    #[test]
    fn a_backquoted_template_is_silent() {
        assert!(!fires("(defmacro m (p) `(open ,p :direction :input))"));
    }

    // -- string literal -------------------------------------------------------

    #[test]
    fn a_form_spelled_only_inside_a_string_is_not_a_form() {
        let source = "(format t \"(open p :direction :input)\")";
        assert!(report(source).findings.is_empty());
        assert!(!fires(source));
    }

    // -- report envelope ------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(open p :direction :input)", Dialect::Clojure)
            .expect("parse");
        let report = build_with_open_file_redundant_direction_default_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report("(open a :direction :input)\n(open b)\n(with-open-file (s c) 1)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_operator() {
        let report = report("(defun f (p)\n  (open p :direction :input))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "with-open-file-redundant-direction-default");
        assert_eq!(finding.text_columns(), vec!["open".to_owned()]);
    }
}
