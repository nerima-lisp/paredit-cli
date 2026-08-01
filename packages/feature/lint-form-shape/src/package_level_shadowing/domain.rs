//! `package-level-shadowing` detection: an inner `let`/`let*` binding or
//! lambda-list parameter that reuses the name of a top-level
//! `defun`/`defvar`/`defparameter`/`defconstant`/`defmacro` in the same file.
//!
//! `inspect shadowed-bindings` already covers *lexical* shadowing — an inner
//! binding reusing an *enclosing* `let`/parameter name. It deliberately does
//! not reach outward past the enclosing definition to the file's top-level
//! names, because that is a different, wider question: a local named the
//! same as a top-level function or special variable makes the outer one
//! unreachable for the rest of that binding's scope, and a rename or a typo
//! is the usual cause either way.
//!
//! Judging "is this name also a top-level definition" needs every top-level
//! definition in the file at once, which is why this is a whole-document
//! rule.
//!
//! Scope, by design: only `let`/`let*` bindings and a `defun`/`defmacro`'s own
//! lambda-list parameters are read as shadowing sources — the same two
//! sources `inspect shadowed-bindings` tracks, for the same reason: extending
//! to `flet`/`labels`/inline `lambda` parameter lists would need the same
//! per-dialect local-callable-form recognition that report already declines
//! to reimplement here. Lambda-list parsing is shallow: a bare name and a
//! `(name default)`/`(name default supplied-p)` pair are read; the nested
//! destructuring lists a macro lambda list can have are not.
//!
//! Report-only: renaming is `refactor rename` territory, not a lint fix.
//!
//! Scope: Common Lisp only.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path as SexprPath, SyntaxTree,
};
use paredit_core_syntax::view_query::{
    atom_text, for_each_subview, list_head, symbol_in, unqualified,
};

#[derive(Debug, Clone)]
pub struct PackageLevelShadowingItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The shadowing name, and what kind of binding introduced it.
    pub name: String,
    pub source: &'static str,
}

#[derive(Debug)]
pub struct PackageLevelShadowingSummary {
    pub scanned_form_count: usize,
    pub violations: Vec<PackageLevelShadowingItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct PackageLevelShadowingPolicyOptions {
    fail_on_violation: bool,
}

impl PackageLevelShadowingPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct PackageLevelShadowingPolicy {
    pub fail_on_violation: bool,
    pub scanned_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Every name a top-level `defun`/`defvar`/`defparameter`/`defconstant`/
/// `defmacro` in `root` defines.
#[must_use]
pub fn top_level_names(root: &ExpressionView) -> HashSet<String> {
    let mut names = HashSet::new();
    for form in &root.children {
        let Some(head) = list_head(form) else {
            continue;
        };
        if !symbol_in(
            head,
            &["defun", "defvar", "defparameter", "defconstant", "defmacro"],
        ) {
            continue;
        }
        if let Some(name) = form.children.get(1).and_then(atom_text) {
            names.insert(unqualified(name).to_ascii_lowercase());
        }
    }
    names
}

/// One shallow lambda-list parameter name: a bare symbol, or the first
/// element of a `(name default)`/`(name default supplied-p)` pair. `&`-marker
/// entries are not names.
fn parameter_name(entry: &ExpressionView) -> Option<&str> {
    let name = match entry.kind {
        ExpressionKind::Atom => atom_text(entry)?,
        ExpressionKind::List => atom_text(entry.children.first()?)?,
        ExpressionKind::Root => return None,
    };
    if name.starts_with('&') {
        return None;
    }
    Some(name)
}

/// Every node in `view` that introduces a binding shadowing one of
/// `top_level`: a `let`/`let*` binding, or a `defun`/`defmacro`'s own
/// lambda-list parameter.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    top_level: &HashSet<String>,
    scanned_form_count: &mut usize,
    violations: &mut Vec<PackageLevelShadowingItem>,
) {
    *scanned_form_count += 1;
    let Some(head) = list_head(view) else {
        return;
    };

    if symbol_in(head, &["let", "let*"]) {
        let Some(bindings) = view.children.get(1) else {
            return;
        };
        if bindings.kind != ExpressionKind::List {
            return;
        }
        for binding in &bindings.children {
            let name = match binding.kind {
                ExpressionKind::Atom => atom_text(binding),
                ExpressionKind::List => binding.children.first().and_then(atom_text),
                ExpressionKind::Root => None,
            };
            let Some(name) = name else { continue };
            let lowered = unqualified(name).to_ascii_lowercase();
            if top_level.contains(&lowered) {
                violations.push(PackageLevelShadowingItem {
                    path: path.to_path_buf(),
                    span: binding.span,
                    name: name.to_owned(),
                    source: "let",
                });
            }
        }
        return;
    }

    if symbol_in(head, &["defun", "defmacro"]) {
        let Some(lambda_list) = view.children.get(2) else {
            return;
        };
        if lambda_list.kind != ExpressionKind::List {
            return;
        }
        for entry in &lambda_list.children {
            let Some(name) = parameter_name(entry) else {
                continue;
            };
            let lowered = unqualified(name).to_ascii_lowercase();
            if top_level.contains(&lowered) {
                violations.push(PackageLevelShadowingItem {
                    path: path.to_path_buf(),
                    span: entry.span,
                    name: name.to_owned(),
                    source: "parameter",
                });
            }
        }
    }
}

/// Collects every violation across a whole file, along with the total number of
/// forms scanned.
pub fn collect_package_level_shadowing(
    path: &Path,
    _dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<PackageLevelShadowingItem>)> {
    let root = tree.root_view();
    let top_level = top_level_names(&root);
    let mut scanned_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(
                subview,
                path,
                &top_level,
                &mut scanned_form_count,
                &mut violations,
            );
        });
    }
    Ok((scanned_form_count, violations))
}

