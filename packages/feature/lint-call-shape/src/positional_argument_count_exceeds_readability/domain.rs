//! A call inside a definition body passing a long run of unlabelled literal
//! arguments.
//!
//! ```lisp
//! (render-panel 10 20 240 "status" t 3 nil)
//! ```
//!
//! Nothing at that call site says which number is the width and which the
//! height, or what the `t` turns on. The reader has to go and find the callee's
//! lambda list — which is the cost this reports, independently of what that
//! lambda list turns out to say.
//!
//! # What has to be true before this reports
//!
//! 1. **More than `max-positional-literals` arguments** — four by default, so
//!    the first reported call passes five.
//! 2. **Every argument is a literal**: a number, a string, a character, or
//!    `t`/`nil`. One variable, one quoted symbol, or one nested call and the
//!    call is left alone — a name is exactly the thing that makes an argument
//!    readable, and a call with any of them is not the shape this is about.
//! 3. **At least two different kinds of literal.** A run of nothing but
//!    numbers is *data* — a matrix, a colour, a coordinate list, a palette —
//!    and reads perfectly well: `(matrix 1 0 0 0 1 0 0 0 1)` is never
//!    reported. The unreadable shape is the unlabelled *mixed* bag, where a
//!    boolean or a string sits among the numbers with nothing naming it.
//! 4. **The head is a plain symbol that is not variadic by nature.** `list`,
//!    `vector`, `format`, `+`, `and`, `values` and the rest of the
//!    arbitrary-arity operators take as many arguments as they are given by
//!    design; reporting them would report the language.
//!
//! # What this rule does not attempt
//!
//! - It never looks at the callee. That is deliberate — the callee is usually
//!   in another file, and the call site is unreadable either way.
//! - It only inspects calls *inside* a definition body, because that is what
//!   lets it stay `HeadFilter::Heads` and still cost one extra pass over the
//!   file rather than one per definition. A long literal call at top level, or
//!   inside a bare `let` at top level, is a deliberate false negative.
//! - It prunes at a nested definition, which the dispatcher will hand to this
//!   rule separately; otherwise a call inside one would be reported twice.
//! - Scope is Common Lisp only: the excluded-operator table is the CLHS one.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_in, unqualified,
};
use serde_json::{Value, json};

use crate::support::{for_each_evaluated_branch_positioned, is_unevaluated_at, normalized_symbol};

/// Forms whose trailing children are *clauses*, which look exactly like calls.
///
/// `(cond (ready 1 2 3 "a" nil))` is not a call to `ready`; without this the
/// rule reports every long `cond`/`case` clause in the file, which was the
/// first false positive its own tests caught.
const CLAUSE_FORMS_FROM_INDEX_TWO: &[&str] = &[
    "case",
    "ecase",
    "ccase",
    "typecase",
    "etypecase",
    "ctypecase",
    "handler-case",
    "restart-case",
    "handler-bind",
    "restart-bind",
];

/// Whether the child at `index` of `parent` is a clause or a binding rather
/// than a call.
///
/// A parent with no symbol head is a *list of lists* — a `let` binding list, a
/// `defclass` slot list, a lambda list, a clause list — and nothing directly in
/// one is a call.
fn is_clause_position(parent: &ExpressionView, index: usize) -> bool {
    match list_head(parent) {
        None => true,
        Some(head) if symbol_in(head, &["cond"]) => index >= 1,
        Some(head) if symbol_in(head, CLAUSE_FORMS_FROM_INDEX_TWO) => index >= 2,
        Some(_) => false,
    }
}

/// How many literal arguments a call may carry by default. The first reported
/// call passes one more than this.
pub const DEFAULT_MAX_POSITIONAL_LITERALS: usize = 4;

/// The definition heads whose bodies are scanned.
pub const DEFINITION_HEADS: &[&str] = &["defun", "defmacro", "defmethod"];

