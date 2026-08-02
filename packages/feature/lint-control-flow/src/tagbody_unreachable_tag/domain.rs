//! Common Lisp dead-tag detection: a `tagbody` label nothing in the form ever
//! names.
//!
//! A tag has exactly one use — being the target of a `go` (CLHS 5.3). Control
//! reaches the statements after a label by falling through whether the label
//! is there or not, so a label no `go` names does nothing at all.
//!
//! # What counts as naming the tag
//!
//! Deliberately much more than `(go tag)`: **any** occurrence of the tag's
//! name anywhere in the tagbody's subtree, in any position, quoted or not,
//! other than a tag position itself. `(my-jump-to 'retry)`, `(go retry)` and
//! `(list retry)` all count.
//!
//! That is far wider than the standard requires, and on purpose. The one thing
//! this analysis cannot see is a macro expansion: `(retry-when …)` may expand
//! to `(go retry)`, and then the tag is live while nothing in the source says
//! so. Requiring the name to appear *nowhere else* is what keeps such a file
//! from being flagged, at the cost of missing a dead tag whose name happens to
//! be used for something else. The remaining hole — a macro that hardcodes a
//! tag name its own expansion does not mention, and that is defined outside
//! this tagbody — is noted rather than covered.
//!
//! Tag positions of *nested* tagbodies count as definitions too, not as uses:
//! `(tagbody done (tagbody done (foo)))` has two dead labels, not one live
//! one.
//!
//! # Relationship to `remove-unused-tag`
//!
//! `paredit-feature-remove-unused`'s `remove-unused-tag` command answers the
//! same question for *one* tag the caller points at, and rewrites it away.
//! This is the scanning half, and cannot borrow that implementation: a
//! feature package depending on another feature package would be a new
//! feature→feature edge in the dependency contract.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{Tag, direct_tags, is_unevaluated_at, tagbody_body_start};

#[derive(Debug, Clone)]
pub struct TagbodyUnreachableTagItem {
    /// The span of the label itself, not of the whole tagbody.
    pub span: ByteSpan,
    /// The tag, as it reads in a report.
    pub tag: String,
}

impl Finding for TagbodyUnreachableTagItem {
    fn kind(&self) -> &'static str {
        "tagbody-unreachable-tag"
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
        "the tagbody label `{tag}` is never named by a go; control only falls through it, \
         so the label does nothing"
    )
}

/// Every tag-position span in `view`'s subtree — this tagbody's own labels and
/// those of every nested implicit tagbody.
///
/// These are the occurrences that are *definitions*. Everything else the name
/// appears in is a use.
fn definition_spans(view: &ExpressionView) -> Vec<ByteSpan> {
    let mut spans = Vec::new();
    for_each_subview(view, |subview| {
        if list_head(subview).is_some_and(|head| tagbody_body_start(head).is_some()) {
            spans.extend(direct_tags(subview).into_iter().map(|(_, span)| span));
        }
    });
    spans
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// Reads only the matched form's own subtree, so a file of T tagbodies costs
/// the file once and not T times.
pub fn examine_tagbody(
    tree: &SyntaxTree,
    view: &ExpressionView,
    tagbody_form_count: &mut usize,
    violations: &mut Vec<TagbodyUnreachableTagItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "tagbody")) {
        return;
    }
    *tagbody_form_count += 1;

    let tags = direct_tags(view);
    if tags.is_empty() {
        return;
    }

    let definitions = definition_spans(view);
    // Every atom in the subtree that reads as a tag, definitions included;
    // quoted data deliberately included, since `(jump-to 'retry)` names the
    // tag as far as this rule is concerned.
    let mut mentions: Vec<(Tag, ByteSpan)> = Vec::new();
    // `Tag::mention`, not `Tag::read`: `'retry` names no tag to `go`, but it
    // does mention the name, and a macro may be what turns it into a jump.
    // It also answers `None` for anything with children, so this visits every
    // atom and nothing else.
    for_each_subview(view, |subview| {
        if let Some(tag) = Tag::mention(subview) {
            mentions.push((tag, subview.span));
        }
    });

    let dead: Vec<(Tag, ByteSpan)> = tags
        .into_iter()
        .filter(|(tag, _)| {
            !mentions.iter().any(|(mentioned, at)| {
                mentioned == tag && !definitions.iter().any(|definition| definition == at)
            })
        })
        .collect();
    if dead.is_empty() {
        return;
    }
    // Last, not first: the quote descent materializes the enclosing top-level
    // form, and a tagbody whose every label is jumped to — which is what a
    // healthy file is made of — must not pay for it. Measured at 3.8µs per
    // matched tagbody when asked early against 0.5µs when asked here.
    if is_unevaluated_at(tree, view.span) {
        return;
    }

    for (tag, span) in dead {
        violations.push(TagbodyUnreachableTagItem {
            span,
            tag: tag.display(),
        });
    }
}

