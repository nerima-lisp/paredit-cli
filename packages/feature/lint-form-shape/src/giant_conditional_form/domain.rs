//! `giant-conditional-form` detection: a `let`/`let*`/`cond`/`case`-family
//! form carrying more clauses than a threshold, a candidate for splitting
//! into smaller, named pieces.
//!
//! The count is the same idea for every head, read from a different position:
//! `let`/`let*` count their bindings; `cond` counts its clauses; the
//! `case`-family (`case`, `ecase`, `ccase`, `typecase`, `etypecase`,
//! `ctypecase`) counts every clause after the key/test form. Heads are matched
//! unqualified and lower-cased, so this is the same rule across every dialect
//! this tool parses — `cond`/`case`/`let` all exist, spelled the same, in
//! Common Lisp, Scheme, Racket, and Clojure.
//!
//! Report-only. Which bindings or clauses belong together, and what the split
//! pieces should be named, is a design decision this tool does not make.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path as SexprPath, SyntaxTree,
};
use paredit_core_syntax::view_query::{for_each_subview, list_head, symbol_in, unqualified};

/// The default clause/binding count a form may carry before this rule speaks,
/// used by the standalone `inspect giant-conditional-form` command. The
/// registered lint rule reads the same default from its own `RuleSetting`,
/// which a `--rule-arg` can override; the standalone command has no such
/// override, so it always uses this constant.
pub const DEFAULT_THRESHOLD: usize = 8;

/// Heads whose clauses/bindings sit after the key or test form rather than
/// starting at index 1, the way `let`/`let*`/`cond` do.
const KEYED_HEADS: [&str; 6] = [
    "case",
    "ecase",
    "ccase",
    "typecase",
    "etypecase",
    "ctypecase",
];

#[derive(Debug, Clone)]
pub struct GiantConditionalFormItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The unqualified, lower-cased head: `let`, `let*`, `cond`, or one of the
    /// `case`-family spellings.
    pub head: String,
    /// How many bindings (`let`/`let*`) or clauses (everything else) the form
    /// carries.
    pub count: usize,
    pub threshold: usize,
}

#[derive(Debug)]
pub struct GiantConditionalFormSummary {
    pub scanned_form_count: usize,
    pub violations: Vec<GiantConditionalFormItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct GiantConditionalFormPolicyOptions {
    fail_on_violation: bool,
}

impl GiantConditionalFormPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct GiantConditionalFormPolicy {
    pub fail_on_violation: bool,
    pub scanned_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// How many bindings or clauses `view` carries, and under what label, if its
/// head is one this rule judges at all.
#[must_use]
pub fn clause_count(view: &ExpressionView) -> Option<(String, usize)> {
    let head = list_head(view)?;
    let label = unqualified(head).to_ascii_lowercase();

    if symbol_in(head, &["let", "let*"]) {
        let bindings = view.children.get(1)?;
        if bindings.kind != ExpressionKind::List {
            return None;
        }
        return Some((label, bindings.children.len()));
    }

    if symbol_in(head, &["cond"]) {
        return Some((label, view.children.len().saturating_sub(1)));
    }

    if symbol_in(head, &KEYED_HEADS) {
        // children[0] = head, children[1] = keyform/testform, the rest are
        // clauses. A form too short to have a keyform is not judged.
        if view.children.len() < 2 {
            return None;
        }
        return Some((label, view.children.len().saturating_sub(2)));
    }

    None
}

pub fn examine(
    view: &ExpressionView,
    path: &Path,
    threshold: usize,
    scanned_form_count: &mut usize,
    violations: &mut Vec<GiantConditionalFormItem>,
) {
    *scanned_form_count += 1;
    let Some((head, count)) = clause_count(view) else {
        return;
    };
    if count > threshold {
        violations.push(GiantConditionalFormItem {
            path: path.to_path_buf(),
            span: view.span,
            head,
            count,
            threshold,
        });
    }
}

/// Collects every violation across a whole file, along with the total number of
/// forms scanned, using [`DEFAULT_THRESHOLD`].
pub fn collect_giant_conditional_form(
    path: &Path,
    _dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<GiantConditionalFormItem>)> {
    let mut scanned_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(
                subview,
                path,
                DEFAULT_THRESHOLD,
                &mut scanned_form_count,
                &mut violations,
            );
        });
    }
    Ok((scanned_form_count, violations))
}

#[must_use]
pub const fn summarize_giant_conditional_form(
    scanned_form_count: usize,
    violations: Vec<GiantConditionalFormItem>,
) -> GiantConditionalFormSummary {
    GiantConditionalFormSummary {
        scanned_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_giant_conditional_form_policy(
    options: GiantConditionalFormPolicyOptions,
    summary: &GiantConditionalFormSummary,
) -> GiantConditionalFormPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    GiantConditionalFormPolicy {
        fail_on_violation: options.fail_on_violation(),
        scanned_form_count: summary.scanned_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::Path as TreePath;

    fn counted(input: &str) -> Option<(String, usize)> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&TreePath::root_child(0))
            .expect("root form")
            .view();
        clause_count(&view)
    }

    #[test]
    fn counts_let_bindings() {
        assert_eq!(
            counted("(let ((a 1) (b 2) (c 3)) a)"),
            Some(("let".to_owned(), 3))
        );
    }

    #[test]
    fn counts_let_star_bindings() {
        assert_eq!(
            counted("(let* ((a 1) (b 2)) a)"),
            Some(("let*".to_owned(), 2))
        );
    }

    #[test]
    fn counts_cond_clauses() {
        assert_eq!(
            counted("(cond ((a) 1) ((b) 2) (t 3))"),
            Some(("cond".to_owned(), 3))
        );
    }

    #[test]
    fn counts_case_clauses_after_the_keyform() {
        assert_eq!(
            counted("(case x (1 :a) (2 :b) (otherwise :c))"),
            Some(("case".to_owned(), 3))
        );
    }

    #[test]
    fn counts_typecase_clauses() {
        assert_eq!(
            counted("(typecase x (integer 1) (string 2))"),
            Some(("typecase".to_owned(), 2))
        );
    }

    #[test]
    fn does_not_judge_an_unrelated_form() {
        assert_eq!(counted("(defun f (x) x)"), None);
    }

    #[test]
    fn a_let_with_no_bindings_list_is_not_judged() {
        // Malformed input; another rule's problem to report.
        assert_eq!(counted("(let)"), None);
    }

    #[test]
    fn examine_reports_a_form_over_threshold() {
        let tree =
            SyntaxTree::parse_with_dialect("(cond ((a) 1) ((b) 2) ((c) 3))", Dialect::CommonLisp)
                .expect("parse");
        let view = tree
            .select_path(&TreePath::root_child(0))
            .expect("root form")
            .view();
        let mut scanned = 0;
        let mut violations = Vec::new();
        examine(&view, Path::new("t.lisp"), 2, &mut scanned, &mut violations);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].count, 3);
        assert_eq!(violations[0].head, "cond");
    }

    #[test]
    fn examine_does_not_report_a_form_at_or_under_threshold() {
        let tree = SyntaxTree::parse_with_dialect("(cond ((a) 1) ((b) 2))", Dialect::CommonLisp)
            .expect("parse");
        let view = tree
            .select_path(&TreePath::root_child(0))
            .expect("root form")
            .view();
        let mut scanned = 0;
        let mut violations = Vec::new();
        examine(&view, Path::new("t.lisp"), 2, &mut scanned, &mut violations);
        assert!(violations.is_empty());
    }
}
