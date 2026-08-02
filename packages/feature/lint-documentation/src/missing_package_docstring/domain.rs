//! A `defpackage` that says nothing about what the package is *for*.
//!
//! A package is the unit a reader meets first — it is what `:use` names, what
//! `apropos` groups by, and what an API index is organised around — and it is
//! the one definition in a Common Lisp file that nothing else in this suite
//! asks about. `missing-docstring` does not list `defpackage` among its heads,
//! and `inspect docstrings` excludes it explicitly (`carries_docstring` in
//! `paredit-feature-code-metrics`'s `docstring_report` names ten categories and
//! `Package` is not one of them, on the grounds that a `defpackage` has no
//! docstring *position*). That is true of the body — and false of the form,
//! which has taken a `(:documentation "…")` option since CLtL2.
//!
//! This is package-level coverage, and it is a different question from
//! per-definition coverage: a file can document all forty of its functions and
//! still never say what the package they live in is for.
//!
//! # Two data sources, on purpose
//!
//! Documentation is looked for in both places a project might reasonably put
//! it:
//!
//! - the `(:documentation "…")` option, which is a **node** — a list among the
//!   `defpackage` form's children; and
//! - a prose **comment** before the declaration, which is not a node at all and
//!   is read from [`SyntaxTree::comments`].
//!
//! The second is what keeps this rule off the enormous number of real
//! `package.lisp` files that open with a `;;;; The public interface of …`
//! header instead of an option. Preferring a false negative there is the whole
//! point: a project that documents its package in a comment has documented its
//! package.
//!
//! # Limits, deliberately
//!
//! - **Common Lisp only.** Clojure's `ns` takes a docstring too, in two
//!   spellings — a string in third position and a `^{:doc "…"}` metadata map —
//!   and reading the metadata one wrongly would report a documented namespace.
//!   Until that shape is pinned down, `ns` is not read at all.
//! - **Any prose comment before the form counts**, not only one immediately
//!   above it. A file-header comment describes the file, and a one-package file
//!   is its package.
//! - **An empty `(:documentation "")` is not documentation** and is reported as
//!   absent, so the rule cannot be satisfied with an empty string.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head, symbol_is};

use crate::support::{comment_prose, documentation_option};

/// The two forms that declare a Common Lisp package: `defpackage` and ASDF's
/// `uiop:define-package`, which is a superset of it.
pub const PACKAGE_DECLARATION_HEADS: [&str; 2] = ["defpackage", "define-package"];

/// How many alphabetic characters a comment needs before it counts as prose.
///
/// Low on purpose. This is the guard against a `;;;;` rule-off divider or a
/// `;; -*-` fragment being read as documentation, not a judgement about
/// whether the prose is any good.
const PROSE_MINIMUM_LETTERS: usize = 3;

/// One package declaration with nothing that says what it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndocumentedPackage {
    pub span: ByteSpan,
    /// The package's name designator, as written.
    pub name: String,
}

impl UndocumentedPackage {
    /// The sentence the rule reports.
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "package {} says nothing about what it is for: add a (:documentation \"…\") option, \
             or a comment above the declaration",
            self.name
        )
    }
}

/// Whether `view` declares a package.
#[must_use]
pub fn is_package_declaration(view: &ExpressionView) -> bool {
    is_paren_list(view)
        && list_head(view).is_some_and(|head| {
            PACKAGE_DECLARATION_HEADS
                .iter()
                .any(|expected| symbol_is(head, expected))
        })
}

/// Examines one package declaration and reports it if nothing documents it.
///
/// The guards are ordered so the cheapest settles the common case first: the
/// head test, then the `(:documentation …)` option among the form's own
/// children, and only then — for a declaration that really has no option — the
/// comment scan. A documented package never reads a comment at all.
#[must_use]
pub fn examine(tree: &SyntaxTree, view: &ExpressionView) -> Option<UndocumentedPackage> {
    if !is_package_declaration(view) {
        return None;
    }
    let name = atom_text(view.children.get(1)?)?.to_owned();
    if name.is_empty() {
        return None;
    }
    if documentation_option(view).is_some() {
        return None;
    }
    if documented_by_a_comment(tree, view.span) {
        return None;
    }
    Some(UndocumentedPackage {
        span: view.span,
        name,
    })
}

/// Whether a prose comment appears before the end of the declaration.
///
/// Comments come back in source order, so the scan stops at the first one that
/// starts past the declaration rather than reading the whole file's worth: a
/// package declaration is near the top of its file, so this reads a handful of
/// comments however many the file has below it. It also stops at the first
/// qualifying comment, so the common case — a documented `package.lisp` whose
/// header is its first comment — is one comparison.
fn documented_by_a_comment(tree: &SyntaxTree, declaration: ByteSpan) -> bool {
    tree.comments()
        .take_while(|comment| comment.span().start().get() < declaration.end().get())
        .filter_map(comment_prose)
        .any(is_prose)
}

