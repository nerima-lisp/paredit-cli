//! Synthesizing a `defgeneric` for a name whose `defmethod` forms have no
//! declaration.
//!
//! `inspect generic-dispatch` already finds this case — it is the
//! `undeclared` verdict — and already computes the one number a `defgeneric`
//! needs to be congruent with every method: the required-parameter arity,
//! counted the same way CLHS congruence is defined, up to the first lambda
//! list keyword. This reuses that shape rather than re-deriving it.
//!
//! What is *not* reused: `&optional`, `&rest`, and `&key` parameters. CLHS
//! congruence requires the generic to agree with every method on their
//! presence and count too, and a method is free to add them without another
//! method agreeing. Rather than guess, a name whose methods disagree on
//! required arity, or whose methods carry `&optional`/`&rest`/`&key` at all,
//! is reported unready with a reason instead of a form — `inspect
//! generic-dispatch` already names the disagreement, and a caller who wants
//! that shape can write the lambda list by hand from what it reports.

use std::collections::BTreeMap;

use paredit_core_syntax::common_lisp::{
    common_lisp_operator_head_eq, common_lisp_symbol_reference_eq,
};
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;

/// One name with `defmethod` forms but no `defgeneric`, ready to synthesize.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericCandidate {
    /// As written by the first method, so the generated form uses the
    /// caller's own casing rather than an internal folded key.
    pub name: String,
    pub required_arity: usize,
    pub method_count: usize,
    /// Byte offset of the first `defmethod` for this name. The generated
    /// `defgeneric` is inserted immediately before it.
    pub insertion_offset: usize,
    pub generated: String,
}

/// A name found with methods but excluded from generation, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadyGeneric {
    pub name: String,
    pub method_count: usize,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Candidate {
    Ready(GenericCandidate),
    Unready(UnreadyGeneric),
}

impl Candidate {
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Ready(candidate) => &candidate.name,
            Self::Unready(candidate) => &candidate.name,
        }
    }
}

struct RawMethod {
    original_name: String,
    required_arity: usize,
    has_extra_lambda_list_keywords: bool,
    span_start: usize,
}

/// Finds every name with `defmethod` forms and no `defgeneric`, in source
/// order of each name's first method.
///
/// Common Lisp only: the caller is expected to have refused every other
/// dialect before parsing, the same way a semantic refactor does.
#[must_use]
pub fn find_undeclared_generics(tree: &SyntaxTree) -> Vec<Candidate> {
    let mut declared: Vec<String> = Vec::new();
    let mut methods: BTreeMap<String, Vec<RawMethod>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();

    for form in &tree.root_view().children {
        let Some(head) = list_head(form) else {
            continue;
        };
        if common_lisp_operator_head_eq(head, "defgeneric") {
            if let Some(name) = form.children.get(1).and_then(atom_symbol_text) {
                declared.push(fold(name));
            }
        } else if common_lisp_operator_head_eq(head, "defmethod") {
            if let Some(method) = read_method(form) {
                let key = fold(&method.original_name);
                if !methods.contains_key(&key) {
                    order.push(key.clone());
                }
                methods.entry(key).or_default().push(method);
            }
        }
    }

    order
        .into_iter()
        .filter(|key| !declared.contains(key))
        .filter_map(|key| {
            let group = methods.get(&key)?;
            let first = group.first()?;
            let name = first.original_name.clone();
            let method_count = group.len();

            if group
                .iter()
                .any(|method| method.has_extra_lambda_list_keywords)
            {
                return Some(Candidate::Unready(UnreadyGeneric {
                    name,
                    method_count,
                    reason: "a method has &optional, &rest, or &key parameters, which \
                             congruence requires the defgeneric to match exactly",
                }));
            }
            let arity = first.required_arity;
            if group.iter().any(|method| method.required_arity != arity) {
                return Some(Candidate::Unready(UnreadyGeneric {
                    name,
                    method_count,
                    reason: "methods disagree on required-parameter count",
                }));
            }

            let insertion_offset = group
                .iter()
                .map(|method| method.span_start)
                .min()
                .unwrap_or(first.span_start);
            let params = (1..=arity)
                .map(|index| format!("arg{index}"))
                .collect::<Vec<_>>()
                .join(" ");
            let generated = format!("(defgeneric {name} ({params}))\n\n");

            Some(Candidate::Ready(GenericCandidate {
                name,
                required_arity: arity,
                method_count,
                insertion_offset,
                generated,
            }))
        })
        .collect()
}

