//! `maphash-mutates-other-entry` detection: a `maphash` body that adds or
//! removes a hash-table entry **other than the one being processed**.
//!
//! # What the standard actually permits
//!
//! The naive form of this rule — "flag any `remhash` or `(setf (gethash …))`
//! inside a `maphash` body" — fires on code the standard explicitly blesses.
//! CLHS 18.2 (`maphash`) says, verbatim:
//!
//! > the *function* can use `setf` of `gethash` to change the *value* part of
//! > the entry currently being processed, or it can use `remhash` to remove
//! > that entry.
//!
//! and separately:
//!
//! > The consequences are unspecified if any attempt is made to add or remove
//! > an entry from the *hash-table* while a `maphash` is in progress
//!
//! So there are two permitted operations and everything else is unspecified.
//! This rule reports the difference and nothing more: a `remhash` or a `setf`
//! of `gethash` whose key is **not** the key parameter the `maphash` lambda was
//! handed, plus `clrhash`, which removes every entry including ones not yet
//! visited.
//!
//! That distinction is what makes the rule safe to run. Deleting entries as you
//! walk them is a completely ordinary idiom:
//!
//! ```lisp
//! (maphash (lambda (k v) (when (expired-p v) (remhash k table))) table)
//! ```
//!
//! and it is *correct* — the key is the current key. A rule that flagged it
//! would be wrong on the most common shape it sees.
//!
//! # Why SBCL not complaining is not a defence
//!
//! SBCL tolerates all of it today. Running the unspecified cases:
//!
//! ```text
//! === P4c: adding a NEW key inside maphash (undefined per CLHS) ===
//! no error; final count=6
//! === P4d: remhash of a DIFFERENT key inside maphash ===
//! no error; count=5
//! ```
//!
//! No error, no warning. That is exactly why this is worth a lint rather than
//! being left to the implementation: the behaviour is unspecified, one
//! implementation's tolerance is not portable, and a table that rehashes
//! mid-walk can skip or revisit entries with nothing to say so.
//!
//! # Deliberate limits, all in the direction of not reporting
//!
//! - **The function must be a literal `lambda`.** `(maphash #'purge table)`
//!   hides its body in another definition; this rule does not chase it, so it
//!   reports nothing.
//! - **The lambda must have a readable key parameter.** Without one there is
//!   nothing to compare a key against, so nothing is reported.
//! - **The mutated table must be spelled the same as the mapped table.**
//!   Mutating a *different* hash table inside the walk is fine and common —
//!   building an index while iterating, say — and is not reported.
//! - **The key must be a name, not an expression.** `(remhash (parent-of k) h)`
//!   is a different key in general, but it is also the shape where an alias is
//!   most likely, so it is left alone.
//!
//! Scope: Common Lisp only. Clojure, Scheme, Fennel and the rest have no
//! `maphash`.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{self, key};

/// Which unspecified operation was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutation {
    /// `(remhash other-key table)` — removes an entry that is not this one.
    RemovesOtherEntry,
    /// `(setf (gethash other-key table) …)` — adds or replaces another entry.
    WritesOtherEntry,
    /// `(clrhash table)` — removes every entry, visited or not.
    ClearsTable,
}

impl Mutation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::RemovesOtherEntry => "remhash-other-key",
            Self::WritesOtherEntry => "setf-gethash-other-key",
            Self::ClearsTable => "clrhash",
        }
    }

    const fn describe(self) -> &'static str {
        match self {
            Self::RemovesOtherEntry => "removes an entry other than the one being processed",
            Self::WritesOtherEntry => {
                "adds or replaces an entry other than the one being processed"
            }
            Self::ClearsTable => "removes every entry, including ones not yet visited",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MaphashMutatesOtherEntryItem {
    /// The span of the mutating call, not of the whole `maphash`.
    pub span: ByteSpan,
    pub mutation: Mutation,
    /// The hash table both the `maphash` and the mutation name.
    pub table: String,
    /// The key parameter the lambda was handed — the one key it *is* allowed
    /// to remove or rewrite.
    pub key_parameter: String,
}

impl Finding for MaphashMutatesOtherEntryItem {
    fn kind(&self) -> &'static str {
        "maphash-mutates-other-entry"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("mutation={}", self.mutation.as_str()),
            format!("table={}", self.table),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("mutation", json!(self.mutation.as_str())),
            ("table", json!(self.table)),
            ("key_parameter", json!(self.key_parameter)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "this call {} of {} while a maphash over it is in progress: per CLHS 18.2 the only \
             modifications a maphash function may make are `setf` of `gethash` on the value of \
             the current entry and `remhash` of that same entry, keyed by {}; anything else has \
             unspecified consequences, so collect the keys first and act on them after the walk",
            self.mutation.describe(),
            self.table,
            self.key_parameter
        )
    }
}

