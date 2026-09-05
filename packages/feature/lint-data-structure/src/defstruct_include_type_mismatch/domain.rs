//! `defstruct-include-type-mismatch` detection: a `defstruct` whose `:include`
//! names a same-file structure declared with a different `:type`.
//!
//! # The premise this rule replaces
//!
//! The proposed rule was "`:type list` combined with `:include` is constrained
//! by CLHS". It is not: the combination is fine, and works.
//!
//! ```text
//! === P3: :type list with :include ===
//! (defstruct (base (:type list)) a b)
//! (defstruct (derived (:type list) (:include base)) c)
//! (make-derived :a 1 :b 2 :c 3)   => (1 2 3)
//! ```
//!
//! What CLHS constrains is the *agreement* between the two, under `defstruct`:
//!
//! > If the structure being defined has a `:type` option, then the included
//! > structure must have been declared with a `:type` option specifying the
//! > same representation *type*.
//!
//! Break the agreement and it stops working. Including a `:type list`
//! structure from an untyped one:
//!
//! ```text
//! === P3b: :include WITHOUT matching :type on the child ===
//! (defstruct (base2 (:type list)) a b)
//! (defstruct (derived2 (:include base2)) c)
//! ERROR: Class is not yet defined or was undefined: BASE2
//! ```
//!
//! The error names a *class*, which is the tell: a `:type list` structure is
//! not a class at all, so the untyped child's attempt to inherit from one has
//! nothing to inherit from. The message points at the parent's definition and
//! says nothing about `:type`, which is why the mistake is worth naming
//! directly.
//!
//! # Both directions are reported
//!
//! - untyped child, typed parent (above);
//! - typed child, untyped parent — the same disagreement, and equally refused;
//! - two different representation types, `(:type list)` including a
//!   `(:type vector)`.
//!
//! # Deliberate limits
//!
//! - **Same file only.** A parent this file does not define is not resolved,
//!   and nothing is reported. Resolving across files would need the whole
//!   system's source, which a lint pass over one file does not have.
//! - **Named representations only.** `(:type (vector double-float))` is
//!   compared as an unresolved compound type and never matches another
//!   spelling, so it is left alone rather than guessed at.
//!
//! # Scope
//!
//! Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, symbol_is};
use serde_json::{Value, json};

use crate::support::{self, DefstructForm, declared_type, defstruct_form, key};

#[derive(Debug, Clone)]
pub struct DefstructIncludeTypeMismatchItem {
    /// The span of the `(:include parent …)` option, which is the form to fix.
    pub span: ByteSpan,
    pub structure: String,
    pub included: String,
    /// The child's representation, or `"(none)"` when it declares no `:type`
    /// and so is a real structure class.
    pub structure_type: String,
    /// The parent's representation, same convention.
    pub included_type: String,
}

impl Finding for DefstructIncludeTypeMismatchItem {
    fn kind(&self) -> &'static str {
        "defstruct-include-type-mismatch"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("structure={}", self.structure),
            format!("included={}", self.included),
            format!("{} vs {}", self.structure_type, self.included_type),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("structure", json!(self.structure)),
            ("included", json!(self.included)),
            ("structure_type", json!(self.structure_type)),
            ("included_type", json!(self.included_type)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} includes {}, but their representations disagree — {} is {} and {} is {}: per CLHS \
             defstruct, a structure with a :type option may only include one declared with a \
             :type specifying the same representation, and SBCL refuses the pair outright \
             (\"Class is not yet defined or was undefined\"); give both the same :type, or \
             neither",
            self.structure,
            self.included,
            self.structure,
            self.structure_type,
            self.included,
            self.included_type
        )
    }
}

/// How a representation reads in a message: a named type, or "no :type".
fn describe(representation: Option<&String>) -> String {
    representation.map_or_else(
        || "an untyped structure class".to_owned(),
        |name| format!(":type {name}"),
    )
}

/// The `(:include parent …)` option's parent name and the option's own span.
fn included_structure<'a>(form: &'a DefstructForm<'a>) -> Option<(&'a str, ByteSpan)> {
    let option = form.option("include")?;
    let name = atom_text(option.children.get(1)?)?;
    Some((name, option.span))
}

///
/// The `:include` option is read off this form's own header, and only a
/// `defstruct` that *has* one pays for the top-level scan that looks for its
/// parent — which reads a head per top-level form and materializes only the
/// `defstruct`s.
pub fn examine_defstruct_include_type_mismatch(
    tree: &SyntaxTree,
    view: &ExpressionView,
    defstruct_form_count: &mut usize,
    violations: &mut Vec<DefstructIncludeTypeMismatchItem>,
) {
    let Some(form) = defstruct_form(view) else {
        return;
    };
    *defstruct_form_count += 1;

    // The cheap gate, local to this header: no `:include`, nothing to compare.
    let Some((included, span)) = included_structure(&form) else {
        return;
    };

    // The parent scan materializes one `defstruct` at a time and stops at the
    // first name match, rather than collecting every same-file `defstruct`
    // first. `top_level_view` builds a whole subtree, so collecting them all
    // would materialize every structure in the file for every `:include` in
    // it — quadratic in a file of structures, and paid even when the parent is
    // the first form scanned.
    let wanted = key(included);
    let Some(parent_type) = support::top_level_heads(tree)
        .filter(|top| symbol_is(top.head, "defstruct"))
        .filter_map(|top| support::top_level_view(tree, top.index))
        .find_map(|candidate| {
            let parent = defstruct_form(&candidate)?;
            (key(parent.name) == wanted).then(|| (parent.name.to_owned(), declared_type(&parent)))
        })
    else {
        // A parent this file does not define is not resolved.
        return;
    };
    let (parent_name, parent_type) = parent_type;

    let child_type = declared_type(&form);
    if child_type == parent_type {
        return;
    }
    if support::locate(tree, view.span).is_none_or(|site| site.quoted) {
        return;
    }
    violations.push(DefstructIncludeTypeMismatchItem {
        span,
        structure: form.name.to_owned(),
        included: parent_name,
        structure_type: describe(child_type.as_ref()),
        included_type: describe(parent_type.as_ref()),
    });
}

