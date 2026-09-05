//! Common Lisp undefined-`go`-tag detection: a `(go tag)` no enclosing
//! tagbody establishes `tag` in.
//!
//! `go` is lexical (CLHS 5.3 `go`): "go transfers control to the point in the
//! body of an enclosing tagbody form labelled by a tag eql to tag". Two
//! consequences shape this rule:
//!
//! - **Every** enclosing tagbody is searched, not just the innermost.
//!   `(tagbody top (tagbody (go top)))` is legal, and a rule that only read
//!   the nearest tagbody would flag it.
//! - The enclosing tagbody need not be spelled `tagbody`. `do`, `do*`,
//!   `prog`, `prog*`, `dolist`, `dotimes` and the three `do-…-symbols` macros
//!   all have an implicit tagbody body, and a tag written in one of those is a
//!   real tag. `loop` is the exception and is deliberately not on the list: a
//!   `loop` body is a sequence of clauses, not a tagbody.
//!
//! Tags are compared the way `go` compares them, with `eql`: integers
//! numerically (`007` and `7` are one tag) and symbols by name, case- and
//! package-insensitively, since the reader upcases them.
//!
//! # Why this reports so little
//!
//! The walk outward stops — reporting nothing — at any head that is not a
//! standard Common Lisp operator. `(with-retry-loop (go retry))` is a good
//! program if `with-retry-loop` expands to `(tagbody retry …)`, and this file
//! cannot see that it does. A false negative there is the deliberate price of
//! never flagging one.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{BlockScope, Tag, block_scope, direct_tags, with_lexical_chain};

#[derive(Debug, Clone)]
pub struct GoToUndefinedTagItem {
    /// The span of the whole `(go …)` form.
    pub span: ByteSpan,
    /// The tag it names, as it reads in a report.
    pub tag: String,
}

impl Finding for GoToUndefinedTagItem {
    fn kind(&self) -> &'static str {
        "go-to-undefined-tag"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("tag={}", self.tag)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("tag", json!(self.tag))]
    }

    fn message(&self) -> String {
        message_for(&self.tag)
    }
}

/// The one sentence both the report and the lint rule phrase a finding with.
#[must_use]
pub fn message_for(tag: &str) -> String {
    format!(
        "go names the tag `{tag}`, which no enclosing tagbody establishes; \
         a go tag is lexical"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolution {
    Established,
    Unestablished,
    Unknown,
}

fn resolve(tree: &SyntaxTree, span: ByteSpan, tag: &Tag) -> Resolution {
    with_lexical_chain(tree, span, |chain| {
        if chain.unevaluated {
            return Resolution::Unknown;
        }
        for index in chain.ancestors_inward() {
            let Some(node) = chain.nodes.get(index) else {
                return Resolution::Unknown;
            };
            if direct_tags(node)
                .iter()
                .any(|(established, _)| established == tag)
            {
                return Resolution::Established;
            }
            // Every implicit-tagbody form is a standard operator this table
            // knows, so an `Unknown` here is a macro whose expansion could
            // hold the tag.
            if block_scope(&chain.nodes, index) == BlockScope::Unknown {
                return Resolution::Unknown;
            }
        }
        Resolution::Unestablished
    })
    .unwrap_or(Resolution::Unknown)
}

pub fn examine_go(
    tree: &SyntaxTree,
    view: &ExpressionView,
    go_form_count: &mut usize,
    violations: &mut Vec<GoToUndefinedTagItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "go")) {
        return;
    }
    *go_form_count += 1;

    // `(go tag)` is the only shape CLHS defines. A `go` with no tag or two is
    // malformed, and a computed one — `(go (compute))` — names no tag this
    // analysis can read.
    if view.children.len() != 2 {
        return;
    }
    let Some(tag) = view.children.get(1).and_then(Tag::read) else {
        return;
    };

    if resolve(tree, view.span, &tag) == Resolution::Unestablished {
        violations.push(GoToUndefinedTagItem {
            span: view.span,
            tag: tag.display(),
        });
    }
}

