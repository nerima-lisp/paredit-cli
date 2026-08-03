//! `(defconstant +x+ (list 1 2))` — a constant whose value is a freshly
//! allocated object, and so is not `eql` to itself across the
//! compile-time/load-time boundary.
//!
//! CLHS `defconstant` is explicit: *"If defconstant is used to redefine a name
//! ... the consequences are undefined unless the new value is eql to the old."*
//! `defconstant` also has an implicit compile-time effect, so `compile-file`
//! evaluates the initform once at compile time and the fasl evaluates it again
//! at load time. When the initform allocates, those are two distinct objects and
//! the second `defconstant` is a redefinition to a non-`eql` value.
//!
//! # This fires on the *first* build, not on a re-evaluation
//!
//! The proposal for this rule described the failure as "re-evaluating the file
//! signals". Measured against SBCL 2.6.0, it is worse than that: a **fresh
//! image**, compiling and loading the file exactly once, already signals.
//!
//! ```text
//! (load "src.lisp")                       => no error, no warning
//! (load (compile-file "src.lisp"))        => DEFCONSTANT-UNEQL
//!    "The constant +LIST+ is being redefined (from (1 2) to (1 2))"
//! ```
//!
//! Note the message: the two values *print identically*. Nothing about the
//! error tells the reader that identity rather than value is the problem, and
//! `compile-file` itself reported `warnings-p = NIL, failure-p = NIL` before the
//! load failed. That is the ASDF workflow — compile then load, in one image — so
//! this is not an exotic path.
//!
//! # Which initforms are flagged, and why the list is short
//!
//! Every entry below was run through `compile-file` + `load` of the fasl in a
//! fresh SBCL 2.6.0, defining a **uniquely named** constant per case. (A first
//! version of that probe reused one constant name and reported *every* initform
//! as unequal, including `42` and `nil` — an all-positive harness is as broken
//! as an all-negative one, and the controls below are what caught it.)
//!
//! | initform | result | flagged |
//! | --- | --- | --- |
//! | `(list 1 2)` | `DEFCONSTANT-UNEQL` | yes |
//! | `'(1 2)` | `DEFCONSTANT-UNEQL` | yes |
//! | `"hello"` | `DEFCONSTANT-UNEQL` | yes |
//! | `#(1 2 3)` | `DEFCONSTANT-UNEQL` | yes |
//! | `(vector 1 2)` | `DEFCONSTANT-UNEQL` | yes |
//! | `(make-array 3)` | `DEFCONSTANT-UNEQL` | yes |
//! | `(copy-seq "ab")` | `DEFCONSTANT-UNEQL` | yes |
//! | `42`, `12345678901234567890`, `1.5d0`, `1/3` | ok | no |
//! | `'foo`, `:foo`, `#\a`, `nil`, `t` | ok | no |
//! | `(+ 1 2)`, `(if t 1 2)`, `most-positive-fixnum` | ok | no |
//! | `(coerce 1 'double-float)` | ok | no |
//!
//! The last row of "ok" results is the important one for the rule's shape: an
//! arbitrary *call* is not evidence of anything. `(+ 1 2)` returns a fixnum and
//! is fine; `(list 1 2)` returns a fresh cons and is not. So the rule cannot key
//! on "the initform is a call" — it keys on a **closed list of constructors
//! that provably allocate**, and says nothing about any other call. A
//! `(defconstant +x+ (compute-it))` is left alone, because whether `compute-it`
//! conses is not a question the source answers.
//!
//! Literals are classified the same way and for the same reason: a string, a
//! `#(…)` vector and a quoted *list* are freshly constructed by the fasl loader,
//! while a number, a character, a symbol, a keyword, `t` and `nil` are not.
//! `'()` is `nil` and is therefore stable — which is why the quoted-list test
//! asks for a non-empty list rather than for a `(…)` shape.
//!
//! # Deliberate limits
//!
//! - **Top level only**, in the CLHS 3.2.3.1 sense. A `defconstant` nested
//!   inside a `defun` has no compile-time effect, so there is no second
//!   evaluation and no collision.
//! - **No fix.** The repair is a judgement: `alexandria:define-constant` with
//!   an appropriate `:test`, or `defparameter`, or hoisting the value into a
//!   `load-time-value`. Which one depends on whether the constant is compared,
//!   mutated, or merely read. The message names `define-constant` because that
//!   is what the idiom exists for, but choosing the `:test` is the author's.
//! - **`alexandria:define-constant` is not flagged**, and needs no exclusion:
//!   its head normalizes to `define-constant`, which is not this rule's head.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head};
use serde_json::{Value, json};

