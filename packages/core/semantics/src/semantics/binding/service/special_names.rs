//! Which names a file could possibly declare special.
//!
//! [`paredit_core_syntax::common_lisp::common_lisp_dynamic_binding_is_declared`]
//! answers the real question, but it re-scans the whole document per binding,
//! which is quadratic over a file's variable bindings. Almost every binding is
//! an ordinary lexical one whose name appears in no declaration at all, so this
//! index is scanned once and used to skip that call outright.
//!
//! The index is deliberately *loose*: it over-collects, and a hit only means
//! the real predicate has to run. Being a superset is what keeps the shortcut
//! sound — every path that answers `true` over there requires the name to be
//! written as a non-head atom of a `special`, `defvar`, or `defparameter` form,
//! which is exactly what is collected here.

use std::collections::HashSet;

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_needle;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SymbolName};

/// The heads under which a special variable's name is written, canonicalized.
const DECLARING_HEADS: [&str; 3] = ["SPECIAL", "DEFVAR", "DEFPARAMETER"];

/// The canonical names that some declaration in the file mentions.
#[derive(Debug, Default)]
pub(super) struct SpecialNameIndex {
    mentioned: HashSet<String>,
}

impl SpecialNameIndex {
    /// Collects every name a `special` specifier or a `defvar`/`defparameter`
    /// writes, anywhere in the document.
    pub(super) fn scan(document: &ExpressionView) -> Self {
        let mut index = Self::default();
        index.visit(document);
        index
    }

    /// Whether `name` is worth asking the real predicate about.
    ///
    /// A miss is a proof of "not special"; a hit proves nothing on its own.
    pub(super) fn may_be_declared_special(&self, name: &SymbolName) -> bool {
        !self.mentioned.is_empty()
            && self
                .mentioned
                .contains(&common_lisp_symbol_reference_needle(name.as_str()))
    }

    fn visit(&mut self, view: &ExpressionView) {
        if view.kind == ExpressionKind::List && self.declares_special_names(view) {
            self.mentioned.extend(
                view.children[1..]
                    .iter()
                    .filter_map(atom_text)
                    .map(common_lisp_symbol_reference_needle),
            );
        }

        for child in &view.children {
            self.visit(child);
        }
    }

    /// Whether the form's arguments can name a special variable.
    ///
    /// The head is canonicalized once and matched against all three spellings,
    /// rather than compared three times through the operator-head predicate the
    /// real check uses. That is looser — it also accepts a head qualified by
    /// some package other than `CL` — and looser is the safe direction: an
    /// extra name in the index costs one call that answers `false`.
    ///
    /// `declare`, `declaim`, and `proclaim` are absent on purpose. The name
    /// never sits directly under them, only under the `(special …)` specifier
    /// they wrap.
    fn declares_special_names(&self, view: &ExpressionView) -> bool {
        view.children
            .first()
            .and_then(atom_text)
            .is_some_and(|head| {
                DECLARING_HEADS.contains(&common_lisp_symbol_reference_needle(head).as_str())
            })
    }
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn index_of(input: &str) -> SpecialNameIndex {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        SpecialNameIndex::scan(&tree.root_view())
    }

    fn mentions(input: &str, name: &str) -> bool {
        index_of(input).may_be_declared_special(&SymbolName::new(name).expect("symbol"))
    }

    #[test]
    fn a_file_with_no_declaration_mentions_nothing() {
        assert!(!mentions("(let ((x 1)) x)", "x"));
    }

    #[test]
    fn every_spelling_the_real_predicate_accepts_is_collected() {
        assert!(mentions(
            "(declaim (special *a*)) (let ((*a* 1)) *a*)",
            "*a*"
        ));
        assert!(mentions("(proclaim '(special *b*))", "*b*"));
        assert!(mentions("(defvar *c* 1)", "*c*"));
        assert!(mentions("(defparameter *d* 1)", "*d*"));
        assert!(mentions("(defun f () (declare (special *e*)) *e*)", "*e*"));
    }

    #[test]
    fn a_declaration_nested_anywhere_is_still_seen() {
        assert!(mentions("(progn (progn (defvar *deep* 1)))", "*deep*"));
    }

    #[test]
    fn matching_folds_case_and_package_qualification() {
        assert!(mentions("(defvar cl-user::*g* 1)", "*G*"));
    }

    #[test]
    fn an_unrelated_name_in_the_same_file_is_not_a_hit() {
        assert!(!mentions("(defvar *taken* 1) (let ((x 1)) x)", "x"));
    }
}