/// The `(lambda (key value) …)` a `maphash` was handed, past a `#'`.
///
/// `#'` is a reader prefix on the same node, not a wrapper, so the only thing
/// to look through is the spelled-out `(function (lambda …))`.
fn lambda_form(view: &ExpressionView) -> Option<&ExpressionView> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    if symbol_is(head, "lambda") {
        return Some(view);
    }
    if symbol_is(head, "function") {
        let inner = view.children.get(1)?;
        return list_head(inner)
            .is_some_and(|head| symbol_is(head, "lambda"))
            .then_some(inner);
    }
    None
}

/// The key parameter of a `maphash` lambda: the first name in its lambda list.
///
/// `&`-markers are refused rather than skipped. A `maphash` function must take
/// exactly two required arguments, so a lambda list starting with a marker is
/// not one this rule understands, and guessing would compare against the wrong
/// name.
fn key_parameter(lambda: &ExpressionView) -> Option<&str> {
    let list = lambda.children.get(1)?;
    if !is_paren_list(list) {
        return None;
    }
    let first = atom_text(list.children.first()?)?;
    (!first.starts_with('&')).then_some(first)
}

/// Whether `view` names the symbol `name`, case- and package-folded.
fn names(view: &ExpressionView, name: &str) -> bool {
    atom_text(view).is_some_and(|text| key(text) == key(name))
}

/// Reads one call inside the lambda body, given the mapped table and the key
/// parameter it is allowed to touch.
fn mutation_at(
    view: &ExpressionView,
    table: &str,
    key_param: &str,
) -> Option<(Mutation, ByteSpan)> {
    let head = list_head(view)?;

    // (clrhash table)
    if symbol_is(head, "clrhash") {
        let target = view.children.get(1)?;
        return names(target, table).then_some((Mutation::ClearsTable, view.span));
    }

    // (remhash key table)
    if symbol_is(head, "remhash") {
        let mutated_key = view.children.get(1)?;
        let target = view.children.get(2)?;
        if !names(target, table) {
            return None;
        }
        // CLHS 18.2 permits removing *this* entry.
        if names(mutated_key, key_param) {
            return None;
        }
        // A computed key is not obviously a different entry; not reported.
        atom_text(mutated_key)?;
        return Some((Mutation::RemovesOtherEntry, view.span));
    }

    // (setf (gethash key table) value …)
    if symbol_is(head, "setf") || symbol_is(head, "psetf") {
        let mut index = 1;
        while index + 1 < view.children.len() {
            let place = &view.children[index];
            index += 2;
            if !list_head(place).is_some_and(|head| symbol_is(head, "gethash")) {
                continue;
            }
            let Some(mutated_key) = place.children.get(1) else {
                continue;
            };
            let Some(target) = place.children.get(2) else {
                continue;
            };
            if !names(target, table) {
                continue;
            }
            // CLHS 18.2 permits rewriting the value of *this* entry.
            if names(mutated_key, key_param) {
                continue;
            }
            if atom_text(mutated_key).is_none() {
                continue;
            }
            return Some((Mutation::WritesOtherEntry, place.span));
        }
    }

    None
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// The cheap checks come first and are all local to `view`: head, arity, a
/// literal `lambda`, a readable key parameter, a named table. Only once all of
/// those hold does anything walk a subtree, and nothing here ever touches the
/// tree root.
pub fn examine_maphash_mutates_other_entry(
    tree: &SyntaxTree,
    view: &ExpressionView,
    maphash_form_count: &mut usize,
    violations: &mut Vec<MaphashMutatesOtherEntryItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "maphash")) {
        return;
    }
    *maphash_form_count += 1;

    let Some(function) = view.children.get(1) else {
        return;
    };
    let Some(table_view) = view.children.get(2) else {
        return;
    };
    let Some(table) = atom_text(table_view) else {
        return;
    };
    let Some(lambda) = lambda_form(function) else {
        return;
    };
    let Some(key_param) = key_parameter(lambda) else {
        return;
    };

    // A nested `maphash` over the same table re-establishes which entry is
    // current, so the inner walk's own key parameter governs its body. Stopping
    // at one keeps this rule from reporting the inner lambda's legal `remhash`
    // against the outer lambda's key name.
    let mut found = Vec::new();
    let mut stack: Vec<&ExpressionView> = lambda.children.iter().collect();
    while let Some(node) = stack.pop() {
        if !is_paren_list(node) {
            continue;
        }
        if list_head(node).is_some_and(|head| symbol_is(head, "maphash")) {
            continue;
        }
        if let Some((mutation, span)) = mutation_at(node, table, key_param) {
            found.push(MaphashMutatesOtherEntryItem {
                span,
                mutation,
                table: table.to_owned(),
                key_parameter: key_param.to_owned(),
            });
        }
        stack.extend(node.children.iter());
    }

    // The quote check descends from the root, so it is paid once per *finding*
    // rather than once per matched `maphash`. A file whose every `maphash` is
    // legal never reaches it. A `(maphash …)` inside `'(…)` is a list of
    // symbols and iterates nothing.
    if found.is_empty() || support::locate(tree, view.span).is_none_or(|site| site.quoted) {
        return;
    }
    found.sort_by_key(|item| item.span.start().get());
    violations.extend(found);
}

