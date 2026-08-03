//! `hash-table-literal-string-key-under-eql` detection: a literal string used
//! as a key on a hash table whose test is `eq` or `eql`, where every lookup
//! silently misses.
//!
//! # The failure, run rather than argued
//!
//! `eql` — the default test — compares strings by identity. Two string literals
//! with the same characters are two objects, so a store and a load of "the same
//! string" never meet. SBCL 2.6.0:
//!
//! ```text
//! === P5: default (eql) hash table with STRING keys ===
//! (let ((h (make-hash-table)))
//!   (setf (gethash "alpha" h) 1)
//!   (gethash "alpha" h))            => NIL
//!   ;; and after storing "alpha" twice:
//!   (hash-table-count h)            => 2
//! ```
//!
//! Two entries, both keyed "alpha", neither reachable. No error, no warning,
//! no compile-time complaint — the lookup just returns `nil` forever. That
//! silence is the whole case for the rule: nothing else in the toolchain says
//! anything.
//!
//! `:test #'eq` behaves the same way except that a lookup with the *identical*
//! object does hit:
//!
//! ```text
//! === P5b: :test #'eq with string keys, same object ===
//! same object: 1 / fresh copy: NIL
//! === P5c: :test #'equal with string keys ===
//! fresh copy: 1
//! ```
//!
//! `equal` is the fix, and `equalp` if case should not matter.
//!
//! # Why this is scoped to *literal* keys, and what that costs
//!
//! Hash keys are usually runtime values, and a rule that tried to decide
//! whether `(gethash name table)` had a string in `name` would be guessing at
//! types. It is not guessing when the key is spelled `"alpha"` in the source:
//! a string literal is a string, with no inference at all.
//!
//! The cost is coverage, and it is real. This finds the shape a person writes
//! while getting started — a literal-keyed lookup against a default table — and
//! not the shape where a string arrives from a parser three frames up. It is
//! deliberately the half that can be settled from the text.
//!
//! Two further restrictions, both in the direction of not reporting:
//!
//! - **The table must resolve to a `make-hash-table` in this file.** A table
//!   held in a slot, passed in as an argument, or built in another file is not
//!   chased; without seeing the `:test` there is nothing to report.
//! - **A reassigned variable is not reported.** If the binding is `setf`'d
//!   anywhere, the `make-hash-table` this rule can see may not be the table the
//!   lookup reaches.
//!
//! # Overlap with `make-hash-table-test`
//!
//! `paredit-feature-lint-form-shape`'s `make-hash-table-test` fires on an
//! explicit `:test 'eql` — the *redundant* spelling of the default — and its
//! fix deletes the argument. It says nothing about keys, and firing it makes
//! the table no less broken for string keys. The two never report the same
//! defect: that rule wants a shorter call, this one wants a different test.
//!
//! # Scope
//!
//! Common Lisp only.

use std::cell::OnceCell;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_semantics::semantics::NodeKey;
use paredit_core_semantics::semantics::binding::{BindingTable, build_binding_table};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{self, keyword_argument};

/// The accessors that take `(key table)` in that order.
pub const KEYED_ACCESSORS: [&str; 2] = ["gethash", "remhash"];

#[derive(Debug, Clone)]
pub struct HashTableLiteralStringKeyItem {
    /// The span of the literal string key, which is where the mismatch shows.
    pub span: ByteSpan,
    /// The accessor: `gethash` or `remhash`.
    pub accessor: String,
    pub table: String,
    /// The test the table was made with, as written: `eql` when the
    /// `make-hash-table` supplied none.
    pub test: String,
    /// Whether the test was left implicit.
    pub test_defaulted: bool,
}

impl Finding for HashTableLiteralStringKeyItem {
    fn kind(&self) -> &'static str {
        "hash-table-literal-string-key-under-eql"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("table={}", self.table),
            format!("test={}", self.test),
            format!("accessor={}", self.accessor),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("table", json!(self.table)),
            ("test", json!(self.test)),
            ("accessor", json!(self.accessor)),
            ("test_defaulted", json!(self.test_defaulted)),
        ]
    }

    fn message(&self) -> String {
        let test = if self.test_defaulted {
            format!(
                "{} was made with no :test, so it compares keys with eql",
                self.table
            )
        } else {
            format!("{} was made with :test {}", self.table, self.test)
        };
        format!(
            "this {} uses a literal string as a key, but {}: eq and eql compare strings by \
             identity, so a literal key never matches a separately-written literal of the same \
             characters and this lookup silently returns nil forever — pass :test #'equal (or \
             #'equalp to ignore case) to make-hash-table",
            self.accessor, test
        )
    }
}