/// Collects every dead tagbody label in one file, with the number of `tagbody`
/// forms scanned as the denominator beside them.
pub fn build_tagbody_unreachable_tag_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<TagbodyUnreachableTagItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("tagbody_form_count", json!(0))],
        ));
    }

    let mut tagbody_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_tagbody(tree, subview, &mut tagbody_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("tagbody_form_count", json!(tagbody_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<TagbodyUnreachableTagItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_tagbody_unreachable_tag_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
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
    fn flags_a_label_no_go_names() {
        assert_eq!(
            tags("(tagbody start (foo) (go finish) finish)"),
            vec!["start"]
        );
    }

    #[test]
    fn flags_a_label_in_a_tagbody_with_no_go_at_all() {
        assert_eq!(tags("(tagbody cleanup (teardown))"), vec!["cleanup"]);
    }

    #[test]
    fn flags_an_integer_label_no_go_names() {
        assert_eq!(tags("(tagbody 10 (foo))"), vec!["10"]);
    }

    #[test]
    fn flags_every_dead_label_of_one_tagbody() {
        assert_eq!(
            tags("(tagbody one (a) two (b) three (go three))"),
            vec!["one", "two"]
        );
    }

    /// A nested tagbody's tag position is a definition, not a use, so both are
    /// dead.
    #[test]
    fn flags_a_label_a_nested_tagbody_redefines_but_nothing_names() {
        assert_eq!(
            tags("(tagbody done (tagbody done (foo)))"),
            vec!["done", "done"]
        );
    }

    #[test]
    fn the_span_covers_the_label_and_not_the_tagbody() {
        let report = report("(tagbody start (foo))");
        let finding = &report.findings[0];
        assert_eq!(finding.span().start().get(), 9);
        assert_eq!(finding.span().end().get(), 14);
    }

    // -- near-miss negatives ------------------------------------------------

    #[test]
    fn does_not_flag_a_label_a_go_names() {
        assert!(tags("(tagbody again (foo) (go again))").is_empty());
    }

    #[test]
    fn does_not_flag_a_label_a_go_deep_in_the_body_names() {
        assert!(tags("(tagbody again (when (foo) (let ((x 1)) (go again))))").is_empty());
    }

    /// CLHS 5.3: a `go` in a nested tagbody may name an outer tag.
    #[test]
    fn does_not_flag_a_label_a_nested_tagbody_jumps_to() {
        assert!(tags("(tagbody top (tagbody (go top)))").is_empty());
    }

    /// The macro guard: a name mentioned anywhere else is left alone, because
    /// the mention may be what a macro turns into a `go`.
    #[test]
    fn does_not_flag_a_label_whose_name_appears_anywhere_else() {
        assert!(tags("(tagbody retry (jump-to 'retry))").is_empty());
        assert!(tags("(tagbody retry (my-macro retry))").is_empty());
        assert!(tags("(tagbody retry (list \"x\" retry))").is_empty());
    }

    #[test]
    fn does_not_flag_a_tagbody_with_no_labels_at_all() {
        assert!(tags("(tagbody (foo) (bar))").is_empty());
        assert_eq!(report("(tagbody (foo))").summary[0].1, json!(1));
    }

    #[test]
    fn does_not_read_a_string_or_a_float_as_a_label() {
        assert!(tags("(tagbody \"a doc string\" (foo))").is_empty());
        assert!(tags("(tagbody 1.5 (foo))").is_empty());
    }

    #[test]
    fn does_not_flag_the_body_of_a_prog_or_a_do() {
        // This rule anchors on `tagbody` alone; the implicit tagbodies are
        // read only as *definitions* when nested inside one.
        assert!(tags("(prog () start (foo))").is_empty());
        assert!(tags("(do ((i 0)) ((= i 3)) start (foo))").is_empty());
    }

    #[test]
    fn case_folds_and_ignores_the_package_qualifier() {
        assert!(tags("(TAGBODY START (GO start))").is_empty());
        assert_eq!(tags("(CL:TAGBODY START (foo))"), vec!["start"]);
    }

    // -- the five quote shapes ---------------------------------------------

    #[test]
    fn does_not_flag_a_hard_quoted_form() {
        assert!(tags("'(tagbody start (foo))").is_empty());
    }

    #[test]
    fn does_not_flag_a_long_hand_quote_form() {
        assert!(tags("(quote (tagbody start (foo)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_comma_inside_a_hard_quote() {
        assert!(tags("'(a ,(tagbody start (foo)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_macro_template() {
        assert!(tags("(defmacro m () `(tagbody start (foo)))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_quasiquote() {
        assert_eq!(
            tags("(defmacro m () `(a ,(tagbody start (foo))))"),
            vec!["start"]
        );
    }

    // -- strings ------------------------------------------------------------

    #[test]
    fn does_not_read_a_go_inside_a_string_literal_as_a_use() {
        assert_eq!(tags("(tagbody start \"(go start)\" (foo))"), vec!["start"]);
    }

    // -- report shape -------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(tagbody start (foo))", Dialect::Clojure)
            .expect("parse");
        let report =
            build_tagbody_unreachable_tag_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("tagbody_form_count", json!(0))]);
    }

    #[test]
    fn the_summary_counts_every_tagbody_scanned_not_only_the_flagged_ones() {
        let report = report("(tagbody a (go a))\n(tagbody b (foo))\n");
        assert_eq!(report.summary, vec![("tagbody_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_tag() {
        let report = report("(tagbody\n  start\n  (foo))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "tagbody-unreachable-tag");
        assert_eq!(finding.json_fields(), vec![("tag", json!("start"))]);
        assert_eq!(finding.text_columns(), vec!["tag=start".to_owned()]);
    }
}
