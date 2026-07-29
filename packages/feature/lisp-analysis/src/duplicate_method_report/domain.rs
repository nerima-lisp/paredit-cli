//! Common Lisp duplicate-method detection: two or more `defmethod` forms
//! for the same generic function name, method qualifier (`:before`,
//! `:after`, `:around`, or none for the primary method), and specializer
//! list — a literal redefinition of the exact same method, not CLOS
//! overloading (which requires a *different* specializer list to select
//! between methods at dispatch time).
//!
//! [`crate::domain::redefinition_report`] deliberately excludes
//! `Method`-category definitions entirely, since two methods sharing a
//! name with different specializers is completely normal CLOS dispatch;
//! this report narrows back in on the one shape that IS a bug — the same
//! name, qualifier, *and* specializers declared more than once, almost
//! always a copy-paste duplicate rather than an intentional second
//! definition.
//!
//! `(defmethod name [qualifier] ((param specializer)*) . body)` — a
//! parameter with no specializer (a bare symbol) implicitly specializes on
//! `t`, so `(x)` and `(x t)` describe the same method and must compare
//! equal here.
//!
//! Scope: Common Lisp only. Only literal, unqualified specializer symbols
//! are collected — `eql` specializers (`(x (eql :foo))`) and specializers
//! computed by a macro are out of scope for a syntactic comparison and are
//! treated as distinct from every other specializer, which only risks a
//! false negative (an undetected duplicate), never a false positive.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use paredit_core_cli::CliResult;

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_needle;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path as SexprPath, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_child, atom_text, list_head};

const LAMBDA_LIST_KEYWORDS: [&str; 5] = ["&optional", "&rest", "&key", "&aux", "&allow-other-keys"];

/// Reads the required-parameter specializers from a `defmethod` lambda
/// list, stopping at the first lambda-list keyword (`&optional`, `&rest`,
/// `&key`, `&aux`, `&allow-other-keys`) — only required parameters can
/// carry a specializer.
fn method_specializers(lambda_list: &ExpressionView) -> Vec<String> {
    let mut specializers = Vec::new();
    for param in &lambda_list.children {
        if let Some(keyword) = atom_text(param) {
            if LAMBDA_LIST_KEYWORDS
                .iter()
                .any(|candidate| keyword.eq_ignore_ascii_case(candidate))
            {
                break;
            }
            specializers.push("T".to_owned());
            continue;
        }
        let specializer = atom_child(param, 1).unwrap_or("T");
        specializers.push(common_lisp_symbol_reference_needle(specializer));
    }
    specializers
}

#[derive(Debug, Clone)]
pub struct DeclaredMethod {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub name: String,
    pub qualifier: Option<String>,
    pub specializers: Vec<String>,
}

