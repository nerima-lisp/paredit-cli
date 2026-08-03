//! A `def` evaluated inside a function body.
//!
//! `def` is a special form that interns a Var in the **namespace**, not in any
//! local scope. Writing one inside a function does not make a local: it makes a
//! namespace-global whose value is rewritten every time the function is called,
//! by whichever thread got there last.
//!
//! ```clojure
//! (defn load-config! [path]
//!   (def config (read-edn path))   ; a namespace Var, created at call time
//!   config)
//! ```
//!
//! Three things go wrong at once. The Var does not exist until the function has
//! been called, so anything that reads it at load time sees an unbound Var; two
//! callers race on it; and the name is invisible to a reader scanning the
//! namespace's top level for its definitions. The fix is almost always `let`,
//! or a top-level `defonce`/`delay` for the memoized case.
//!
//! # What it looks at
//!
//! The body of a `defn`, `defn-` or `defmethod`, and every evaluated form
//! underneath it — including inside a nested `fn`, `let`, `loop` or `#(…)`.
//! A `def`, `defn`, `defn-` or `defonce` found there is reported.
//!
//! # What it does not attempt
//!
//! - **A bare `(fn …)` or `letfn` at the top level.** `(def handler (fn []
//!   (def x 1)))` is the same defect and is not reported. Anchoring on the
//!   function-*defining* heads instead of on `fn` is what keeps this rule from
//!   reporting the same inner `def` once per enclosing closure — `(defn f []
//!   (fn [] (def x 1)))` has two function scopes around one defect, and a
//!   reader needs one finding, not two.
//! - **`defmacro`.** A macro that *emits* a `def` is how `defonce`,
//!   `defprotocol` and every project's own definer are written. The syntax-quote
//!   in `` `(def ~name …) `` already reads as data, but an unquoted `(def …)`
//!   inside a macro body is far more likely to be a template than a defect, so
//!   `defmacro` is not an anchor at all.
//! - **`defmethod` bodies that register.** `defmethod` *is* an anchor, because
//!   a `def` in a method body is the same namespace write as anywhere else.
//! - **`intern`, `alter-var-root`, `with-redefs`.** All of them write a Var at
//!   run time on purpose, and all of them say so at the call site.
//!
//! # Prior art
//!
//! This is clj-kondo's `:inline-def`, which is on by default there — the
//! independent evidence that the shape is a defect rather than a style
//! preference.
//!
//! Scope: Clojure only. Common Lisp's `defvar`/`defparameter` inside a `defun`
//! is a different question with a different answer, and `def` is not an
//! operator there at all.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::symbol_in;
use serde_json::{Value, json};

use crate::support::{
    for_each_evaluated_call, for_each_evaluated_subview, head_is, normalized_symbol,
    skip_metadata_forms, symbol_name,
};

/// The forms this rule descends into.
///
/// The function *definers*, not `fn` itself: see the module documentation for
/// why anchoring on `fn` would report one defect once per enclosing closure.
pub const FUNCTION_DEFINITION_HEADS: &[&str] = &["defn", "defn-", "defmethod"];

/// The definition forms that intern a namespace Var when evaluated.
///
/// `defmulti`, `defrecord`, `deftype` and `defprotocol` are absent: each does
/// far more than intern a Var (installs a dispatch table, generates a class),
/// and a rule that lumps them in here would be phrasing a different complaint
/// under this one's message.
pub const NAMESPACE_DEFINITION_HEADS: &[&str] = &["def", "defn", "defn-", "defonce"];

/// Whether a head could possibly be one of [`NAMESPACE_DEFINITION_HEADS`],
/// decided from its first three bytes.
///
/// This is the rule's cost fix, and it is worth the paragraph. Unlike the other
/// four rules here, this one examines a whole *subtree* per head match — the
/// body of every `defn` in the file — so its per-node work is multiplied by the
/// file's size rather than by the number of candidates. Measured against the
/// shipped `atom-swap-with-side-effect` in one timed pass, the straightforward
/// spelling (`head_is(node, NAMESPACE_DEFINITION_HEADS)` on every node) cost
/// **1313 ns/invocation against the control's 43** — thirty times the control,
/// and 82% of this package's entire cost.
///
/// The reason is `symbol_in`, which calls `unqualified` once per candidate
/// name: `unqualified` is an `rsplit_once(':')`, so it scans the whole symbol,
/// and four of them ran on every list node in every function body. Rejecting on
/// three bytes first turns that into one length check and one 3-byte compare.
///
/// It is *exact* rather than approximate, which is the only reason it is
/// allowed to be a pre-filter and not a semantic change: every name in
/// [`NAMESPACE_DEFINITION_HEADS`] begins with `def`, and Clojure's namespace
/// separator is `/`, which `unqualified` does not strip — so in this dialect a
/// head is already its own unqualified form and the two tests agree on every
/// input. (In Common Lisp, where `unqualified` does strip `pkg:`, it would not;
/// this rule is `CLOJURE_ONLY`.)
fn may_be_definition_head(head: &str) -> bool {
    head.len() >= 3 && head.as_bytes()[..3].eq_ignore_ascii_case(b"def")
}