/// Operators whose arity is arbitrary by design, so a long argument list says
/// nothing about readability.
///
/// Read as a closed list of *language* forms, not of user functions: everything
/// here either takes an arbitrary number of arguments by definition (`list`,
/// `+`, `and`) or is a special operator whose "arguments" are not arguments at
/// all (`declare`, `setq`, `cond`). A name missing from this list only ever
/// makes the rule louder, so it is the one place a false positive can enter,
/// and it is deliberately generous.
const ARBITRARY_ARITY_OPERATORS: &[&str] = &[
    // Sequence and data construction.
    "list",
    "list*",
    "vector",
    "append",
    "nconc",
    "concatenate",
    "make-string",
    "values",
    "values-list",
    "multiple-value-call",
    // Arithmetic, comparison and bitwise, all n-ary in the CLHS.
    "+",
    "-",
    "*",
    "/",
    "=",
    "/=",
    "<",
    ">",
    "<=",
    ">=",
    "max",
    "min",
    "gcd",
    "lcm",
    "logand",
    "logior",
    "logxor",
    "logeqv",
    "lognand",
    "lognor",
    "char=",
    "char/=",
    "char<",
    "char>",
    "string=",
    "string/=",
    "string<",
    "string>",
    "eq",
    "eql",
    "equal",
    "equalp",
    // Control and binding special operators: their subforms are not arguments.
    "and",
    "or",
    "progn",
    "prog1",
    "prog2",
    "if",
    "when",
    "unless",
    "cond",
    "case",
    "ecase",
    "ccase",
    "typecase",
    "etypecase",
    "ctypecase",
    "let",
    "let*",
    "flet",
    "labels",
    "macrolet",
    "block",
    "tagbody",
    "return-from",
    "setq",
    "psetq",
    "setf",
    "psetf",
    "declare",
    "declaim",
    "proclaim",
    "the",
    "t",
    "otherwise",
    // Reporting forms, whose trailing arguments are format arguments.
    "format",
    "error",
    "warn",
    "cerror",
    "signal",
    "assert",
    "check-type",
    "print",
    "princ",
    "write",
    "write-string",
    "write-line",
    // Definition heads: their children are a name and a lambda list, not
    // arguments, and the rule matches them as *anchors* rather than as calls.
    "defun",
    "defmacro",
    "defmethod",
    "defgeneric",
    "defvar",
    "defparameter",
    "defconstant",
    "defclass",
    "defstruct",
    "defpackage",
    "deftype",
    // Application, whose argument count is the callee's business.
    "funcall",
    "apply",
];

/// What kind of literal an argument is. The rule needs at least two different
/// ones, because a homogeneous run is data rather than an argument list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    Number,
    String,
    Character,
    Boolean,
}

/// The kind of literal an atom is, or `None` for anything that is not one.
///
/// A reader prefix disqualifies: `'foo` is a quoted symbol, `#'f` is a
/// function, and neither is the unlabelled constant this rule is about.
#[must_use]
pub fn literal_kind(view: &ExpressionView) -> Option<LiteralKind> {
    if !view.children.is_empty() || !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_text(view)?;
    if text.starts_with('"') {
        return Some(LiteralKind::String);
    }
    if text.starts_with("#\\") {
        return Some(LiteralKind::Character);
    }
    if unqualified(text).eq_ignore_ascii_case("t") || unqualified(text).eq_ignore_ascii_case("nil")
    {
        return Some(LiteralKind::Boolean);
    }
    let mut characters = text.chars();
    let first = characters.next()?;
    if first.is_ascii_digit() {
        return Some(LiteralKind::Number);
    }
    if matches!(first, '+' | '-' | '.')
        && characters.next().is_some_and(|next| next.is_ascii_digit())
    {
        return Some(LiteralKind::Number);
    }
    None
}

/// One reported call.
#[derive(Debug, Clone)]
pub struct PositionalLiteralCallItem {
    /// The span of the whole call.
    pub span: ByteSpan,
    /// The callee's name, normalized.
    pub head: String,
    /// How many literal arguments it passes.
    pub argument_count: usize,
    /// The count this run allowed.
    pub threshold: usize,
}

impl Finding for PositionalLiteralCallItem {
    fn kind(&self) -> &'static str {
        "positional-argument-count-exceeds-readability"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("argument_count={}", self.argument_count)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            ("argument_count", json!(self.argument_count)),
            ("threshold", json!(self.threshold)),
        ]
    }

    fn message(&self) -> String {
        message(&self.head, self.argument_count, self.threshold)
    }
}

/// The one sentence both the report and the lint rule print.
#[must_use]
pub fn message(head: &str, argument_count: usize, threshold: usize) -> String {
    format!(
        "`{head}` is called with {argument_count} unlabelled literal arguments, more than the \
         {threshold} allowed; nothing at the call site says which is which, so a reader has to \
         go and find the lambda list"
    )
}