/// Collects every `go` to an undefined tag in one file, with the number of
/// `go` forms scanned as the denominator beside them.
pub fn build_go_to_undefined_tag_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<GoToUndefinedTagItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("go_form_count", json!(0))],
        ));
    }

    let mut go_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_go(tree, subview, &mut go_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("go_form_count", json!(go_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<GoToUndefinedTagItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_go_to_undefined_tag_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build report")
    }

    fn tags(input: &str) -> Vec<String> {
        report(input)
            .findings
            .into_iter()
            .map(|item| item.tag)
            .collect()
    }

    // -- positive -----------------------------------------------------------

    #[test]
    fn flags_a_go_to_a_tag_the_tagbody_does_not_establish() {
        assert_eq!(tags("(tagbody start (go finish))"), vec!["finish"]);
    }

    #[test]
    fn flags_a_go_with_no_enclosing_tagbody_at_all() {
        assert_eq!(tags("(defun f () (go top))"), vec!["top"]);
    }

    /// A tag in a *nested* tagbody is not visible from outside it.
    #[test]
    fn flags_a_go_to_a_tag_of_a_nested_tagbody() {
        assert_eq!(
            tags("(tagbody (tagbody inner (foo)) (go inner))"),
            vec!["inner"]
        );
    }

    #[test]
    fn flags_a_go_to_an_integer_tag_that_does_not_exist() {
        assert_eq!(tags("(tagbody 1 (go 2))"), vec!["2"]);
    }

    /// A `loop` body is clauses, not a tagbody, so `next` is no tag.
    #[test]
    fn flags_a_go_inside_a_loop_body() {
        assert_eq!(tags("(loop do (go next))"), vec!["next"]);
    }

    // -- near-miss negatives ------------------------------------------------

    #[test]
    fn does_not_flag_a_go_to_a_tag_of_its_own_tagbody() {
        assert_eq!(
            tags("(tagbody start (foo) (go start))"),
            Vec::<String>::new()
        );
    }

    /// CLHS 5.3: `go` reaches *any* enclosing tagbody, not just the nearest.
    #[test]
    fn does_not_flag_a_go_from_a_nested_tagbody_to_an_outer_tag() {
        assert!(tags("(tagbody top (tagbody (go top)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_go_to_a_tag_of_an_implicit_tagbody() {
        for source in [
            "(prog () start (go start))",
            "(prog* () start (go start))",
            "(do ((i 0)) ((= i 3)) start (go start))",
            "(do* ((i 0)) ((= i 3)) start (go start))",
            "(dolist (x l) start (go start))",
            "(dotimes (i 3) start (go start))",
            "(do-symbols (s) start (go start))",
        ] {
            assert!(tags(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn does_not_flag_a_go_to_an_integer_tag_written_with_leading_zeroes() {
        assert!(tags("(tagbody 007 (go 7))").is_empty());
    }

    #[test]
    fn does_not_flag_a_go_from_deep_inside_the_body() {
        assert!(tags("(tagbody again (when (foo) (let ((x 1)) (go again))))").is_empty());
    }

    #[test]
    fn does_not_flag_a_go_under_an_unknown_macro() {
        assert!(tags("(with-retry-loop (go retry))").is_empty());
        assert!(tags("(tagbody start (with-thing (go finish)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_malformed_go() {
        assert!(tags("(go)").is_empty());
        assert!(tags("(go a b)").is_empty());
        assert!(tags("(go (compute-tag))").is_empty());
        assert!(tags("(go 'a)").is_empty());
    }

    #[test]
    fn case_folds_and_ignores_the_package_qualifier() {
        assert!(tags("(tagbody START (CL:GO start))").is_empty());
        assert_eq!(tags("(tagbody START (GO FINISH))"), vec!["finish"]);
    }

    // -- the five quote shapes ---------------------------------------------

    #[test]
    fn does_not_flag_a_hard_quoted_form() {
        assert!(tags("'(go nowhere)").is_empty());
    }

    #[test]
    fn does_not_flag_a_long_hand_quote_form() {
        assert!(tags("(quote (go nowhere))").is_empty());
    }

    #[test]
    fn does_not_flag_a_comma_inside_a_hard_quote() {
        assert!(tags("'(a ,(go nowhere))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_macro_template() {
        assert!(tags("(defmacro m () `(go nowhere))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_quasiquote() {
        assert_eq!(
            tags("(defmacro m () `(progn ,(go nowhere)))"),
            vec!["nowhere"]
        );
    }

    /// The same shape under an unknown head reports nothing, and for a reason
    /// that is *not* the quote state: `(a …)` may be a macro that expands to a
    /// tagbody. Pinned beside the test above so a change to either the quote
    /// handling or the `Unknown` stop cannot be mistaken for the other.
    #[test]
    fn an_unquoted_form_under_an_unknown_head_is_still_unknown() {
        assert!(tags("(defmacro m () `(a ,(go nowhere)))").is_empty());
    }

    // -- strings ------------------------------------------------------------

    #[test]
    fn does_not_flag_a_go_inside_a_string_literal() {
        assert!(tags("(tagbody start \"(go finish)\")").is_empty());
    }

    // -- report shape -------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(go finish)", Dialect::Clojure).expect("parse");
        let report =
            build_go_to_undefined_tag_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("go_form_count", json!(0))]);
    }

    #[test]
    fn the_summary_counts_every_go_scanned_not_only_the_flagged_ones() {
        let report = report("(tagbody start (go start) (go finish))");
        assert_eq!(report.summary, vec![("go_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_tag() {
        let report = report("(tagbody start\n  (go finish))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "go-to-undefined-tag");
        assert_eq!(finding.json_fields(), vec![("tag", json!("finish"))]);
        assert_eq!(finding.text_columns(), vec!["tag=finish".to_owned()]);
    }
}