#[derive(Debug, Clone)]
pub struct DefInsideFunctionBodyItem {
    /// The span of the inner definition form.
    pub span: ByteSpan,
    /// The definer, normalized: `def`, `defn`, `defn-` or `defonce`.
    pub definer: String,
    /// The name it interns, when it names one literally.
    pub name: String,
    /// The function definition it is written inside.
    pub enclosing: String,
}

impl Finding for DefInsideFunctionBodyItem {
    fn kind(&self) -> &'static str {
        "def-inside-function-body"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("definer={}", self.definer),
            format!("name={}", self.name),
            format!("enclosing={}", self.enclosing),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("definer", json!(self.definer)),
            ("name", json!(self.name)),
            ("enclosing", json!(self.enclosing)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} interns the namespace Var {} every time {} is called; use let, \
             or a top-level definition",
            self.definer, self.name, self.enclosing
        )
    }
}

/// The name a definition form defines, past any metadata.
///
/// `(defn ^:private f [] …)` puts `^:private` at child 1 and `f` at child 2 —
/// the reader makes Clojure metadata a *sibling* of what it annotates, not a
/// prefix on it — so reading child 1 unconditionally would report the function
/// as being called `:private`. `^:private` is on a large fraction of real
/// `defn-`-adjacent code, so this is not an edge case.
fn defined_symbol(view: &ExpressionView) -> Option<String> {
    skip_metadata_forms(&view.children)
        .nth(1)
        .and_then(symbol_name)
}