/// Whether `view` is a string literal.
///
/// The atom text carries the delimiters, which is what distinguishes `"a"` from
/// the symbol `a`. A reader-conditional atom such as `#+sbcl "x"` keeps its
/// prefix in `atom_text`, so the `starts_with` is not fooled into treating one
/// as a bare literal.
fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"') && text.len() >= 2)
}

/// The test a `make-hash-table` call establishes, as `(written, defaulted)`.
///
/// `None` when the call supplies a `:test` this rule cannot read — a variable,
/// or a computed function — because an unreadable test is not an `eql` test.
fn table_test(make_call: &ExpressionView) -> Option<(String, bool)> {
    let Some(test) = keyword_argument(make_call, 1, "test") else {
        // CLHS make-hash-table: ":test — ... The default is eql."
        return Some(("eql".to_owned(), true));
    };
    // `#'eq`, `'eq` and `eq` all name the same function here; the reader
    // prefix stays in the atom text, so strip it before comparing.
    let text = atom_text(test)?;
    let name = text
        .trim_start_matches("#'")
        .trim_start_matches('\'')
        .trim_start_matches('`');
    Some((support::key(name), false))
}

/// The `defvar`-family heads whose second operand is the initial value.
const GLOBAL_DEFINERS: [&str; 3] = ["defparameter", "defvar", "defconstant"];

/// Reads the `make-hash-table` a binding was initialized with, if that is what
/// it was initialized with and nothing ever reassigns it.
///
/// Two lookups, because the binding table answers only the first:
///
/// 1. **Lexical bindings** — `let`, `let*`, `defun` parameters — resolve
///    through [`BindingTable::resolve`], which also carries the assignment list
///    that makes a reassigned variable unreportable.
/// 2. **Top-level `defparameter`/`defvar` globals** do *not*. The binding table
///    records such a name as *special* (see
///    `paredit-core-semantics`'s `binding::service::special_names`) but builds
///    no binding for the definition itself, so a reference to `*cache*`
///    resolves to nothing and `init_form` is unreachable. That is a gap in
///    `packages/core/semantics`, reported rather than patched here — this
///    package does not modify `core`. The fallback below covers it by name
///    within the file, which is the shape that matters: a module-level
///    `(defparameter *cache* (make-hash-table))` is where string-keyed tables
///    actually live.
fn table_construction(
    tree: &SyntaxTree,
    bindings: &BindingTable,
    reference: &ExpressionView,
) -> Option<ExpressionView> {
    if let Some(id) = NodeKey::of(reference).and_then(|node| bindings.resolve(node)) {
        let binding = bindings.binding(id);
        // A reassigned variable may hold a different table by the time this
        // runs.
        if !binding.assignments().is_empty() {
            return None;
        }
        let init = support::view_at_span(tree, binding.init_form()?)?;
        return list_head(&init)
            .is_some_and(|head| symbol_is(head, "make-hash-table"))
            .then_some(init);
    }
    global_table_construction(tree, atom_text(reference)?)
}

/// The `make-hash-table` a top-level `defparameter`/`defvar` gives `name`, if
/// nothing in the file assigns `name` afterwards.
fn global_table_construction(tree: &SyntaxTree, name: &str) -> Option<ExpressionView> {
    let wanted = support::key(name);
    let definition = support::top_level_heads(tree)
        .filter(|top| GLOBAL_DEFINERS.iter().any(|head| symbol_is(top.head, head)))
        .filter_map(|top| support::top_level_view(tree, top.index))
        .find(|view| {
            view.children
                .get(1)
                .and_then(atom_text)
                .is_some_and(|bound| support::key(bound) == wanted)
        })?;
    let init = definition.children.get(2)?;
    if !list_head(init).is_some_and(|head| symbol_is(head, "make-hash-table")) {
        return None;
    }
    // A global this file reassigns may hold a different table by the time a
    // lookup runs, exactly as a reassigned lexical may.
    if assigns_global(tree, &wanted) {
        return None;
    }
    Some(init.clone())
}

