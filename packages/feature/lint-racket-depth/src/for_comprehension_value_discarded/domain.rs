//! `racket-for-comprehension-value-discarded`: a `for/list`-family
//! comprehension in a body position whose value nothing can read.
//!
//! Racket's `for` family splits cleanly in two. `for` and `for*` iterate for
//! effect and return `(void)`. Every other member — `for/list`, `for/vector`,
//! `for/hash`, `for/set`, and their `for*` twins — *builds and returns a
//! container*. Using one of those in a statement position allocates that
//! container, fills it, and drops it on the floor; `for` does the same
//! iteration without the allocation.
//!
//! Verified on Racket v9.2 — this is legal, silent code:
//!
//! ```text
//! (define (discard lst)
//!   (for/list ([x lst]) (displayln x))   ; builds a list of void, then drops it
//!   'done)
//! ```
//!
//! # Why this anchors on the enclosing form
//!
//! The obvious spelling registers the rule for `for/list` and then asks what
//! encloses it. `RuleContext` carries no parent link, so answering that means
//! an ancestor walk **per `for/list` in the file** — and `for/list` is one of
//! the most common forms in Racket (1842 occurrences across 565 files of the
//! audited corpus). That is precisely the shape that has twice produced a rule
//! which is linear per invocation and quadratic per file.
//!
//! So the rule is registered for the *body forms* instead. A body form already
//! holds its children, so "is this child a discarded comprehension?" is
//! answered by reading a head off a child — node-local, allocation-free, and
//! with no ancestor walk at all. The quote guard is the only non-local cost and
//! it runs last, only for a node that would otherwise be reported.
//!
//! # Which position counts as discarded
//!
//! The **last** form of a body is its result, so it is never reported: a
//! function whose final expression is `(for/list …)` is returning that list,
//! which is the overwhelmingly common and entirely correct use. Only a body
//! form strictly before the last one has its value discarded.
//!
//! Each head's body starts at a different index, and getting that wrong is the
//! whole rule. `(let ⟨bindings⟩ ⟨body⟩ …)` starts at 2 but a *named*
//! `(let ⟨name⟩ ⟨bindings⟩ ⟨body⟩ …)` starts at 3; reading the binding list as
//! a body form would report a comprehension that is really a binding's
//! right-hand side, whose value is very much read.
//!
//! `(define ⟨name⟩ ⟨expression⟩)` has no body sequence at all — its single
//! operand is the value being bound — so only the function-definition spelling,
//! whose second child is a list, is examined.
//!
//! Scope: Racket only. The `for/` comprehension family is Racket's; R7RS has
//! `do` and nothing of this shape.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{is_inert_at, is_racket_list, racket_head};

/// The dialects this rule models.
pub const DIALECTS: [Dialect; 1] = [Dialect::Racket];

/// The body forms this rule anchors on, shared with its `HeadFilter`.
pub const HEADS: [&str; 11] = [
    "begin",
    "when",
    "unless",
    "lambda",
    "\u{3bb}",
    "define",
    "let",
    "let*",
    "letrec",
    "letrec*",
    "parameterize",
];

/// The comprehensions that allocate a container and return it.
///
/// `for` and `for*` are deliberately absent: they return `(void)` and are the
/// forms this rule *recommends*. `for/fold`, `for/first`, `for/last`,
/// `for/and`, `for/or`, `for/sum` and `for/product` are absent too — they
/// return a scalar rather than a fresh container, so discarding the result
/// wastes nothing but the accumulator, and `for/and`/`for/or` are sometimes
/// written in statement position on purpose for their short-circuit.
const DISCARDED_HEADS: [&str; 16] = [
    "for/list",
    "for*/list",
    "for/vector",
    "for*/vector",
    "for/hash",
    "for*/hash",
    "for/hasheq",
    "for*/hasheq",
    "for/hasheqv",
    "for*/hasheqv",
    "for/set",
    "for*/set",
    "for/seteq",
    "for*/seteq",
    "for/string",
    "for*/string",
];