/// Finds the candidate for one name, matching the way Common Lisp compares
/// symbol references: case-insensitively and ignoring a package qualifier.
#[must_use]
pub fn find_by_name(tree: &SyntaxTree, name: &str) -> Option<Candidate> {
    find_undeclared_generics(tree)
        .into_iter()
        .find(|candidate| common_lisp_symbol_reference_eq(candidate.name(), name))
}

fn read_method(form: &ExpressionView) -> Option<RawMethod> {
    let original_name = atom_symbol_text(form.children.get(1)?)?.to_owned();
    let lambda_index = form
        .children
        .iter()
        .enumerate()
        .skip(2)
        .find(|(_, child)| child.kind == ExpressionKind::List)
        .map(|(index, _)| index)?;
    let lambda_list = &form.children[lambda_index];

    let required_arity = lambda_list
        .children
        .iter()
        .take_while(|parameter| {
            atom_symbol_text(parameter).is_none_or(|text| !text.starts_with('&'))
        })
        .count();
    let has_extra_lambda_list_keywords = lambda_list.children.len() != required_arity;

    Some(RawMethod {
        original_name,
        required_arity,
        has_extra_lambda_list_keywords,
        span_start: form.span.start().get(),
    })
}

fn fold(name: &str) -> String {
    name.to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn candidates(source: &str) -> Vec<Candidate> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        find_undeclared_generics(&tree)
    }

    #[test]
    fn a_method_with_no_defgeneric_is_ready() {
        let found = candidates("(defmethod speak ((x fish)) 1)");
        assert_eq!(found.len(), 1);
        match &found[0] {
            Candidate::Ready(candidate) => {
                assert_eq!(candidate.name, "speak");
                assert_eq!(candidate.required_arity, 1);
                assert_eq!(candidate.generated, "(defgeneric speak (arg1))\n\n");
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn a_declared_generic_is_not_a_candidate() {
        let found = candidates("(defgeneric speak (x))\n(defmethod speak ((x fish)) 1)");
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn methods_that_disagree_on_arity_are_unready() {
        let found = candidates("(defmethod speak ((x fish)) 1)\n(defmethod speak ((x bird) y) 2)");
        assert_eq!(found.len(), 1);
        match &found[0] {
            Candidate::Unready(candidate) => {
                assert_eq!(candidate.name, "speak");
                assert!(candidate.reason.contains("disagree"));
            }
            other => panic!("expected Unready, got {other:?}"),
        }
    }

    #[test]
    fn a_method_with_optional_parameters_is_unready() {
        let found = candidates("(defmethod speak ((x fish) &optional y) 1)");
        assert_eq!(found.len(), 1);
        assert!(matches!(found[0], Candidate::Unready(_)));
    }

    #[test]
    fn two_methods_on_different_classes_agree_on_one_generated_form() {
        let found = candidates("(defmethod speak ((x fish)) 1)\n(defmethod speak ((x bird)) 2)");
        assert_eq!(found.len(), 1);
        match &found[0] {
            Candidate::Ready(candidate) => assert_eq!(candidate.method_count, 2),
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn the_generated_form_inserts_before_the_earliest_method() {
        let source = "(defmethod speak ((x fish)) 1)\n(other-code)\n(defmethod speak ((x bird)) 2)";
        let found = candidates(source);
        match &found[0] {
            Candidate::Ready(candidate) => {
                assert_eq!(candidate.insertion_offset, 0);
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn find_by_name_ignores_case_and_package_qualifiers() {
        let tree = SyntaxTree::parse_with_dialect(
            "(defmethod pkg:speak ((x fish)) 1)",
            Dialect::CommonLisp,
        )
        .expect("parse");
        let found = find_by_name(&tree, "SPEAK").expect("found by relaxed match");
        assert_eq!(found.name(), "pkg:speak");
    }

    #[test]
    fn zero_arity_generates_an_empty_lambda_list() {
        let found = candidates("(defmethod current-time () 1)");
        match &found[0] {
            Candidate::Ready(candidate) => {
                assert_eq!(candidate.generated, "(defgeneric current-time ())\n\n");
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }
}
