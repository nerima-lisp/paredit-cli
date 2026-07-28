//! Common Lisp binds-constant detection: a `let`, `let*`, `do`, or `do*`
//! binding whose variable is a constant that cannot be bound — `nil`, `t`, or
//! a keyword (`:foo`). The standard forbids binding a constant variable, so
//! `(let ((nil 1)) …)`, `(let ((t 1)) …)`, and `(let ((:x 1)) …)` are all
//! program errors, caught at macroexpansion rather than by the reader.
//!
//! Only the three statically-known constants are flagged. A user `defconstant`
//! such as `+limit+` is equally unbindable, but which symbols are constant is
//! not visible from a single file, so those are left alone to keep the rule
//! free of false positives.
//!
//! Complements `malformed-let-binding`, which checks a
//! binding's *shape*; this rule checks the *validity of the bound variable*.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

const BINDING_HEADS: [&str; 4] = ["let", "let*", "do", "do*"];

/// Whether a bound variable name is a constant that cannot be bound: `nil`,
/// `t`, or a keyword. A keyword is a single leading colon followed by a
/// non-colon, so `foo:bar` (package-qualified) and `::x` (internal-symbol
/// syntax) are not treated as keywords.
fn is_constant_variable(name: &str) -> bool {
    if name.eq_ignore_ascii_case("t") || name.eq_ignore_ascii_case("nil") {
        return true;
    }
    name.len() > 1 && name.starts_with(':') && name.as_bytes()[1] != b':'
}

/// The bound variable of one binding: a bare symbol, or the head symbol of a
/// `(var …)` list binding.
fn binding_variable(binding: &ExpressionView) -> Option<&str> {
    atom_text(binding).or_else(|| {
        is_paren_list(binding)
            .then(|| binding.children.first().and_then(atom_text))
            .flatten()
    })
}

#[derive(Debug, Clone)]
pub struct BindsConstantItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub head: String,
    pub variable: String,
}

#[derive(Debug)]
pub struct BindsConstantSummary {
    pub binding_form_count: usize,
    pub violations: Vec<BindsConstantItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct BindsConstantPolicyOptions {
    fail_on_violation: bool,
}

impl BindsConstantPolicyOptions {
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
pub struct BindsConstantPolicy {
    pub fail_on_violation: bool,
    pub binding_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_binding_form(
    view: &ExpressionView,
    path: &Path,
    binding_form_count: &mut usize,
    violations: &mut Vec<BindsConstantItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !BINDING_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted binding form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    let Some(binding_list) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(binding_list) {
        return;
    }
    *binding_form_count += 1;

    for binding in &binding_list.children {
        let Some(variable) = binding_variable(binding) else {
            continue;
        };
        if is_constant_variable(variable) {
            violations.push(BindsConstantItem {
                path: path.to_path_buf(),
                span: binding.span,
                head: head.to_owned(),
                variable: variable.to_owned(),
            });
        }
    }
}

/// Collects every `let`/`let*`/`do`/`do*` binding of a constant variable across
/// a whole file, along with the total number of such binding forms scanned.
pub fn collect_binds_constant(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<BindsConstantItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut binding_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_binding_form(subview, path, &mut binding_form_count, &mut violations);
        });
    }
    Ok((binding_form_count, violations))
}

#[must_use]
pub const fn summarize_binds_constant(
    binding_form_count: usize,
    violations: Vec<BindsConstantItem>,
) -> BindsConstantSummary {
    BindsConstantSummary {
        binding_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_binds_constant_policy(
    options: BindsConstantPolicyOptions,
    summary: &BindsConstantSummary,
) -> BindsConstantPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    BindsConstantPolicy {
        fail_on_violation: options.fail_on_violation(),
        binding_form_count: summary.binding_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<BindsConstantItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_binds_constant(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect binds constant")
    }

    #[test]
    fn flags_a_let_binding_of_nil() {
        let (binding_form_count, items) = violations("(let ((nil 1)) nil)");
        assert_eq!(binding_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].variable, "nil");
        assert_eq!(items[0].head, "let");
    }

    #[test]
    fn flags_a_let_binding_of_t() {
        let (_, items) = violations("(let ((t 1)) t)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].variable, "t");
    }

    #[test]
    fn flags_a_let_binding_of_a_keyword() {
        let (_, items) = violations("(let ((:x 1)) :x)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].variable, ":x");
    }

    #[test]
    fn flags_a_bare_symbol_nil_binding() {
        let (_, items) = violations("(let (nil) 1)");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_a_let_star_binding_of_t() {
        let (_, items) = violations("(let* ((t 1)) t)");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_a_do_binding_of_a_keyword() {
        let (binding_form_count, items) = violations("(do ((:k 0 (1+ :k))) ((done)))");
        assert_eq!(binding_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "do");
    }

    #[test]
    fn does_not_flag_ordinary_variables() {
        let (binding_form_count, items) = violations("(let ((x 1) (y 2)) (+ x y))");
        assert_eq!(binding_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_special_variable() {
        let (_, items) = violations("(let ((*foo* 1)) *foo*)");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_package_qualified_symbol() {
        let (_, items) = violations("(let ((foo:bar 1)) foo:bar)");
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_binding_form() {
        let (binding_form_count, items) = violations("(list '(let ((nil 1)) 1))");
        assert_eq!(binding_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_binding_nested_in_a_function_body() {
        let (binding_form_count, items) = violations("(defun f () (let ((t 1)) t))");
        assert_eq!(binding_form_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(let ((nil 1)) nil)", Dialect::Clojure).expect("parse");
        let (binding_form_count, items) =
            collect_binds_constant(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect binds constant");
        assert_eq!(binding_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (binding_form_count, items) = violations("(let ((nil 1)) nil)");
        let summary = summarize_binds_constant(binding_form_count, items);

        let quiet =
            evaluate_binds_constant_policy(BindsConstantPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_binds_constant_policy(BindsConstantPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