/// The name a definition form interns, or `?` when it is computed.
fn defined_name(view: &ExpressionView) -> String {
    defined_symbol(view).unwrap_or_else(|| "?".to_owned())
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_function_body(
    view: &ExpressionView,
    function_count: &mut usize,
    violations: &mut Vec<DefInsideFunctionBodyItem>,
) {
    if !head_is(view, FUNCTION_DEFINITION_HEADS) {
        return;
    }
    *function_count += 1;

    let Some(enclosing) = defined_symbol(view) else {
        return;
    };

    for_each_evaluated_call(view, |node, head| {
        // The anchor is itself a `defn`, which is in the reported set. Skipping
        // it by identity rather than by head keeps a nested `defn` reportable.
        if std::ptr::eq(node, view) {
            return;
        }
        // Three bytes before four `symbol_in` comparisons: see
        // `may_be_definition_head`. This is the hot line of the package.
        if !may_be_definition_head(head) || !symbol_in(head, NAMESPACE_DEFINITION_HEADS) {
            return;
        }
        violations.push(DefInsideFunctionBodyItem {
            span: node.span,
            definer: normalized_symbol(head),
            name: defined_name(node),
            enclosing: enclosing.clone(),
        });
    });
}

/// Collects every namespace definition written inside a function in one file,
/// with the number of function definitions scanned as the denominator.
pub fn build_def_inside_function_body_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DefInsideFunctionBodyItem>> {
    let mut function_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::Clojure {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_function_body(view, &mut function_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::Clojure,
        tree.source(),
        violations,
        vec![("function_definition_count", json!(function_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DefInsideFunctionBodyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_def_inside_function_body_report(Path::new("test.clj"), Dialect::Clojure, &tree)
            .expect("build report")
    }

    fn violations(input: &str) -> Vec<DefInsideFunctionBodyItem> {
        report(input).findings
    }

    fn names(input: &str) -> Vec<String> {
        violations(input)
            .into_iter()
            .map(|item| item.name)
            .collect()
    }

    // --- the defect ----------------------------------------------------------

    #[test]
    fn flags_a_def_in_a_function_body() {
        assert_eq!(
            names("(defn load! [p] (def config (read-edn p)) config)"),
            vec!["config".to_owned()]
        );
    }

    #[test]
    fn flags_a_def_nested_in_a_let_or_when() {
        assert_eq!(
            names("(defn f [x] (let [y 1] (def cached y)))"),
            vec!["cached".to_owned()]
        );
        assert_eq!(
            names("(defn f [x] (when x (def flag true)))"),
            vec!["flag".to_owned()]
        );
    }

    /// The reason `fn` is not an anchor: one defect, one finding, even though
    /// two function scopes enclose it.
    #[test]
    fn flags_a_def_inside_a_nested_closure_exactly_once() {
        assert_eq!(names("(defn f [] (fn [] (def x 1)))"), vec!["x".to_owned()]);
        assert_eq!(names("(defn f [] #(def y %))"), vec!["y".to_owned()]);
    }

    #[test]
    fn flags_an_inner_defn() {
        let items = violations("(defn outer [] (defn inner [] 1))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].definer, "defn");
        assert_eq!(items[0].name, "inner");
        assert_eq!(items[0].enclosing, "outer");
    }

    #[test]
    fn flags_a_defonce_in_a_function_body() {
        assert_eq!(
            names("(defn init [] (defonce pool (make-pool)))"),
            vec!["pool".to_owned()]
        );
    }

    #[test]
    fn flags_a_def_in_a_defmethod_body() {
        assert_eq!(
            names("(defmethod render :text [x] (def last-rendered x) x)"),
            vec!["last-rendered".to_owned()]
        );
    }

    // --- realistic, correct Clojure that must stay silent --------------------

    #[test]
    fn does_not_flag_a_top_level_definition() {
        assert!(violations("(def config {:a 1})\n(defn f [] config)").is_empty());
        assert!(violations("(defonce pool (make-pool))").is_empty());
        assert!(violations("(defn f [] 1)\n(defn g [] 2)").is_empty());
    }

    #[test]
    fn does_not_flag_a_let_binding_which_is_the_repair() {
        assert!(violations("(defn load! [p] (let [config (read-edn p)] config))").is_empty());
    }

    /// A macro that emits a `def` is how every project's own definer is
    /// written, and `defmacro` is deliberately not an anchor.
    #[test]
    fn does_not_flag_a_def_in_a_macro_template() {
        assert!(violations("(defmacro defthing [n v] `(def ~n ~v))").is_empty());
        assert!(violations("(defmacro defthing [n v] (list 'def n v))").is_empty());
    }

    #[test]
    fn does_not_flag_the_run_time_var_writers_that_say_so() {
        assert!(
            violations("(defn reload! [] (alter-var-root #'config (constantly 1)))").is_empty()
        );
        assert!(violations("(defn t [] (with-redefs [config {:a 2}] (run)))").is_empty());
        assert!(violations("(defn make [n] (intern *ns* n 1))").is_empty());
    }

    /// A `case` test constant is a *list of symbols*, not a call.
    ///
    /// This is the false positive the corpus audit found — twice, in
    /// `clj-kondo`'s own `src/clj_kondo/impl/analyzer.clj`, at the two clauses
    /// that dispatch on the definer names. The shape below is that code,
    /// reduced. `case` never evaluates its test constants, and nothing in the
    /// reader marks them: no quote, no prefix. Read as code, `(def defonce
    /// defmulti goog-define)` is a textbook inline `def`.
    #[test]
    fn does_not_flag_a_definer_name_used_as_a_case_test_constant() {
        assert!(
            violations(
                "(defn analyze [tag expr]\n  \
                 (case tag\n    \
                 (def defonce defmulti goog-define) (analyze-def expr)\n    \
                 (defn defn- defmacro definline) (analyze-defn expr)\n    \
                 (analyze-other expr)))"
            )
            .is_empty()
        );
    }

    /// The other half of that fix: a real `def` in a `case` *result* must
    /// still be reported. Over-suppression is the default failure mode of a
    /// false-positive fix, and this is the negative control for it.
    #[test]
    fn still_flags_a_def_in_a_case_result_expression() {
        assert_eq!(
            names("(defn f [tag] (case tag :a (def x 1) :b 2))"),
            vec!["x".to_owned()]
        );
        // And in the trailing default clause, which is a result and not a test.
        assert_eq!(
            names("(defn f [tag] (case tag :a 1 (def y 2)))"),
            vec!["y".to_owned()]
        );
    }

    #[test]
    fn does_not_flag_a_defmulti_or_defrecord_inside_a_function() {
        // A different complaint with a different message; see the head list.
        assert!(violations("(defn f [] (defrecord R [a]))").is_empty());
        assert!(violations("(defn f [] (defmulti d :k))").is_empty());
    }

    #[test]
    fn does_not_flag_a_function_with_no_name_or_no_body() {
        assert!(violations("(defn)").is_empty());
        assert!(violations("(defn f)").is_empty());
    }

    // --- reader-syntax negatives ---------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(defn f [] (def x 1))").is_empty());
        assert!(violations("(quote (defn f [] (def x 1)))").is_empty());
    }

    /// The shape that makes the two-counter quote model load-bearing: the
    /// `def` is quoted data even though it carries no prefix of its own, and a
    /// single depth counter would call the comma an escape back to code.
    #[test]
    fn a_comma_is_whitespace_in_clojure_so_the_form_stays_data() {
        assert!(violations("'(a ,(defn f [] (def x 1)))").is_empty());
        assert!(violations("`(a ,(defn f [] (def x 1)))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(violations("`(defn f [] (def x 1))").is_empty());
    }

    /// Inside an anchor that *is* code, a quoted inner `def` is still data.
    #[test]
    fn a_quoted_def_inside_a_real_function_body_is_data() {
        assert!(violations("(defn f [] '(def x 1))").is_empty());
        assert!(violations("(defn f [] `(def x 1))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(names("`(do ~(defn f [] (def x 1)))"), vec!["x".to_owned()]);
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations("(println \"(defn f [] (def x 1))\")").is_empty());
    }

    // --- envelope ------------------------------------------------------------

    #[test]
    fn the_summary_counts_every_function_definition_scanned() {
        let report = report("(defn a [] 1)\n(defn b [] (def x 1))\n(defmethod m :k [] 2)\n");
        assert_eq!(
            report.summary,
            vec![("function_definition_count", json!(3))]
        );
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_definer_name_and_enclosing() {
        let report = report("(defn load! [p]\n  (def config (read-edn p))\n  config)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "def-inside-function-body");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("definer", json!("def")),
                ("name", json!("config")),
                ("enclosing", json!("load!")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "definer=def".to_owned(),
                "name=config".to_owned(),
                "enclosing=load!".to_owned(),
            ]
        );
        assert_eq!(
            finding.message(),
            "def interns the namespace Var config every time load! is called; \
             use let, or a top-level definition"
        );
    }

    #[test]
    fn a_computed_definition_name_is_reported_without_being_guessed_at() {
        assert_eq!(names("(defn f [] (def (sym) 1))"), vec!["?".to_owned()]);
    }

    /// Clojure metadata is a *sibling* of the name, not a prefix on it, so a
    /// rule reading child 1 unconditionally reports `^:private` as the name.
    /// Both positions matter: the enclosing function's and the inner def's.
    #[test]
    fn metadata_is_skipped_on_both_the_function_and_the_definition() {
        let items = violations("(defn ^:private load! [p] (def ^:dynamic config (read p)))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].enclosing, "load!");
        assert_eq!(items[0].name, "config");

        let items = violations("(defn ^{:doc \"d\"} f [] (def x 1))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].enclosing, "f");
    }

    /// `defn-` and a metadata-tagged `defn` are the same anchor, and both are
    /// on almost every real namespace.
    #[test]
    fn a_private_definer_is_still_an_anchor() {
        assert_eq!(names("(defn- helper [] (def x 1))"), vec!["x".to_owned()]);
    }

    /// A Clojure-only rule must say "nothing was looked for" rather than
    /// "nothing was found" when handed another dialect.
    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defun f () (defvar *config* 1))", Dialect::CommonLisp)
                .expect("parse");
        let report =
            build_def_inside_function_body_report(Path::new("a.lisp"), Dialect::CommonLisp, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![("function_definition_count", json!(0))]
        );
    }

    #[test]
    fn a_clojure_file_is_reported_as_modelled() {
        assert!(report("(defn f [] 1)").dialect_modelled);
    }

    /// `may_be_definition_head` is a **pre-filter**, and its licence to exist
    /// is that it is exact: for every head, three bytes plus `symbol_in` must
    /// agree with `symbol_in` alone. If it ever rejected something `symbol_in`
    /// would accept, it would be a silent false negative rather than an
    /// optimization.
    ///
    /// Mutation testing cannot prove this — making the pre-filter always-true
    /// leaves the suite green, which is exactly what an exact filter should do
    /// — so the claim needs its own assertion.
    #[test]
    fn the_three_byte_pre_filter_agrees_with_the_membership_test_on_every_head() {
        for head in [
            "def",
            "defn",
            "defn-",
            "defonce",
            "defmacro",
            "defmulti",
            "defrecord",
            "deftype",
            "defprotocol",
            "defstruct",
            "definline",
            "deftest",
            "default",
            "defer",
            "de",
            "d",
            "",
            "let",
            "fn",
            "DEF",
            "Defn",
            "clojure.core/defn",
            "undef",
            "redefine",
        ] {
            let exact = symbol_in(head, NAMESPACE_DEFINITION_HEADS);
            let filtered =
                may_be_definition_head(head) && symbol_in(head, NAMESPACE_DEFINITION_HEADS);
            assert_eq!(
                filtered, exact,
                "the pre-filter changed the answer for {head:?}"
            );
        }
    }
}