/// Whether a comment's body is prose rather than punctuation.
///
/// A `;;;; ----------------` divider and a bare `;;;` are not documentation. A
/// `-*- Mode: Lisp -*-` header is — deliberately, because a project that writes
/// one has said something about the file, and the cost of being wrong in this
/// direction is a missed finding rather than a complaint about a documented
/// package.
fn is_prose(body: &str) -> bool {
    body.chars().filter(|c| c.is_alphabetic()).count() >= PROSE_MINIMUM_LETTERS
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn found(source: &str) -> Option<UndocumentedPackage> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let form = root
            .children
            .iter()
            .find(|child| is_package_declaration(child))?;
        examine(&tree, form)
    }

    // --- positive

    #[test]
    fn flags_a_defpackage_with_no_documentation_at_all() {
        let item = found("(defpackage :app (:use :cl) (:export #:run))").expect("a finding");
        assert_eq!(item.name, ":app");
    }

    #[test]
    fn flags_the_uiop_spelling_too() {
        assert!(found("(uiop:define-package :app (:use :cl))").is_some());
    }

    #[test]
    fn flags_a_declaration_whose_only_neighbour_is_a_divider_comment() {
        assert!(found(";;;; ----------------\n(defpackage :app (:use :cl))").is_some());
        assert!(found(";;;\n(defpackage :app (:use :cl))").is_some());
    }

    #[test]
    fn flags_a_declaration_documented_only_by_a_comment_that_comes_after_it() {
        // A comment below the declaration describes what follows it, not the
        // package. The `take_while` is what makes this a finding.
        assert!(found("(defpackage :app (:use :cl))\n;; Everything below is the API.\n").is_some());
    }

    #[test]
    fn the_span_covers_the_whole_declaration() {
        let source = "(defpackage :app (:use :cl))";
        let item = found(source).expect("a finding");
        assert_eq!(item.span.start().get(), 0);
        assert_eq!(item.span.end().get(), source.len());
    }

    // --- near-miss negatives

    #[test]
    fn a_documentation_option_is_documentation() {
        assert!(
            found("(defpackage :app (:use :cl) (:documentation \"The public interface.\"))")
                .is_none()
        );
    }

    #[test]
    fn a_comment_above_the_declaration_is_documentation() {
        assert!(
            found(";;;; The public interface of the app.\n(defpackage :app (:use :cl))").is_none()
        );
        assert!(found(";; Everything a caller needs.\n(defpackage :app (:use :cl))").is_none());
    }

    /// The trap: a file that documents the package elsewhere. A header comment
    /// several forms up still counts.
    #[test]
    fn a_file_header_comment_several_forms_up_is_documentation() {
        assert!(
            found(
                ";;;; app.lisp — the application's public interface.\n\n\
                 (in-package :cl-user)\n\n\
                 (defpackage :app (:use :cl))\n"
            )
            .is_none()
        );
    }

    #[test]
    fn a_comment_inside_the_declaration_is_documentation() {
        assert!(
            found("(defpackage :app\n  ;; Everything a caller needs.\n  (:use :cl))").is_none()
        );
    }

    /// The empty-string loophole a rule like this must not open.
    #[test]
    fn an_empty_documentation_option_does_not_satisfy_the_rule() {
        assert!(found("(defpackage :app (:use :cl) (:documentation \"\"))").is_some());
        assert!(found("(defpackage :app (:use :cl) (:documentation \"  \"))").is_some());
    }

    #[test]
    fn a_form_that_is_not_a_package_declaration_is_not_examined() {
        assert!(found("(in-package :app)").is_none());
        assert!(found("(defun f () 1)").is_none());
        // `mk-defsystem`-style heads merely *ending* in the spelling are not it.
        assert!(found("(my-defpackage :app)").is_none());
    }

    #[test]
    fn a_declaration_with_no_name_is_not_reported() {
        assert!(found("(defpackage)").is_none());
    }

    /// A `:documentation` inside a nested option belongs to that option, not to
    /// the package.
    #[test]
    fn a_nested_documentation_keyword_does_not_satisfy_the_rule() {
        assert!(found("(defpackage :app (:use :cl) (:export (:documentation #:run)))").is_some());
    }

    // --- the string-literal negative

    /// A string that merely contains the text is not a `(:documentation …)`
    /// option, and a `;` inside a string is not a comment.
    #[test]
    fn neither_a_string_nor_a_comment_inside_a_string_documents_a_package() {
        assert!(found("(defpackage :app (:use :cl) \"The public interface.\")").is_some());
        assert!(
            found("(defvar *note* \";; The public interface.\")\n(defpackage :app (:use :cl))")
                .is_some()
        );
    }

    #[test]
    fn the_message_names_the_package_and_both_ways_to_satisfy_the_rule() {
        let message = found("(defpackage :app (:use :cl))")
            .expect("a finding")
            .message();
        assert!(message.contains(":app"), "{message}");
        assert!(message.contains(":documentation"), "{message}");
        assert!(message.contains("comment"), "{message}");
    }
}