/// Whether any `(setf name …)` or `(setq name …)` in the file writes `name`.
///
/// The walk borrows. `ExpressionView`'s `Clone` is a deep, recursive copy of a
/// whole subtree, so the obvious `stack.extend(node.children.iter().cloned())`
/// clones the entire file once per call — measured at 1534735 ns/invocation
/// before this was written by reference, against 148 ns for the cheapest rule
/// in this package. Each top-level form is materialized once and walked in
/// place.
fn assigns_global(tree: &SyntaxTree, wanted: &str) -> bool {
    // Cheap byte scan first: no `(setf name …)` text means no assignment, and
    // the walk below need not run at all.
    if !support::might_assign(tree.source(), wanted) {
        return false;
    }
    (0..tree.root_children().len())
        .filter_map(|index| support::top_level_view(tree, index))
        .any(|top| {
            let mut stack: Vec<&ExpressionView> = vec![&top];
            while let Some(node) = stack.pop() {
                let writes = list_head(node)
                    .is_some_and(|head| symbol_is(head, "setf") || symbol_is(head, "setq"))
                    && node
                        .children
                        .iter()
                        .skip(1)
                        .step_by(2)
                        .filter_map(atom_text)
                        .any(|place| support::key(place) == wanted);
                if writes {
                    return true;
                }
                stack.extend(node.children.iter());
            }
            false
        })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// The gate order is the whole cost story, and `bindings` is a **closure** for
/// exactly that reason. A literal string in key position is a two-byte check on
/// this form's own second operand, and it is rare; the binding table is a
/// whole-file semantic build. Taking `&BindingTable` by value would have the
/// caller evaluate `context.binding_table()` at the call site — that is,
/// before this function runs at all — and every `gethash` in the file would
/// pay for it. Measured, that mistake cost 1667047 ns/invocation against 164
/// ns for the cheapest rule in this package.
pub fn examine_hash_table_literal_string_key<'a>(
    tree: &SyntaxTree,
    bindings: impl FnOnce() -> &'a BindingTable,
    view: &ExpressionView,
    keyed_accessor_count: &mut usize,
    violations: &mut Vec<HashTableLiteralStringKeyItem>,
) {
    if !is_paren_list(view) {
        return;
    }
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(accessor) = KEYED_ACCESSORS
        .iter()
        .find(|name| symbol_is(head, name))
        .copied()
    else {
        return;
    };
    *keyed_accessor_count += 1;

    let Some(key_view) = view.children.get(1) else {
        return;
    };
    // The cheap gate: a literal string key, read off this node alone.
    if !is_string_literal(key_view) {
        return;
    }
    let Some(table_view) = view.children.get(2) else {
        return;
    };
    let Some(table) = atom_text(table_view) else {
        return;
    };
    // Past every cheap gate: only now is the semantic table worth building.
    let Some(make_call) = table_construction(tree, bindings(), table_view) else {
        return;
    };
    let Some((test, test_defaulted)) = table_test(&make_call) else {
        return;
    };
    if test != "eq" && test != "eql" {
        return;
    }
    if support::locate(tree, view.span).is_none_or(|site| site.quoted) {
        return;
    }
    violations.push(HashTableLiteralStringKeyItem {
        span: key_view.span,
        accessor: accessor.to_owned(),
        table: table.to_owned(),
        test,
        test_defaulted,
    });
}