/// Whether one node is a call this rule reports.
///
/// The order of these checks is load-bearing, not stylistic. This runs on every
/// node of every definition body, so the cheapest disqualifier has to come
/// first: almost every list in a Lisp file has fewer than five children, and
/// `children.len()` is one integer comparison. Asking the hundred-entry
/// arbitrary-arity table first — which the first draft did — spent 32ms of a
/// 66ms pass on string comparisons for nodes that were about to be rejected on
/// their size anyway.
fn over_long_literal_call(view: &ExpressionView, max_literals: usize) -> Option<(String, usize)> {
    // 1. Size. One integer comparison, and it rejects almost everything.
    if view.children.len() <= max_literals + 1 || !is_paren_list(view) {
        return None;
    }
    let arguments = &view.children[1..];

    // 2. Shape. Bails on the first argument that is not a literal, which for a
    //    long call is almost always the first one.
    let mut kinds: Vec<LiteralKind> = Vec::new();
    for argument in arguments {
        let kind = literal_kind(argument)?;
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    // A homogeneous run of literals is data — a matrix, a colour, a palette —
    // and reads perfectly well. The unreadable shape is the mixed bag.
    if kinds.len() < 2 {
        return None;
    }

    // 3. The operator table, last: by here there are at most a handful of
    //    candidate nodes in the whole file.
    let head = list_head(view)?;
    if symbol_in(head, ARBITRARY_ARITY_OPERATORS) || head.starts_with(':') || head.starts_with('&')
    {
        return None;
    }

    Some((unqualified(head).to_ascii_lowercase(), arguments.len()))
}

/// Examines one definition, reporting the over-long literal calls in its body.
///
/// Reads only the matched node's own subtree, which the dispatcher has already
/// materialized, and prunes at a nested definition — so a file costs one extra
/// pre-order pass in total rather than one per definition.
pub fn examine_definition(
    tree: &SyntaxTree,
    view: &ExpressionView,
    max_literals: usize,
    definition_count: &mut usize,
    violations: &mut Vec<PositionalLiteralCallItem>,
) {
    if !is_paren_list(view)
        || !list_head(view).is_some_and(|head| symbol_in(head, DEFINITION_HEADS))
    {
        return;
    }
    *definition_count += 1;

    let mut candidates = Vec::new();
    // The branch-only walk: a finding here is always a call with at least
    // five arguments, so a node with no children of its own can never be one
    // and can never contain one. Two thirds of an ordinary definition's nodes
    // are exactly that.
    for_each_evaluated_branch_positioned(view, |parent, node| {
        if node.span == view.span {
            return true;
        }
        // The dispatcher will hand a nested definition to this rule on its
        // own; descending into it here would report its calls twice.
        if list_head(node).is_some_and(|head| symbol_in(head, DEFINITION_HEADS)) {
            return false;
        }
        if let Some((head, argument_count)) = over_long_literal_call(node, max_literals) {
            // A clause or a binding is not a call, however much it looks like
            // one. Asked last, because by here at most a handful of nodes in
            // the file are still candidates. Its *body* is still walked either
            // way.
            if parent.is_some_and(|(enclosing, index)| is_clause_position(enclosing, index)) {
                return true;
            }
            candidates.push(PositionalLiteralCallItem {
                span: node.span,
                head,
                argument_count,
                threshold: max_literals,
            });
        }
        true
    });
    if candidates.is_empty() {
        return;
    }
    // Only now, once there is something to report, is the descent worth paying
    // for: a definition inside `'(…)` defines nothing and calls nothing.
    if is_unevaluated_at(tree, view.span) {
        return;
    }
    violations.append(&mut candidates);
}

/// Collects every over-long literal call in one file, with the number of
/// definitions scanned as the denominator beside them.
pub fn build_positional_argument_count_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<PositionalLiteralCallItem>> {
    build_report_with_threshold(path, dialect, tree, DEFAULT_MAX_POSITIONAL_LITERALS)
}

/// [`build_positional_argument_count_report`] at a caller-chosen threshold.
pub fn build_report_with_threshold(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    max_literals: usize,
) -> LintResult<FileFindings<PositionalLiteralCallItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("definition_count", json!(0))],
        ));
    }

    let mut definition_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        paredit_core_syntax::view_query::for_each_subview(&view, |subview| {
            examine_definition(
                tree,
                subview,
                max_literals,
                &mut definition_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("definition_count", json!(definition_count))],
    ))
}