/// The index of the first body form for a given head, or `None` when this
/// occurrence has no body sequence to examine.
fn first_body_index(head: &str, view: &ExpressionView) -> Option<usize> {
    match head {
        "begin" => Some(1),
        "when" | "unless" | "lambda" | "\u{3bb}" | "parameterize" => Some(2),
        // `(define (f x) body …)` has a body; `(define x expr)` does not — its
        // single operand is the value being bound, not a discarded statement.
        "define" => is_racket_list(view.children.get(1)?).then_some(2),
        // A named `let` puts the loop name where the bindings otherwise go, so
        // its body really does start at 3 rather than 2 — but this rule does
        // not need to know that, and an earlier version that computed the
        // offset survived every mutation.
        //
        // The reason is that the extra element the offset would skip is always
        // *inert to this rule*. Scanning from 2 in a named `let` adds the
        // binding list to the range; a binding list is a list **of lists**, so
        // its head is not an atom, `racket_head` answers `None`, and it is
        // skipped. In an ordinary `let` the same position is that same binding
        // list. Either way no binding's right-hand side can be mistaken for a
        // discarded comprehension, because a comprehension in a binding sits
        // one level deeper than this scan ever looks.
        //
        // So the offset is written the simple way, and
        // `does_not_flag_a_comprehension_in_a_binding_list` pins the behaviour
        // the elaborate version was protecting.
        "let" | "let*" | "letrec" | "letrec*" => Some(2),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct ForComprehensionValueDiscardedItem {
    /// The span of the comprehension whose value is discarded.
    pub span: ByteSpan,
    /// The comprehension's head, so the message can name it.
    pub comprehension: String,
    /// The head of the body form that discards it.
    pub enclosing_form: String,
}

impl Finding for ForComprehensionValueDiscardedItem {
    fn kind(&self) -> &'static str {
        "racket-for-comprehension-value-discarded"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("comprehension={}", self.comprehension),
            format!("enclosing_form={}", self.enclosing_form),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("comprehension", json!(self.comprehension)),
            ("enclosing_form", json!(self.enclosing_form)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "`{}` builds a container and its value is discarded here, because it is not the last \
             form of this `{}` body; `for` iterates without allocating one",
            self.comprehension, self.enclosing_form
        )
    }
}

/// Examines one body form, reporting each comprehension in it whose value is
/// discarded.
pub fn examine_body_form(
    tree: &SyntaxTree,
    view: &ExpressionView,
    body_form_count: &mut usize,
    violations: &mut Vec<ForComprehensionValueDiscardedItem>,
) {
    let Some(head) = racket_head(view) else {
        return;
    };
    if !HEADS.contains(&head) {
        return;
    }
    let Some(start) = first_body_index(head, view) else {
        return;
    };
    *body_form_count += 1;

    // The last child is the body's result and is never discarded, so only the
    // children strictly before it are candidates.
    let Some(last) = view.children.len().checked_sub(1) else {
        return;
    };
    if start >= last {
        return;
    }

    let mut found: Vec<(ByteSpan, String)> = Vec::new();
    for child in &view.children[start..last] {
        let Some(comprehension) = racket_head(child) else {
            continue;
        };
        if DISCARDED_HEADS.contains(&comprehension) {
            found.push((child.span, comprehension.to_owned()));
        }
    }
    if found.is_empty() {
        return;
    }

    // Last, and only for a node that would otherwise be reported.
    if is_inert_at(tree, view.span) {
        return;
    }

    let head = head.to_owned();
    for (span, comprehension) in found {
        violations.push(ForComprehensionValueDiscardedItem {
            span,
            comprehension,
            enclosing_form: head.clone(),
        });
    }
}

/// Collects every discarded comprehension in one file, with the number of body
/// forms scanned as the denominator beside them.
pub fn build_for_comprehension_value_discarded_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ForComprehensionValueDiscardedItem>> {
    let modelled = DIALECTS.contains(&dialect);
    let mut body_form_count = 0;
    let mut violations = Vec::new();

    if modelled {
        let root = tree.root_view();
        let mut stack: Vec<&ExpressionView> = root.children.iter().rev().collect();
        while let Some(view) = stack.pop() {
            examine_body_form(tree, view, &mut body_form_count, &mut violations);
            stack.extend(view.children.iter().rev());
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("body_form_count", json!(body_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ForComprehensionValueDiscardedItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("parse input");
        build_for_comprehension_value_discarded_report(
            Path::new("main.rkt"),
            Dialect::Racket,
            &tree,
        )
        .expect("build report")
    }

    fn findings(input: &str) -> Vec<ForComprehensionValueDiscardedItem> {
        report(input).findings
    }

    fn scanned(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "body_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("body_form_count in the summary")
    }

    // -- positive ------------------------------------------------------------

    /// The executed premise, reduced.
    #[test]
    fn flags_a_comprehension_in_a_non_final_define_body_position() {
        let found = findings("(define (discard lst) (for/list ([x lst]) (displayln x)) 'done)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].comprehension, "for/list");
        assert_eq!(found[0].enclosing_form, "define");
    }

    #[test]
    fn flags_every_allocating_comprehension_head() {
        for head in DISCARDED_HEADS {
            let found = findings(&format!("(begin ({head} ([x l]) x) 'done)"));
            assert_eq!(found.len(), 1, "{head} must be reported");
            assert_eq!(found[0].comprehension, head);
        }
    }

    #[test]
    fn flags_in_every_anchored_body_form() {
        let cases = [
            ("(begin (for/list ([x l]) x) 'done)", "begin"),
            ("(when p (for/list ([x l]) x) 'done)", "when"),
            ("(unless p (for/list ([x l]) x) 'done)", "unless"),
            ("(lambda (a) (for/list ([x l]) x) 'done)", "lambda"),
            ("(\u{3bb} (a) (for/list ([x l]) x) 'done)", "\u{3bb}"),
            ("(define (f) (for/list ([x l]) x) 'done)", "define"),
            ("(let ([a 1]) (for/list ([x l]) x) 'done)", "let"),
            ("(let* ([a 1]) (for/list ([x l]) x) 'done)", "let*"),
            ("(letrec ([a 1]) (for/list ([x l]) x) 'done)", "letrec"),
            ("(letrec* ([a 1]) (for/list ([x l]) x) 'done)", "letrec*"),
            (
                "(parameterize ([p v]) (for/list ([x l]) x) 'done)",
                "parameterize",
            ),
        ];
        for (source, head) in cases {
            let found = findings(source);
            assert_eq!(found.len(), 1, "{head}: {source}");
            assert_eq!(found[0].enclosing_form, head);
        }
    }

    #[test]
    fn flags_each_of_several_discarded_comprehensions() {
        let found = findings("(begin (for/list ([x l]) x) (for/vector ([y m]) y) 'done)");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].comprehension, "for/list");
        assert_eq!(found[1].comprehension, "for/vector");
    }

    /// A named `let` puts the loop name where the bindings otherwise go, so its
    /// body starts one position later.
    #[test]
    fn flags_in_a_named_let_body() {
        let found = findings("(let loop ([a 1]) (for/list ([x l]) x) 'done)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].enclosing_form, "let");
    }

    // -- the last form is the result, never discarded -------------------------

    #[test]
    fn does_not_flag_a_comprehension_that_is_the_body_result() {
        for source in [
            "(define (f lst) (for/list ([x lst]) x))",
            "(begin (setup) (for/list ([x l]) x))",
            "(lambda (l) (for/list ([x l]) x))",
            "(let ([a 1]) (for/list ([x l]) x))",
            "(when p (for/list ([x l]) x))",
        ] {
            assert!(findings(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn does_not_flag_a_body_with_only_one_form() {
        assert!(findings("(begin (for/list ([x l]) x))").is_empty());
    }

    // -- the non-allocating members are the recommendation, not the defect ----

    #[test]
    fn does_not_flag_for_or_for_star() {
        assert!(findings("(begin (for ([x l]) (f x)) 'done)").is_empty());
        assert!(findings("(begin (for* ([x l] [y m]) (f x y)) 'done)").is_empty());
    }

    /// A scalar-accumulating comprehension allocates no container, and
    /// `for/and`/`for/or` are sometimes written in statement position for their
    /// short-circuit.
    #[test]
    fn does_not_flag_the_scalar_accumulating_comprehensions() {
        for head in [
            "for/fold",
            "for/first",
            "for/last",
            "for/and",
            "for/or",
            "for/sum",
            "for/product",
        ] {
            assert!(
                findings(&format!("(begin ({head} ([x l]) x) 'done)")).is_empty(),
                "{head} allocates no container"
            );
        }
    }

    // -- header positions are not body positions ------------------------------

    /// `(define x (for/list …))` binds the comprehension's value; it is not a
    /// discarded statement. Reading operand 1 as a body form would report it.
    #[test]
    fn does_not_flag_a_value_define() {
        assert!(findings("(define x (for/list ([i l]) i))").is_empty());
        // Even with a following form at top level, the `define` itself has no
        // body sequence.
        assert_eq!(scanned("(define x (for/list ([i l]) i))"), 0);
    }

    /// A comprehension in a binding's right-hand side has its value read by the
    /// binding. Reading the binding list as a body form would report it.
    #[test]
    fn does_not_flag_a_comprehension_in_a_binding_list() {
        for source in [
            "(let ([xs (for/list ([i l]) i)]) (use xs) 'done)",
            "(let* ([xs (for/list ([i l]) i)]) (use xs) 'done)",
            "(letrec ([xs (for/list ([i l]) i)]) (use xs) 'done)",
            "(let loop ([xs (for/list ([i l]) i)]) (use xs) 'done)",
        ] {
            assert!(findings(source).is_empty(), "{source}");
        }
    }

    /// The test of a `when`/`unless` is read, not discarded.
    #[test]
    fn does_not_flag_a_comprehension_in_a_test_position() {
        assert!(findings("(when (for/list ([x l]) x) (f) 'done)").is_empty());
        assert!(findings("(unless (for/list ([x l]) x) (f) 'done)").is_empty());
    }

    /// A lambda's parameter list is not a body form.
    #[test]
    fn does_not_flag_a_lambda_parameter_list() {
        assert!(findings("(lambda (for/list) (f) 'done)").is_empty());
    }

    #[test]
    fn the_named_let_offset_is_not_applied_to_an_ordinary_let() {
        // With the named-let offset an ordinary `let`'s first body form would
        // be skipped and this finding lost.
        assert_eq!(
            findings("(let ([a 1]) (for/list ([x l]) x) 'done)").len(),
            1
        );
    }

    // -- head discipline ------------------------------------------------------

    #[test]
    fn does_not_case_fold_the_enclosing_head() {
        assert_eq!(scanned("(BEGIN (for/list ([x l]) x) 'done)"), 0);
    }

    #[test]
    fn does_not_case_fold_the_comprehension_head() {
        assert!(findings("(begin (FOR/LIST ([x l]) x) 'done)").is_empty());
    }

    #[test]
    fn does_not_flag_a_qualified_comprehension_head() {
        assert!(findings("(begin (racket:for/list ([x l]) x) 'done)").is_empty());
    }

    // -- data and template guards ---------------------------------------------

    #[test]
    fn does_not_flag_a_quoted_body_shape() {
        assert!(findings("'(begin (for/list ([x l]) x) 'done)").is_empty());
        assert!(findings("(quote (begin (for/list ([x l]) x) 'done))").is_empty());
        assert!(findings("`(a (begin (for/list ([x l]) x) 'done))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_body_inside_a_quasiquote() {
        assert_eq!(
            findings("`(a ,(begin (for/list ([x l]) x) 'done))").len(),
            1
        );
    }

    #[test]
    fn does_not_flag_a_vector_constant_that_looks_like_a_body_form() {
        assert_eq!(scanned("#(begin (for/list ([x l]) x) 'done)"), 0);
    }

    #[test]
    fn does_not_flag_a_body_inside_a_macro_template() {
        assert!(
            findings(
                "(define-syntax m (syntax-rules () ((_ l) (begin (for/list ([x l]) x) 'done))))"
            )
            .is_empty()
        );
    }

    // -- the envelope ---------------------------------------------------------

    #[test]
    fn the_summary_counts_every_body_form_scanned() {
        let source = "(begin (for/list ([x l]) x) 'done)\n(begin (f) 'done)\n";
        assert_eq!(scanned(source), 2);
        assert_eq!(findings(source).len(), 1);
    }

    #[test]
    fn the_same_bytes_are_flagged_as_racket_and_unmodelled_elsewhere() {
        // Parenthesized, not bracketed: the Common Lisp and Clojure readers
        // reject `[`, so a bracketed fixture would fail to parse for the
        // controls and prove nothing about scope.
        let source = "(begin (for/list ((x l)) x) 1)\n";
        assert_eq!(findings(source).len(), 1);
        for dialect in [Dialect::Scheme, Dialect::CommonLisp, Dialect::Clojure] {
            let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
            let report =
                build_for_comprehension_value_discarded_report(Path::new("f.scm"), dialect, &tree)
                    .expect("build report");
            assert!(!report.dialect_modelled, "{dialect:?}");
            assert!(report.findings.is_empty(), "{dialect:?}");
        }
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("#lang racket\n(define (f l)\n  (for/list ([x l]) x)\n  'done)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "racket-for-comprehension-value-discarded");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("comprehension", json!("for/list")),
                ("enclosing_form", json!("define"))
            ]
        );
        assert!(finding.message().contains("discarded"));
    }
}