/// Collects every literal-string lookup against an identity-tested table in one
/// file, with the number of `gethash`/`remhash` calls scanned as the
/// denominator beside them.
pub fn build_hash_table_literal_string_key_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<HashTableLiteralStringKeyItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("keyed_accessor_count", json!(0))],
        ));
    }

    // Lazy here too, so a file with no literal-string key never pays for the
    // semantic build — the same property the lint rule has.
    let bindings: OnceCell<BindingTable> = OnceCell::new();
    let mut keyed_accessor_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let Some(view) = support::top_level_view(tree, index) else {
            continue;
        };
        let mut stack = vec![&view];
        while let Some(node) = stack.pop() {
            examine_hash_table_literal_string_key(
                tree,
                || bindings.get_or_init(|| build_binding_table(dialect, tree, tree.source())),
                node,
                &mut keyed_accessor_count,
                &mut violations,
            );
            stack.extend(node.children.iter());
        }
    }
    violations.sort_by_key(|item| item.span.start().get());

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("keyed_accessor_count", json!(keyed_accessor_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<HashTableLiteralStringKeyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_hash_table_literal_string_key_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<HashTableLiteralStringKeyItem> {
        report(input).findings
    }

    // ---- the silent always-miss ----

    #[test]
    fn flags_a_literal_key_against_a_defaulted_table() {
        let found = violations("(let ((h (make-hash-table))) (gethash \"alpha\" h))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].table, "h");
        assert_eq!(found[0].test, "eql");
        assert!(found[0].test_defaulted);
    }

    #[test]
    fn flags_a_literal_key_against_an_explicit_eq_table() {
        let found = violations("(let ((h (make-hash-table :test #'eq))) (gethash \"alpha\" h))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].test, "eq");
        assert!(!found[0].test_defaulted);
    }

    #[test]
    fn flags_an_explicit_eql_table() {
        let found = violations("(let ((h (make-hash-table :test 'eql))) (gethash \"a\" h))");
        assert_eq!(found.len(), 1);
        assert!(!found[0].test_defaulted);
    }

    #[test]
    fn flags_remhash_as_well_as_gethash() {
        let found = violations("(let ((h (make-hash-table))) (remhash \"alpha\" h))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].accessor, "remhash");
    }

    #[test]
    fn flags_the_key_of_a_setf_gethash_place() {
        let found = violations("(let ((h (make-hash-table))) (setf (gethash \"alpha\" h) 1))");
        assert_eq!(found.len(), 1, "the store misses just as the load does");
    }

    // ---- correct code, which must stay silent ----

    #[test]
    fn does_not_flag_an_equal_table() {
        assert!(
            violations("(let ((h (make-hash-table :test #'equal))) (gethash \"a\" h))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_an_equalp_table() {
        assert!(
            violations("(let ((h (make-hash-table :test #'equalp))) (gethash \"a\" h))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_symbol_key() {
        assert!(violations("(let ((h (make-hash-table))) (gethash 'alpha h))").is_empty());
        assert!(violations("(let ((h (make-hash-table))) (gethash name h))").is_empty());
    }

    #[test]
    fn does_not_flag_a_number_or_character_key() {
        assert!(violations("(let ((h (make-hash-table))) (gethash 42 h))").is_empty());
        assert!(violations("(let ((h (make-hash-table))) (gethash #\\a h))").is_empty());
    }

    #[test]
    fn does_not_flag_a_table_this_file_does_not_construct() {
        assert!(violations("(defun f (h) (gethash \"a\" h))").is_empty());
        assert!(violations("(gethash \"a\" (registry-of obj))").is_empty());
    }

    #[test]
    fn does_not_flag_a_table_whose_test_it_cannot_read() {
        let found = violations("(let ((h (make-hash-table :test chosen-test))) (gethash \"a\" h))");
        assert!(found.is_empty(), "an unreadable test is not an eql test");
    }

    /// The binding this rule can see may not be the table the lookup reaches.
    #[test]
    fn does_not_flag_a_variable_that_is_reassigned() {
        let found = violations(
            "(let ((h (make-hash-table))) (setf h (make-hash-table :test #'equal)) \
             (gethash \"a\" h))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_gethash_written_inside_quoted_data() {
        let found = violations("(let ((h (make-hash-table))) (setf form '(gethash \"a\" h)))");
        assert!(found.is_empty());
    }

    #[test]
    fn flags_a_defparameter_table_as_well_as_a_let_binding() {
        let found = violations(
            "(defparameter *cache* (make-hash-table))\n(defun f () (gethash \"a\" *cache*))",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].table, "*cache*");
    }

    #[test]
    fn does_not_flag_a_global_the_file_reassigns() {
        let found = violations(
            "(defparameter *cache* (make-hash-table))\n\
             (defun reset () (setf *cache* (make-hash-table :test #'equal)))\n\
             (defun f () (gethash \"a\" *cache*))",
        );
        assert!(
            found.is_empty(),
            "a reassigned global may hold a different table by the time the lookup runs"
        );
    }

    #[test]
    fn does_not_flag_a_global_bound_to_something_other_than_a_hash_table() {
        let found =
            violations("(defparameter *cache* (load-table))\n(defun f () (gethash \"a\" *cache*))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_correctly_tested_global() {
        let found = violations(
            "(defparameter *cache* (make-hash-table :test #'equal))\n\
             (defun f () (gethash \"a\" *cache*))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_keyed_accessor_scanned() {
        let scanned = report(
            "(let ((h (make-hash-table :test #'equal)) (g (make-hash-table))) \
             (gethash \"a\" h) (gethash 'b h) (gethash \"c\" g))",
        );
        assert_eq!(scanned.summary, vec![("keyed_accessor_count", json!(3))]);
        assert_eq!(scanned.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(gethash \"a\" h)", Dialect::Clojure).expect("parse");
        let built =
            build_hash_table_literal_string_key_report(Path::new("a.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_message_names_the_repair() {
        let built = report("(let ((h (make-hash-table))) (gethash \"a\" h))");
        let message = built.findings[0].message();
        assert!(message.contains("#'equal"), "{message}");
        assert!(message.contains("identity"), "{message}");
    }
}
