//! Common Lisp eval-when-situation detection: an `eval-when` whose situation
//! list names a situation that is not one of the valid keywords. The only
//! valid situations are `:compile-toplevel`, `:load-toplevel`, `:execute`, and
//! the deprecated pre-ANSI names `compile`, `load`, `eval`. A typo like
//! `(eval-when (:executee) …)` or `(eval-when (:compile-top-level) …)` compiles
//! without complaint but silently never runs its body at the intended time — a
//! subtle bug the compiler does not catch.
//!
//! Only the situation list (child 1) is inspected. Situations guarded by a
//! `#+`/`#-` reader conditional are skipped, as is a quoted/quasiquoted
//! `eval-when` (data, not a call).
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// Whether a situation atom is one CLHS accepts.
fn is_valid_situation(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        ":compile-toplevel" | ":load-toplevel" | ":execute" | "compile" | "load" | "eval"
    )
}

/// Whether a situation-list element is a `#+`/`#-` reader conditional, whose
/// presence makes the situation not statically checkable.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    let conditional_prefix = view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional | ReaderPrefix::ReaderConditionalSplicing
        )
    });
    conditional_prefix
        || atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct EvalWhenSituationItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub situation: String,
}

#[derive(Debug)]
pub struct EvalWhenSituationSummary {
    pub eval_when_form_count: usize,
    pub violations: Vec<EvalWhenSituationItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct EvalWhenSituationPolicyOptions {
    fail_on_violation: bool,
}

impl EvalWhenSituationPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct EvalWhenSituationPolicy {
    pub fail_on_violation: bool,
    pub eval_when_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_eval_when(
    view: &ExpressionView,
    path: &Path,
    eval_when_form_count: &mut usize,
    violations: &mut Vec<EvalWhenSituationItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("eval-when")) {
        return;
    }
    // A quoted/quasiquoted eval-when is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    let Some(situations) = view.children.get(1) else {
        return;
    };
    // A non-list situations argument is a different malformation; skip it.
    if !is_paren_list(situations) {
        return;
    }
    *eval_when_form_count += 1;

    for situation in &situations.children {
        if is_reader_conditional(situation) {
            continue;
        }
        // A valid situation is one of the recognized bare atoms; anything else
        // (an unknown keyword, a typo, or a non-atom) is invalid.
        let valid = atom_text(situation).is_some_and(is_valid_situation);
        if !valid {
            violations.push(EvalWhenSituationItem {
                path: path.to_path_buf(),
                span: situation.span,
                situation: atom_text(situation).unwrap_or("()").to_owned(),
            });
        }
    }
}

/// Collects every `eval-when` with an invalid situation across a whole file,
/// along with the total number of `eval-when` forms scanned.
pub fn collect_eval_when_situations(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<EvalWhenSituationItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut eval_when_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_eval_when(subview, path, &mut eval_when_form_count, &mut violations)
        });
    }
    Ok((eval_when_form_count, violations))
}

pub fn summarize_eval_when_situations(
    eval_when_form_count: usize,
    violations: Vec<EvalWhenSituationItem>,
) -> EvalWhenSituationSummary {
    EvalWhenSituationSummary {
        eval_when_form_count,
        violations,
    }
}

pub fn evaluate_eval_when_situation_policy(
    options: EvalWhenSituationPolicyOptions,
    summary: &EvalWhenSituationSummary,
) -> EvalWhenSituationPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    EvalWhenSituationPolicy {
        fail_on_violation: options.fail_on_violation(),
        eval_when_form_count: summary.eval_when_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<EvalWhenSituationItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_eval_when_situations(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect eval-when situations")
    }

    #[test]
    fn flags_a_keyword_typo() {
        let (form_count, items) = violations("(eval-when (:compile-toplevel :executee) 1)");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].situation, ":executee");
    }

    #[test]
    fn flags_a_non_keyword_typo() {
        let (_, items) = violations("(eval-when (excute) 1)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].situation, "excute");
    }

    #[test]
    fn does_not_flag_valid_keyword_situations() {
        let (form_count, items) =
            violations("(eval-when (:compile-toplevel :load-toplevel :execute) 1)");
        assert_eq!(form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_deprecated_situations() {
        let (_, items) = violations("(eval-when (compile load eval) 1)");
        assert!(items.is_empty());
    }

    #[test]
    fn folds_situation_case() {
        let (_, items) = violations("(eval-when (:Execute :COMPILE-TOPLEVEL) 1)");
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_reader_conditional_situation() {
        let (form_count, items) = violations("(eval-when (#+sbcl :execute) 1)");
        assert_eq!(form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_eval_when() {
        let (form_count, items) = violations("(list '(eval-when (:bad) 1))");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn flags_a_typo_among_valid_situations() {
        let (_, items) = violations("(eval-when (:execute :bogus) 1)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].situation, ":bogus");
    }

    #[test]
    fn does_not_flag_a_non_list_situations_argument() {
        let (form_count, items) = violations("(eval-when :execute 1)");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_an_eval_when_nested_in_a_body() {
        let (form_count, items) = violations("(progn (eval-when (:loud) 1))");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(eval-when (:bad) 1)", Dialect::Clojure)
            .expect("parse input");
        let (form_count, items) =
            collect_eval_when_situations(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect eval-when situations");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (form_count, items) = violations("(eval-when (:bad) 1)");
        let summary = summarize_eval_when_situations(form_count, items);

        let quiet = evaluate_eval_when_situation_policy(
            EvalWhenSituationPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_eval_when_situation_policy(
            EvalWhenSituationPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