impl DeclaredMethod {
    pub fn new(
        path: PathBuf,
        span: ByteSpan,
        name: impl Into<String>,
        qualifier: Option<String>,
        specializers: Vec<String>,
    ) -> Self {
        Self {
            path,
            span,
            name: name.into(),
            qualifier,
            specializers,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DuplicateMethodOccurrence {
    pub path: PathBuf,
    pub span: ByteSpan,
}

#[derive(Debug, Clone)]
pub struct DuplicateMethodItem {
    pub name: String,
    pub qualifier: Option<String>,
    pub occurrences: Vec<DuplicateMethodOccurrence>,
}

#[derive(Debug)]
pub struct DuplicateMethodSummary {
    pub declared_count: usize,
    pub duplicates: Vec<DuplicateMethodItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateMethodPolicyOptions {
    fail_on_duplicate: bool,
}

impl DuplicateMethodPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_duplicate: bool) -> Self {
        Self { fail_on_duplicate }
    }

    #[must_use]
    pub const fn fail_on_duplicate(self) -> bool {
        self.fail_on_duplicate
    }
}

#[derive(Debug)]
pub struct DuplicateMethodPolicy {
    pub fail_on_duplicate: bool,
    pub declared_count: usize,
    pub duplicate_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every `defmethod` form's (name, qualifier, specializers) in
/// one file.
pub fn collect_declared_methods(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> CliResult<Vec<DeclaredMethod>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let mut declared = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        if !head.eq_ignore_ascii_case("defmethod") {
            continue;
        }
        let Some(name) = atom_child(&view, 1) else {
            continue;
        };
        let Some(next) = view.children.get(2) else {
            continue;
        };

        let (qualifier, lambda_list) = if next.kind == ExpressionKind::List {
            (None, next)
        } else {
            let Some(qualifier) = atom_text(next) else {
                continue;
            };
            let Some(lambda_list) = view.children.get(3) else {
                continue;
            };
            if lambda_list.kind != ExpressionKind::List {
                continue;
            }
            (
                Some(common_lisp_symbol_reference_needle(qualifier)),
                lambda_list,
            )
        };

        declared.push(DeclaredMethod::new(
            path.to_path_buf(),
            view.span,
            name,
            qualifier,
            method_specializers(lambda_list),
        ));
    }
    Ok(declared)
}

/// Identity of a method for duplicate detection: its case-folded generic
/// function name, its (already-normalized) qualifier, and its normalized
/// required-parameter specializer list. Two `defmethod` forms with equal
/// identities are literal duplicates rather than dispatch-distinguishable
/// overloads.
type MethodIdentity = (String, Option<String>, Vec<String>);

#[must_use]
pub fn analyze_duplicate_methods(declared: &[DeclaredMethod]) -> DuplicateMethodSummary {
    let mut groups: BTreeMap<MethodIdentity, Vec<&DeclaredMethod>> = BTreeMap::new();
    for method in declared {
        let key = (
            common_lisp_symbol_reference_needle(&method.name),
            method.qualifier.clone(),
            method.specializers.clone(),
        );
        groups.entry(key).or_default().push(method);
    }

    let mut duplicates = Vec::new();
    for group in groups.into_values() {
        if group.len() < 2 {
            continue;
        }

        duplicates.push(DuplicateMethodItem {
            name: group[0].name.clone(),
            qualifier: group[0].qualifier.clone(),
            occurrences: group
                .iter()
                .map(|method| DuplicateMethodOccurrence {
                    path: method.path.clone(),
                    span: method.span,
                })
                .collect(),
        });
    }

    DuplicateMethodSummary {
        declared_count: declared.len(),
        duplicates,
    }
}

#[must_use]
pub fn evaluate_duplicate_method_policy(
    options: DuplicateMethodPolicyOptions,
    summary: &DuplicateMethodSummary,
) -> DuplicateMethodPolicy {
    let duplicate_count = summary.duplicates.len();
    let mut violations = Vec::new();
    if options.fail_on_duplicate() && duplicate_count > 0 {
        violations.push(format!("duplicate_count {duplicate_count} exceeds 0"));
    }

    DuplicateMethodPolicy {
        fail_on_duplicate: options.fail_on_duplicate(),
        declared_count: summary.declared_count,
        duplicate_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(path: &str, input: &str) -> Vec<DeclaredMethod> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_declared_methods(&PathBuf::from(path), Dialect::CommonLisp, &tree)
            .expect("collect declared methods")
    }

    #[test]
    fn flags_two_identical_methods_with_the_same_specializers() {
        let mut methods = declared("a.lisp", "(defmethod area ((shape circle)) 1)");
        methods.extend(declared("b.lisp", "(defmethod area ((shape circle)) 2)"));

        let summary = analyze_duplicate_methods(&methods);
        assert_eq!(summary.duplicates.len(), 1);
        assert_eq!(summary.duplicates[0].name, "area");
        assert_eq!(summary.duplicates[0].occurrences.len(), 2);
    }

    #[test]
    fn does_not_flag_methods_with_different_specializers() {
        let mut methods = declared("a.lisp", "(defmethod area ((shape circle)) 1)");
        methods.extend(declared("b.lisp", "(defmethod area ((shape square)) 2)"));

        let summary = analyze_duplicate_methods(&methods);
        assert!(summary.duplicates.is_empty());
    }

    #[test]
    fn does_not_flag_different_qualifiers_with_the_same_specializers() {
        let mut methods = declared("a.lisp", "(defmethod area ((shape circle)) 1)");
        methods.extend(declared(
            "b.lisp",
            "(defmethod area :before ((shape circle)) 2)",
        ));

        let summary = analyze_duplicate_methods(&methods);
        assert!(summary.duplicates.is_empty());
    }

    #[test]
    fn treats_an_unspecialized_parameter_as_t() {
        let mut methods = declared("a.lisp", "(defmethod f (x) 1)");
        methods.extend(declared("b.lisp", "(defmethod f ((x t)) 2)"));

        let summary = analyze_duplicate_methods(&methods);
        assert_eq!(summary.duplicates.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(defmethod area [shape] 1)").expect("parse input");
        let methods = collect_declared_methods(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
            .expect("collect declared methods");
        assert!(methods.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let mut methods = declared("a.lisp", "(defmethod area ((shape circle)) 1)");
        methods.extend(declared("b.lisp", "(defmethod area ((shape circle)) 2)"));
        let summary = analyze_duplicate_methods(&methods);

        let quiet =
            evaluate_duplicate_method_policy(DuplicateMethodPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.duplicate_count, 1);

        let strict =
            evaluate_duplicate_method_policy(DuplicateMethodPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