use crate::support::{
    carries_reader_conditional, for_each_evaluated_subview, is_top_level_form, mentions,
    normalized_symbol,
};

/// The byte-scan needle, and the head this rule anchors on.
pub const DEFCONSTANT: &str = "defconstant";

/// Operators that **always** return a freshly allocated object of a type `eql`
/// compares by identity.
///
/// Closed and deliberately short. Every entry has to be true of *every* call to
/// it, including degenerate ones, which is why several plausible candidates are
/// absent:
///
/// - `append` — `(append)` is `nil` and `(append x)` is `x`; neither allocates.
/// - `make-list` — `(make-list 0)` is `nil`.
/// - `list*` — `(list* x)` is `x`.
/// - `concatenate` — `(concatenate 'list)` is `nil`.
/// - `coerce` — measured returning a `double-float` that compares `eql`.
/// - `reverse`, `sort`, `remove`, `subseq`, `mapcar` — all can return an
///   argument, or `nil`, for an empty or degenerate input.
///
/// `list` and `cons` are the two that survive with a guard rather than
/// unconditionally: `(list)` is `nil`, so `list` needs at least one argument;
/// `cons` always conses.
const ALWAYS_FRESH: [&str; 8] = [
    "cons",
    "vector",
    "make-array",
    "make-string",
    "copy-seq",
    "copy-list",
    "copy-tree",
    "copy-alist",
];

/// What the source says about whether an initform's value is `eql` to itself
/// across the compile/load boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueStability {
    /// Provably a fresh object each time it is evaluated.
    Fresh,
    /// Provably `eql` to itself: a number, character, symbol, keyword, `t`,
    /// `nil`.
    Stable,
    /// Not decidable from the source. Never reported.
    Unknown,
}

/// Whether an atom's printed representation is one of the self-`eql` types.
///
/// Numbers, characters, keywords, `t`, `nil` and bare symbols all read as atoms
/// whose value is `eql` to itself. A *string* is an atom too, and is not.
fn classify_atom(text: &str) -> ValueStability {
    if carries_reader_conditional(text) {
        return ValueStability::Unknown;
    }
    // A string literal is freshly constructed by the loader.
    if text.starts_with('"') {
        return ValueStability::Fresh;
    }
    // `#(…)` and `#*…` read as self-evaluating aggregates, freshly built.
    if text.starts_with("#(") || text.starts_with("#*") {
        return ValueStability::Fresh;
    }
    // `#\a`, and every numeric or symbolic atom, is stable. A bare symbol is a
    // *variable reference* rather than a literal, so it is not decidable — but
    // a reference to another constant is the overwhelmingly common case and was
    // measured stable (`most-positive-fixnum`). Being wrong here would mean
    // staying silent, since `Stable` is never reported.
    ValueStability::Stable
}