/// The normalized head of a node, for tests and for callers that want to
/// describe a call without re-deriving the spelling.
#[must_use]
pub fn call_head(view: &ExpressionView) -> Option<String> {
    view.children.first().and_then(normalized_symbol)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<PositionalLiteralCallItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_positional_argument_count_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build report")
    }

    fn findings(input: &str) -> Vec<PositionalLiteralCallItem> {
        report(input).findings
    }

    /// Wraps a call in the minimal definition body this rule requires.
    fn in_defun(call: &str) -> String {
        format!("(defun f ()\n  {call})")
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_five_mixed_literal_arguments() {
        let items = findings(&in_defun("(render-panel 10 20 \"status\" t 3)"));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "render-panel");
        assert_eq!(items[0].argument_count, 5);
    }

    #[test]
    fn the_reported_span_is_the_call() {
        let source = in_defun("(render-panel 10 20 \"status\" t 3)");
        let items = findings(&source);
        assert_eq!(
            items[0].span.slice(&source),
            "(render-panel 10 20 \"status\" t 3)"
        );
    }

    #[test]
    fn a_call_nested_deep_in_a_body_is_still_found() {
        assert_eq!(
            findings(&in_defun(
                "(when ready (dolist (x xs) (emit 1 2 3 \"a\" nil)))"
            ))
            .len(),
            1
        );
    }

    #[test]
    fn a_defmacro_and_a_defmethod_body_are_scanned_too() {
        assert_eq!(findings("(defmacro m () (emit 1 2 3 \"a\" nil))").len(), 1);
        assert_eq!(
            findings("(defmethod m ((x t)) (emit 1 2 3 \"a\" nil))").len(),
            1
        );
    }

    /// A nested definition is the dispatcher's job, not the outer walk's.
    #[test]
    fn a_call_inside_a_nested_definition_is_reported_once() {
        let items = findings("(defun outer () (defun inner () (emit 1 2 3 \"a\" nil)))");
        assert_eq!(items.len(), 1, "once, from the inner definition");
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn four_arguments_are_below_the_default_threshold() {
        assert!(findings(&in_defun("(emit 1 2 \"a\" nil)")).is_empty());
    }

    /// A homogeneous run of numbers is data, and reads perfectly well.
    #[test]
    fn a_row_of_numbers_is_data_not_an_argument_list() {
        assert!(findings(&in_defun("(matrix 1 0 0 0 1 0 0 0 1)")).is_empty());
        assert!(findings(&in_defun("(rgba 255 128 0 255)")).is_empty());
        assert!(findings(&in_defun("(coefficients 0.1 0.2 0.3 0.4 0.5 0.6)")).is_empty());
    }

    /// One named argument and the call is readable again.
    #[test]
    fn a_single_non_literal_argument_disqualifies_the_call() {
        assert!(findings(&in_defun("(emit 1 2 3 \"a\" width)")).is_empty());
        assert!(findings(&in_defun("(emit 1 2 3 \"a\" (compute))")).is_empty());
        assert!(findings(&in_defun("(emit 1 2 3 \"a\" 'flag)")).is_empty());
        assert!(findings(&in_defun("(emit 1 2 3 \"a\" #'handler)")).is_empty());
    }

    #[test]
    fn a_keyword_argument_call_is_not_a_positional_one() {
        assert!(
            findings(&in_defun(
                "(make-window \"title\" :width 800 :height 600 :resizable t)"
            ))
            .is_empty()
        );
    }

    #[test]
    fn arbitrary_arity_operators_are_never_reported() {
        for call in [
            "(list 1 2 3 \"a\" t)",
            "(vector 1 2 3 \"a\" t)",
            "(+ 1 2 3 4 5)",
            "(format t \"~a~a~a~a\" 1 2 \"x\" nil)",
            "(and 1 2 3 \"a\" t)",
            "(values 1 2 3 \"a\" t)",
            "(concatenate 'string \"a\" \"b\" \"c\" \"d\" \"e\")",
            "(error \"boom ~a ~a ~a ~a\" 1 2 3 t)",
            "(setf a 1 b 2 c 3 d \"x\" e nil)",
        ] {
            assert!(
                findings(&in_defun(call)).is_empty(),
                "{call} must not be reported"
            );
        }
    }

    #[test]
    fn a_vector_literal_is_not_a_call() {
        assert!(findings(&in_defun("#(1 2 3 \"a\" t)")).is_empty());
    }

    #[test]
    fn a_cond_clause_whose_body_is_literals_is_not_a_call() {
        assert!(findings(&in_defun("(cond (ready 1 2 3 \"a\" nil))")).is_empty());
        assert!(findings(&in_defun("(cond (t 1 2 3 \"a\" nil))")).is_empty());
    }

    /// The rule anchors on a definition, so a top-level call is out of scope.
    #[test]
    fn a_call_outside_any_definition_is_a_deliberate_false_negative() {
        assert!(findings("(emit 1 2 3 \"a\" nil)").is_empty());
    }

    /// A realistic, correct file.
    #[test]
    fn idiomatic_code_is_silent() {
        let source = "(defun palette ()\n  (list (rgb 255 0 0) (rgb 0 255 0) (rgb 0 0 255)))\n\n\
             (defun report (stream results)\n  (format stream \"~&~a passed, ~a failed, ~a skipped~%\"\n          (count :pass results) (count :fail results) (count :skip results)))\n\n\
             (defmethod draw ((w window) canvas)\n  (fill-rectangle canvas (window-x w) (window-y w) (window-width w) (window-height w)))\n\n\
             (defun make-default-config ()\n  (make-config :host \"localhost\" :port 8080 :tls nil :retries 3))\n";
        assert!(findings(source).is_empty());
    }

    // -- the five quote shapes ----------------------------------------------

    const CALL: &str = "(defun f () (emit 1 2 3 \"a\" nil))";

    #[test]
    fn a_hard_quoted_definition_is_data() {
        assert!(findings(&format!("'{CALL}")).is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data() {
        assert!(findings(&format!("(quote {CALL})")).is_empty());
    }

    #[test]
    fn a_quasiquoted_definition_without_an_unquote_is_data() {
        assert!(findings(&format!("`{CALL}")).is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(findings(&format!("'(x ,{CALL})")).is_empty());
    }

    #[test]
    fn an_unquoted_definition_inside_a_quasiquote_is_code_again() {
        assert_eq!(findings(&format!("`(x ,{CALL})")).len(), 1);
    }

    /// A quoted call *inside* a live definition is data too, which the
    /// evaluated walk answers without a descent.
    #[test]
    fn a_quoted_call_inside_a_live_definition_is_data() {
        assert!(findings("(defun f () '(emit 1 2 3 \"a\" nil))").is_empty());
        assert!(findings("(defun f () `(emit 1 2 3 \"a\" nil))").is_empty());
        assert!(findings("(defun f () '(x ,(emit 1 2 3 \"a\" nil)))").is_empty());
        assert_eq!(
            findings("(defun f () `(x ,(emit 1 2 3 \"a\" nil)))").len(),
            1,
            "an unquote is code again"
        );
    }

    #[test]
    fn a_call_spelled_only_inside_a_string_is_never_a_form() {
        assert!(findings("(defun f () (format nil \"(emit 1 2 3 \\\"a\\\" nil)\"))").is_empty());
    }

    // -- thresholds, dialects, denominators ----------------------------------

    #[test]
    fn the_threshold_moves_what_is_reported() {
        let source = in_defun("(emit 1 2 \"a\" nil)");
        let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).expect("parse");
        let strict =
            build_report_with_threshold(Path::new("t.lisp"), Dialect::CommonLisp, &tree, 3)
                .expect("report");
        assert_eq!(strict.findings.len(), 1);
        let lenient =
            build_report_with_threshold(Path::new("t.lisp"), Dialect::CommonLisp, &tree, 4)
                .expect("report");
        assert!(lenient.findings.is_empty());
    }

    #[test]
    fn a_dialect_this_rule_does_not_model_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(CALL, Dialect::EmacsLisp).expect("parse");
        let report =
            build_positional_argument_count_report(Path::new("t.el"), Dialect::EmacsLisp, &tree)
                .expect("report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_definition_scanned() {
        let report = report("(defun a () 1)\n(defun b () (emit 1 2 3 \"a\" nil))\n");
        assert_eq!(report.summary, vec![("definition_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_argument_count() {
        let report = report("(defun f ()\n  (emit 1 2 3 \"a\" nil))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(
            finding.kind(),
            "positional-argument-count-exceeds-readability"
        );
        assert_eq!(finding.text_columns(), vec!["argument_count=5".to_owned()]);
        assert!(finding.message().contains("5 unlabelled literal arguments"));
    }

    #[test]
    fn literal_kinds_are_classified_and_names_are_not_literals() {
        let source = "(f 1 -2 3.5 1/2 \"s\" #\\a t nil x 'y #'z (g))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let arguments = &root.children[0].children[1..];
        let kinds: Vec<Option<LiteralKind>> = arguments.iter().map(literal_kind).collect();
        assert_eq!(
            kinds,
            vec![
                Some(LiteralKind::Number),
                Some(LiteralKind::Number),
                Some(LiteralKind::Number),
                Some(LiteralKind::Number),
                Some(LiteralKind::String),
                Some(LiteralKind::Character),
                Some(LiteralKind::Boolean),
                Some(LiteralKind::Boolean),
                None, // x
                None, // 'y
                None, // #'z
                None, // (g)
            ]
        );
        assert_eq!(call_head(&root.children[0]).as_deref(), Some("f"));
    }
}