/// Collects every unspecified mid-walk mutation in one file, with the number of
/// `maphash` forms scanned as the denominator beside them.
pub fn build_maphash_mutates_other_entry_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MaphashMutatesOtherEntryItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("maphash_form_count", json!(0))],
        ));
    }

    let mut maphash_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let Some(view) = support::top_level_view(tree, index) else {
            continue;
        };
        let mut stack = vec![&view];
        while let Some(node) = stack.pop() {
            examine_maphash_mutates_other_entry(
                tree,
                node,
                &mut maphash_form_count,
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
        vec![("maphash_form_count", json!(maphash_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MaphashMutatesOtherEntryItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_maphash_mutates_other_entry_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build report")
    }

    fn violations(input: &str) -> Vec<MaphashMutatesOtherEntryItem> {
        report(input).findings
    }

    // ---- what CLHS 18.2 explicitly permits: none of this may be reported ----

    #[test]
    fn permits_remhash_of_the_current_key() {
        let found =
            violations("(maphash (lambda (k v) (when (expired-p v) (remhash k table))) table)");
        assert!(
            found.is_empty(),
            "CLHS 18.2: the function may use remhash to remove *that* entry"
        );
    }

    #[test]
    fn permits_setf_gethash_of_the_current_key() {
        let found = violations("(maphash (lambda (k v) (setf (gethash k table) (* 2 v))) table)");
        assert!(
            found.is_empty(),
            "CLHS 18.2: the function may setf gethash to change the value of *that* entry"
        );
    }

    #[test]
    fn permits_mutating_a_different_hash_table() {
        let found = violations(
            "(maphash (lambda (k v) (setf (gethash k index) v) (remhash k index)) table)",
        );
        assert!(
            found.is_empty(),
            "building another table while walking this one is not a mid-walk mutation of this one"
        );
    }

    /// Mutation testing found the table guard on the `remhash` branch killed
    /// no test: `permits_mutating_a_different_hash_table` passes the *key*
    /// guard first, because it removes the current key. This is the case that
    /// reaches the table guard and nothing else — a different key **and** a
    /// different table, which is an ordinary two-table walk.
    #[test]
    fn permits_removing_another_key_from_another_table() {
        let found = violations(
            "(maphash (lambda (k v) (declare (ignore k v)) (remhash victim index)) table)",
        );
        assert!(
            found.is_empty(),
            "the mid-walk restriction is about the table being walked, not any table"
        );
    }

    /// The same hole on the `setf` branch.
    #[test]
    fn permits_writing_another_key_of_another_table() {
        let found = violations(
            "(maphash (lambda (k v) (declare (ignore k)) (setf (gethash sentinel index) v)) table)",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn permits_a_body_that_only_reads() {
        let found =
            violations("(maphash (lambda (k v) (format t \"~A=~A~%\" k (gethash k table))) table)");
        assert!(found.is_empty());
    }

    // ---- what CLHS 18.2 leaves unspecified: each must be reported once ----

    #[test]
    fn flags_remhash_of_a_different_key() {
        let found = violations(
            "(maphash (lambda (k v) (declare (ignore v)) (remhash (other k) table)) table)",
        );
        assert!(
            found.is_empty(),
            "a computed key is deliberately not reported"
        );

        let found = violations(
            "(maphash (lambda (k v) (declare (ignore k v)) (remhash victim table)) table)",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mutation, Mutation::RemovesOtherEntry);
        assert_eq!(found[0].table, "table");
        assert_eq!(found[0].key_parameter, "k");
    }

    #[test]
    fn flags_adding_a_new_entry() {
        let found =
            violations("(maphash (lambda (k v) (setf (gethash (mirror k) table) v)) table)");
        assert!(
            found.is_empty(),
            "a computed key is deliberately not reported"
        );

        let found = violations("(maphash (lambda (k v) (setf (gethash sentinel table) v)) table)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mutation, Mutation::WritesOtherEntry);
    }

    #[test]
    fn flags_clrhash_of_the_mapped_table() {
        let found =
            violations("(maphash (lambda (k v) (declare (ignore k v)) (clrhash table)) table)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mutation, Mutation::ClearsTable);
    }

    #[test]
    fn folds_case_and_package_qualification_when_matching_the_table() {
        let found = violations(
            "(maphash (lambda (K v) (declare (ignore v)) (remhash victim CL-USER:TABLE)) table)",
        );
        assert_eq!(found.len(), 1, "TABLE and table are the same name");
    }

    // ---- the conservative limits ----

    #[test]
    fn does_not_flag_a_named_function_whose_body_is_elsewhere() {
        let found = violations("(maphash #'purge table)");
        assert!(found.is_empty(), "nothing here shows what purge does");
    }

    #[test]
    fn reads_through_a_sharp_quoted_lambda() {
        let found =
            violations("(maphash #'(lambda (k v) (declare (ignore k v)) (clrhash table)) table)");
        assert_eq!(found.len(), 1, "#'(lambda …) is the same lambda");
    }

    #[test]
    fn reads_through_a_spelled_out_function_form() {
        let found = violations(
            "(maphash (function (lambda (k v) (declare (ignore k v)) (clrhash table))) table)",
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn does_not_flag_when_the_table_is_not_a_name() {
        let found = violations(
            "(maphash (lambda (k v) (declare (ignore k v)) (clrhash x)) (registry-of obj))",
        );
        assert!(found.is_empty(), "nothing ties `x` to the mapped table");
    }

    #[test]
    fn does_not_flag_a_lambda_list_this_rule_cannot_read() {
        let found = violations("(maphash (lambda (&rest args) (clrhash table)) table)");
        assert!(
            found.is_empty(),
            "with no key parameter there is nothing to compare a key against"
        );
    }

    /// The inner walk re-establishes which entry is current, so the inner
    /// lambda's own `remhash k2` is legal and must not be judged against `k`.
    #[test]
    fn a_nested_maphash_governs_its_own_body() {
        let found = violations(
            "(maphash (lambda (k v) (declare (ignore k v)) \
             (maphash (lambda (k2 v2) (declare (ignore v2)) (remhash k2 table)) table)) table)",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn flags_each_unspecified_mutation_separately() {
        let found = violations(
            "(maphash (lambda (k v) (declare (ignore k v)) \
             (remhash a table) (setf (gethash b table) 1)) table)",
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].mutation, Mutation::RemovesOtherEntry);
        assert_eq!(found[1].mutation, Mutation::WritesOtherEntry);
    }

    #[test]
    fn does_not_flag_a_maphash_written_inside_quoted_data() {
        let found = violations(
            "(setf template '(maphash (lambda (k v) (declare (ignore k v)) (clrhash table)) table))",
        );
        assert!(found.is_empty(), "a quoted maphash iterates nothing");
    }

    #[test]
    fn does_not_flag_a_maphash_inside_an_unescaped_quasiquote() {
        let found = violations(
            "(defmacro m () `(maphash (lambda (k v) (declare (ignore k v)) (clrhash table)) table))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn flags_a_maphash_reached_through_a_comma_in_a_quasiquote() {
        let found = violations(
            "(defmacro m () `(list ,(maphash (lambda (k v) (declare (ignore k v)) \
             (clrhash table)) table)))",
        );
        assert_eq!(found.len(), 1, "the comma escapes back to code");
    }

    #[test]
    fn the_denominator_counts_every_maphash_scanned_not_only_the_flagged_ones() {
        let scanned = report(
            "(maphash (lambda (k v) (remhash k table)) table)\n\
             (maphash (lambda (k v) (declare (ignore k v)) (clrhash other)) other)",
        );
        assert_eq!(scanned.summary, vec![("maphash_form_count", json!(2))]);
        assert_eq!(scanned.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(maphash f t)", Dialect::Clojure).expect("parse");
        let built =
            build_maphash_mutates_other_entry_report(Path::new("a.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_message_states_both_permitted_operations() {
        let built = report("(maphash (lambda (k v) (declare (ignore k v)) (clrhash table)) table)");
        let message = built.findings[0].message();
        assert!(message.contains("CLHS 18.2"), "{message}");
        assert!(message.contains("current entry"), "{message}");
        assert!(message.contains("remhash"), "{message}");
    }
}