/// How stable the value of `view` is, read from the source alone.
#[must_use]
pub fn classify(view: &ExpressionView) -> ValueStability {
    // A reader prefix decides the question before the node's own shape does,
    // and the *outermost* one settles it: in `'#(1 2)` the quote already makes
    // the whole thing a literal, and in `#(1 2)` there is only one prefix to
    // read. So this is a first-prefix test, not a fold over all of them.
    if let Some(prefix) = view.reader_prefixes.first() {
        match prefix {
            ReaderPrefix::Quote => {
                // `'(1 2)` is a fresh list; `'foo` is a symbol; `'()` is nil.
                return if is_paren_list(view) && !view.children.is_empty() {
                    ValueStability::Fresh
                } else {
                    ValueStability::Stable
                };
            }
            // `#(1 2 3)` is a *prefixed list*, not an atom: the reader keeps the
            // bare `#` attached to the following collection as `HashLiteral`.
            // In Common Lisp that is a vector literal, which the fasl loader
            // builds fresh. (Clojure's `#{…}` set and `#(…)` lambda share the
            // prefix, which is why this rule is Common Lisp only.)
            ReaderPrefix::HashLiteral => return ValueStability::Fresh,
            // A backquote may or may not allocate depending on its unquotes, and
            // `#.` was already resolved by the reader into whatever it produced.
            ReaderPrefix::Quasiquote => return ValueStability::Unknown,
            _ => return ValueStability::Unknown,
        }
    }
    if let Some(text) = atom_text(view) {
        return classify_atom(text);
    }
    if !is_paren_list(view) {
        return ValueStability::Unknown;
    }
    let Some(head) = list_head(view) else {
        // `()` reads as nil.
        return ValueStability::Stable;
    };
    let head = normalized_symbol(head);
    // `(quote (1 2))`, the long-hand spelling.
    if head == "quote" {
        return view.children.get(1).map_or(ValueStability::Unknown, |q| {
            if is_paren_list(q) && !q.children.is_empty() {
                ValueStability::Fresh
            } else {
                ValueStability::Stable
            }
        });
    }
    if ALWAYS_FRESH.contains(&head.as_str()) {
        return ValueStability::Fresh;
    }
    // `(list)` is nil; `(list x …)` is a fresh cons.
    if head == "list" {
        return if view.children.len() > 1 {
            ValueStability::Fresh
        } else {
            ValueStability::Stable
        };
    }
    // `(format nil …)` always builds a fresh string; `(format t …)` returns nil
    // and `(format stream …)` is not decidable.
    if head == "format" {
        return view.children.get(1).map_or(
            ValueStability::Unknown,
            |destination| match atom_text(destination).map(normalized_symbol).as_deref() {
                Some("nil") => ValueStability::Fresh,
                Some("t") => ValueStability::Stable,
                _ => ValueStability::Unknown,
            },
        );
    }
    // Any other call: the source does not say.
    ValueStability::Unknown
}

#[derive(Debug, Clone)]
pub struct DefconstantNonEqlValueItem {
    /// The span of the whole `defconstant`.
    pub span: ByteSpan,
    /// The constant's name as written.
    pub name: String,
    /// The initform as written, so the reader can see what allocates.
    pub initform: String,
}

impl Finding for DefconstantNonEqlValueItem {
    fn kind(&self) -> &'static str {
        "defconstant-non-eql-value"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("name={}", self.name),
            format!("initform={}", self.initform),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("initform", json!(self.initform)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} is defined with {}, which allocates a new object every time it is evaluated; \
             compile-file evaluates it once at compile time and the fasl evaluates it again at \
             load time, so loading the compiled file signals DEFCONSTANT-UNEQL even on a first \
             build (alexandria:define-constant with a :test exists for this)",
            self.name, self.initform
        )
    }
}

/// Whether `view` is a `(defconstant …)` form.
#[must_use]
pub fn is_defconstant(view: &ExpressionView) -> bool {
    is_paren_list(view)
        && list_head(view).is_some_and(|head| normalized_symbol(head) == DEFCONSTANT)
}

/// The finding this `defconstant` implies, if any.
///
/// **Ordering is load-bearing.** [`classify`] reads the initform node and
/// nothing else, and answers `Stable` or `Unknown` — no finding — for every
/// `defconstant` except the ones this rule is about. Only a form that has
/// already been classified `Fresh` reaches [`is_top_level_form`], which
/// materializes the enclosing top-level form.
#[must_use]
pub fn examine_defconstant(
    tree: &SyntaxTree,
    view: &ExpressionView,
) -> Option<DefconstantNonEqlValueItem> {
    // 1. node-local: does the initform provably allocate?
    let initform = view.children.get(2)?;
    if classify(initform) != ValueStability::Fresh {
        return None;
    }
    let name = view.children.get(1).and_then(atom_text)?;
    if carries_reader_conditional(name) {
        return None;
    }
    // 2. only now, the tree. A nested `defconstant` has no compile-time effect,
    //    so there is no second evaluation to collide with.
    if !is_top_level_form(tree, view.span) {
        return None;
    }
    Some(DefconstantNonEqlValueItem {
        span: view.span,
        name: name.to_owned(),
        initform: initform_text(tree, initform),
    })
}

/// The initform exactly as written, truncated so a large literal table does not
/// become the whole message.
fn initform_text(tree: &SyntaxTree, initform: &ExpressionView) -> String {
    const LIMIT: usize = 40;
    let text = tree
        .source()
        .get(initform.span.start().get()..initform.span.end().get())
        .unwrap_or_default();
    let flattened = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.chars().count() > LIMIT {
        let kept: String = flattened.chars().take(LIMIT).collect();
        format!("{kept}…")
    } else {
        flattened
    }
}