#[must_use]
pub const fn summarize_package_level_shadowing(
    scanned_form_count: usize,
    violations: Vec<PackageLevelShadowingItem>,
) -> PackageLevelShadowingSummary {
    PackageLevelShadowingSummary {
        scanned_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_package_level_shadowing_policy(
    options: PackageLevelShadowingPolicyOptions,
    summary: &PackageLevelShadowingSummary,
) -> PackageLevelShadowingPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    PackageLevelShadowingPolicy {
        fail_on_violation: options.fail_on_violation(),
        scanned_form_count: summary.scanned_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shadows(input: &str) -> Vec<PackageLevelShadowingItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_package_level_shadowing(&PathBuf::from("t.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        violations
    }

    #[test]
    fn flags_a_parameter_shadowing_a_top_level_function() {
        let found = shadows("(defun helper () 1)\n(defun f (helper) helper)");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "helper");
        assert_eq!(found[0].source, "parameter");
    }

    #[test]
    fn flags_a_let_binding_shadowing_a_top_level_special() {
        let found = shadows("(defvar *state* 1)\n(defun f () (let ((*state* 2)) *state*))");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "*state*");
        assert_eq!(found[0].source, "let");
    }

    #[test]
    fn flags_a_let_binding_shadowing_a_top_level_constant() {
        let found = shadows("(defparameter *limit* 10)\n(let ((*limit* 5)) *limit*)");
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn flags_an_optional_parameter_default_pair_shadowing_a_top_level_name() {
        let found = shadows("(defparameter *limit* 10)\n(defun f (&optional (*limit* 5)) *limit*)");
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn does_not_flag_a_parameter_with_no_top_level_match() {
        assert!(shadows("(defun f (x) x)").is_empty());
    }

    #[test]
    fn does_not_flag_an_ordinary_call() {
        assert!(shadows("(defun helper () 1)\n(defun f () (helper))").is_empty());
    }

    #[test]
    fn does_not_flag_a_lambda_list_marker_itself() {
        assert!(
            shadows("(defparameter *x* 1)\n(defun f (&optional y &rest z) (list y z))").is_empty()
        );
    }
}
