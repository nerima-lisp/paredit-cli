//! Common Lisp `subseq`-zero detection: a two-argument `(subseq seq 0)` whose
//! start index is the literal `0` and which has no end argument. `subseq` with
//! start `0` and no end returns a fresh subsequence spanning the whole
//! sequence — that is exactly `(copy-seq seq)`, a fresh copy of the same kind.
//! `copy-seq` states the intent (copy) directly and skips the bounds arithmetic.
//!
//! Only the bare integer literal `0` is matched, and only the two-argument shape
//! (no `end` operand). A non-`0` start, a float `0.0`, a `#x0`/prefixed
//! spelling, a variable start, a present `end` argument (`(subseq seq 0 n)` is a
//! genuine slice), and a reader-conditional operand are all left alone.
//!
//! The fix rewrites `(subseq seq 0)` as `(copy-seq seq)`, copying the sequence
//! operand from its exact source, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

/// Whether `view` is the bare integer `0` literal (no reader prefixes, so `#x0`
/// and a prefixed `,0` are excluded; `0.0` is a different spelling, excluded).
fn is_zero_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("0")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct SubseqZeroItem {
    pub path: PathBuf,
    /// The span of the whole `(subseq seq 0)` form.
    pub span: ByteSpan,
    /// The span of the sequence operand (for reconstructing the fix).
    pub sequence_span: ByteSpan,
}

#[derive(Debug)]
pub struct SubseqZeroSummary {
    pub subseq_form_count: usize,
    pub violations: Vec<SubseqZeroItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SubseqZeroPolicyOptions {
    fail_on_violation: bool,
}

impl SubseqZeroPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct SubseqZeroPolicy {
    pub fail_on_violation: bool,
    pub subseq_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine(
    view: &ExpressionView,
    path: &Path,
    subseq_form_count: &mut usize,
    violations: &mut Vec<SubseqZeroItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("subseq") {
        return;
    }
    *subseq_form_count += 1;

    // children: [subseq, sequence, start] — require exactly the no-end shape.
    if view.children.len() != 3 {
        return;
    }
    let sequence = &view.children[1];
    let start = &view.children[2];
    if is_reader_conditional(sequence) || is_reader_conditional(start) {
        return;
    }
    if !is_zero_literal(start) {
        return;
    }

    violations.push(SubseqZeroItem {
        path: path.to_path_buf(),
        span: view.span,
        sequence_span: sequence.span,
    });
}

/// Collects every `(subseq seq 0)` across a whole file, along with the total
/// number of `subseq` forms scanned.
pub fn collect_subseq_zeros(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<SubseqZeroItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut subseq_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut subseq_form_count, &mut violations)
        });
    }
    Ok((subseq_form_count, violations))
}

pub fn summarize_subseq_zeros(
    subseq_form_count: usize,
    violations: Vec<SubseqZeroItem>,
) -> SubseqZeroSummary {
    SubseqZeroSummary {
        subseq_form_count,
        violations,
    }
}

pub fn evaluate_subseq_zero_policy(
    options: SubseqZeroPolicyOptions,
    summary: &SubseqZeroSummary,
) -> SubseqZeroPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SubseqZeroPolicy {
        fail_on_violation: options.fail_on_violation(),
        subseq_form_count: summary.subseq_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subseqs(input: &str) -> (usize, Vec<SubseqZeroItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_subseq_zeros(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect subseq zeros")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_subseq_zero() {
        let source = "(subseq items 0)";
        let (count, violations) = subseqs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].sequence_span), "items");
    }

    #[test]
    fn preserves_a_compound_sequence() {
        let source = "(subseq (rest xs) 0)";
        let (_, violations) = subseqs(source);
        assert_eq!(slice(source, violations[0].sequence_span), "(rest xs)");
    }

    #[test]
    fn does_not_flag_with_end_argument() {
        // (subseq seq 0 n) is a genuine slice, not a whole-sequence copy.
        let (count, violations) = subseqs("(subseq seq 0 5)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_nonzero_start() {
        let (_, violations) = subseqs("(subseq seq 1)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_zero() {
        assert!(subseqs("(subseq seq 0.0)").1.is_empty());
    }

    #[test]
    fn does_not_flag_variable_start() {
        assert!(subseqs("(subseq seq n)").1.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = subseqs("(SUBSEQ x 0)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = subseqs("(defun f (x) (subseq x 0))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(subseq x 0)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_subseq_zeros(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect subseq zeros");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = subseqs("(subseq x 0)");
        let summary = summarize_subseq_zeros(count, items);

        let quiet = evaluate_subseq_zero_policy(SubseqZeroPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_subseq_zero_policy(SubseqZeroPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