fn collect(tree: &SyntaxTree) -> (Vec<DefconstantNonEqlValueItem>, usize) {
    let mut findings = Vec::new();
    let mut candidates = 0;
    for_each_evaluated_subview(&tree.root_view(), |view| {
        if !is_defconstant(view) {
            return;
        }
        candidates += 1;
        if let Some(item) = examine_defconstant(tree, view) {
            findings.push(item);
        }
    });
    (findings, candidates)
}

/// Collects the file's findings with the number of `defconstant` forms scanned
/// as the denominator beside it.
pub fn build_defconstant_non_eql_value_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DefconstantNonEqlValueItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("defconstant_count", json!(0))],
        ));
    }
    if !mentions(tree.source(), DEFCONSTANT) {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            true,
            tree.source(),
            Vec::new(),
            vec![("defconstant_count", json!(0))],
        ));
    }
    let (findings, candidates) = collect(tree);
    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        findings,
        vec![("defconstant_count", json!(candidates))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DefconstantNonEqlValueItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_defconstant_non_eql_value_report(Path::new("app.lisp"), Dialect::CommonLisp, &tree)
            .expect("build defconstant-non-eql-value report")
    }

    fn findings(input: &str) -> Vec<DefconstantNonEqlValueItem> {
        report(input).findings
    }

    fn candidates(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "defconstant_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("defconstant_count in the summary")
    }

    fn stability(initform: &str) -> ValueStability {
        let source = format!("(defconstant +x+ {initform})");
        let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).expect("parse");
        let form = tree.root_view().children[0].clone();
        classify(&form.children[2])
    }

    // --- the measured table, as a test

    /// Every one of these was confirmed to signal `DEFCONSTANT-UNEQL` under
    /// `compile-file` + `load` of the fasl in a fresh SBCL 2.6.0.
    #[test]
    fn the_initforms_sbcl_reported_uneql_are_classified_fresh() {
        for initform in [
            "(list 1 2)",
            "'(1 2)",
            "\"hello\"",
            "#(1 2 3)",
            "(vector 1 2)",
            "(make-array 3)",
            "(copy-seq \"ab\")",
        ] {
            assert_eq!(
                stability(initform),
                ValueStability::Fresh,
                "{initform} was measured UNEQL but is not classified Fresh"
            );
        }
    }

    /// The controls. Every one of these was confirmed to load cleanly.
    #[test]
    fn the_initforms_sbcl_accepted_are_never_classified_fresh() {
        for initform in [
            "42",
            "12345678901234567890",
            "1.5d0",
            "1/3",
            "'foo",
            ":foo",
            "#\\a",
            "nil",
            "t",
            "(+ 1 2)",
            "(if t 1 2)",
            "most-positive-fixnum",
            "(coerce 1 'double-float)",
        ] {
            assert_ne!(
                stability(initform),
                ValueStability::Fresh,
                "{initform} loads cleanly but is classified Fresh"
            );
        }
    }

    /// The degenerate calls that make several plausible constructors unsound.
    #[test]
    fn the_constructors_that_can_return_a_non_fresh_value_are_not_flagged() {
        for initform in [
            "(list)",
            "(append)",
            "(append x)",
            "(make-list 0)",
            "(list* x)",
            "(concatenate 'list)",
            "(reverse x)",
            "(subseq x 0)",
        ] {
            assert_ne!(
                stability(initform),
                ValueStability::Fresh,
                "{initform} can return a non-fresh value"
            );
        }
    }

    #[test]
    fn an_empty_quoted_list_is_nil_and_is_stable() {
        assert_eq!(stability("'()"), ValueStability::Stable);
        assert_eq!(stability("'nil"), ValueStability::Stable);
    }

    #[test]
    fn the_long_hand_quote_is_classified_like_the_reader_macro() {
        assert_eq!(stability("(quote (1 2))"), ValueStability::Fresh);
        assert_eq!(stability("(quote foo)"), ValueStability::Stable);
        assert_eq!(stability("(quote ())"), ValueStability::Stable);
    }

    #[test]
    fn an_unknown_call_says_nothing() {
        assert_eq!(stability("(compute-it)"), ValueStability::Unknown);
        assert_eq!(
            stability("(my-package:build-table)"),
            ValueStability::Unknown
        );
    }

    #[test]
    fn format_is_classified_by_its_destination() {
        assert_eq!(stability("(format nil \"~a\" x)"), ValueStability::Fresh);
        assert_eq!(stability("(format t \"~a\" x)"), ValueStability::Stable);
        assert_eq!(stability("(format s \"~a\" x)"), ValueStability::Unknown);
    }

    // --- findings

    #[test]
    fn flags_a_list_valued_constant() {
        let found = findings("(defconstant +limits+ (list 1 2))\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "+limits+");
        assert_eq!(found[0].initform, "(list 1 2)");
        assert!(found[0].message().contains("define-constant"));
    }

    #[test]
    fn flags_a_string_valued_constant() {
        let found = findings("(defconstant +greeting+ \"hello\")\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].initform, "\"hello\"");
    }

    #[test]
    fn a_long_initform_is_truncated_in_the_finding() {
        let found = findings(
            "(defconstant +table+ (list :aaaaaaaaaa :bbbbbbbbbb :cccccccccc :dddddddddd :eeeeeeeeee))\n",
        );
        assert!(found[0].initform.ends_with('…'));
        assert!(found[0].initform.chars().count() <= 41);
    }

    #[test]
    fn does_not_flag_a_numeric_or_symbolic_constant() {
        assert!(findings("(defconstant +limit+ 100)\n").is_empty());
        assert!(findings("(defconstant +mode+ :fast)\n").is_empty());
        assert!(findings("(defconstant +none+ nil)\n").is_empty());
    }

    /// The head is `define-constant`, not `defconstant`, so the idiom this rule
    /// recommends is never itself reported.
    #[test]
    fn does_not_flag_the_alexandria_idiom() {
        assert!(
            findings("(alexandria:define-constant +greeting+ \"hello\" :test #'string=)\n")
                .is_empty()
        );
        assert_eq!(
            candidates("(alexandria:define-constant +g+ \"h\" :test #'string=)\n"),
            0
        );
    }

    /// A nested `defconstant` has no compile-time effect and so no second
    /// evaluation to collide with.
    #[test]
    fn does_not_flag_a_non_top_level_defconstant() {
        assert!(findings("(defun f () (defconstant +x+ (list 1 2)))\n").is_empty());
        assert!(findings("(let () (defconstant +x+ \"s\"))\n").is_empty());
    }

    #[test]
    fn flags_it_inside_the_top_level_preserving_operators() {
        for source in [
            "(progn (defconstant +x+ (list 1 2)))",
            "(eval-when (:compile-toplevel :load-toplevel :execute) (defconstant +x+ \"s\"))",
            "(locally (defconstant +x+ #(1 2)))",
        ] {
            assert_eq!(findings(source).len(), 1, "missed: {source}");
        }
    }

    #[test]
    fn does_not_flag_a_reader_conditional_name_or_initform() {
        assert!(findings("(defconstant #+sbcl +x+ (list 1 2))\n").is_empty());
    }

    // --- quote negatives

    #[test]
    fn quoted_data_is_never_a_finding() {
        for source in [
            "'(defconstant +x+ (list 1 2))",
            "(quote (defconstant +x+ \"s\"))",
            "`(defconstant +x+ (list 1 2))",
            "'(a ,(defconstant +x+ \"s\"))",
        ] {
            assert!(findings(source).is_empty(), "flagged data: {source}");
        }
    }

    #[test]
    fn a_defconstant_template_in_a_macro_body_is_data() {
        assert!(findings("(defmacro m (n) `(defconstant ,n (list 1 2)))\n").is_empty());
    }

    // --- denominator and envelope

    #[test]
    fn the_denominator_counts_every_defconstant_reached_as_code() {
        assert_eq!(
            candidates("(defconstant +a+ 1)\n(defconstant +b+ (list 1))\n(defun f () 1)\n"),
            2
        );
        assert_eq!(candidates("(defparameter *a* (list 1))\n"), 0);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(defconstant +x+ (list 1))", Dialect::Clojure)
            .expect("parse");
        let report =
            build_defconstant_non_eql_value_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_fields() {
        let report = report("\n(defconstant +x+ (list 1 2))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "defconstant-non-eql-value");
        assert_eq!(
            finding.json_fields(),
            vec![("name", json!("+x+")), ("initform", json!("(list 1 2)"))]
        );
    }
}