/// Collects every mismatched `:include` in one file, with the number of
/// `defstruct` forms scanned as the denominator beside them.
pub fn build_defstruct_include_type_mismatch_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DefstructIncludeTypeMismatchItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("defstruct_form_count", json!(0))],
        ));
    }

    let mut defstruct_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let Some(view) = support::top_level_view(tree, index) else {
            continue;
        };
        let mut stack = vec![&view];
        while let Some(node) = stack.pop() {
            examine_defstruct_include_type_mismatch(
                tree,
                node,
                &mut defstruct_form_count,
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
        vec![("defstruct_form_count", json!(defstruct_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DefstructIncludeTypeMismatchItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_defstruct_include_type_mismatch_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<DefstructIncludeTypeMismatchItem> {
        report(input).findings
    }

    // ---- the disagreement, in each direction ----

    #[test]
    fn flags_an_untyped_child_including_a_typed_parent() {
        let found = violations(
            "(defstruct (base (:type list)) a b)\n(defstruct (derived (:include base)) c)",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].structure, "derived");
        assert_eq!(found[0].included, "base");
        assert_eq!(found[0].structure_type, "an untyped structure class");
        assert_eq!(found[0].included_type, ":type list");
    }

    #[test]
    fn flags_a_typed_child_including_an_untyped_parent() {
        let found = violations(
            "(defstruct base a b)\n(defstruct (derived (:type list) (:include base)) c)",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].structure_type, ":type list");
        assert_eq!(found[0].included_type, "an untyped structure class");
    }

    #[test]
    fn flags_two_different_representation_types() {
        let found = violations(
            "(defstruct (base (:type vector)) a)\n(defstruct (derived (:type list) (:include base)) c)",
        );
        assert_eq!(found.len(), 1);
    }

    // ---- agreement, which must stay silent ----

    /// The refuted premise: `:type list` with `:include` is fine when both
    /// agree. Verified against SBCL 2.6.0 — see this module's header.
    #[test]
    fn does_not_flag_a_matching_type_list_pair() {
        let found = violations(
            "(defstruct (base (:type list)) a b)\n\
             (defstruct (derived (:type list) (:include base)) c)",
        );
        assert!(
            found.is_empty(),
            ":type list with :include is legal when the representations agree"
        );
    }

    #[test]
    fn does_not_flag_a_matching_named_type_list_pair() {
        let found = violations(
            "(defstruct (b3 (:type list) :named) a)\n\
             (defstruct (d3 (:type list) :named (:include b3)) c)",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_two_untyped_structures() {
        let found = violations("(defstruct base a b)\n(defstruct (derived (:include base)) c)");
        assert!(found.is_empty(), "the ordinary CLOS-backed case");
    }

    #[test]
    fn does_not_flag_a_defstruct_with_no_include() {
        assert!(violations("(defstruct (base (:type list)) a b)\n(defstruct other c)").is_empty());
    }

    #[test]
    fn does_not_flag_a_parent_this_file_does_not_define() {
        let found = violations("(defstruct (derived (:type list) (:include elsewhere)) c)");
        assert!(found.is_empty());
    }

    #[test]
    fn folds_case_and_package_qualification_when_resolving_the_parent() {
        let found = violations(
            "(defstruct (Base (:type list)) a)\n(defstruct (derived (:include CL-USER:BASE)) c)",
        );
        assert_eq!(found.len(), 1, "Base and BASE are the same structure");
    }

    #[test]
    fn does_not_flag_a_defstruct_written_inside_quoted_data() {
        let found = violations(
            "(defstruct (base (:type list)) a)\n\
             (setf form '(defstruct (derived (:include base)) c))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defstruct_inside_an_unescaped_quasiquote() {
        let found = violations(
            "(defstruct (base (:type list)) a)\n\
             (defmacro m () `(defstruct (derived (:include base)) c))",
        );
        assert!(found.is_empty());
    }

    /// A compound representation is compared as written and never matches a
    /// different spelling, so it is left alone rather than guessed at.
    #[test]
    fn does_not_flag_a_compound_representation_against_itself() {
        let found = violations(
            "(defstruct (base (:type (vector double-float))) a)\n\
             (defstruct (derived (:type (vector double-float)) (:include base)) c)",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_defstruct_scanned_not_only_the_flagged_ones() {
        let scanned = report(
            "(defstruct plain x)\n(defstruct (base (:type list)) a)\n\
             (defstruct (derived (:include base)) c)",
        );
        assert_eq!(scanned.summary, vec![("defstruct_form_count", json!(3))]);
        assert_eq!(scanned.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defstruct a b)", Dialect::Clojure).expect("parse");
        let built = build_defstruct_include_type_mismatch_report(
            Path::new("a.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_message_names_both_representations_and_cites_the_constraint() {
        let built = violations(
            "(defstruct (base (:type list)) a)\n(defstruct (derived (:include base)) c)",
        );
        let message = built[0].message();
        assert!(message.contains("CLHS defstruct"), "{message}");
        assert!(message.contains(":type list"), "{message}");
    }
}
