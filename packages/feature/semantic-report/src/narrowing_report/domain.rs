//! Where a branch proves something about a binding that is not true outside it.
//!
//! Flow narrowing is the part of the type layer that has no output of its own.
//! It runs inside inference, refines a binding along one branch, and is
//! discarded when the branch closes — the type table keeps only the *result*,
//! so nothing records that `(when (stringp x) …)` is the reason `x` is a
//! string in there.
//!
//! That reason is what a refactor needs. Hoisting a form out of the `then`
//! branch, or merging two branches, is safe exactly when it does not escape a
//! narrowing that justified it, and no existing report says where those
//! boundaries are.
//!
//! This module re-asks the question the narrowing service asks, against the
//! same policy table ([`narrowed_by_predicate`]), and reports the sites rather
//! than folding them into a type. It is deliberately syntactic about *which*
//! forms narrow — `if`, `when`, `unless`, `cond`, `typecase`, `etypecase` — for
//! the same reason the service is: an unregistered head may be a macro, and a
//! macro can do anything to the branch structure beneath it.

use std::path::PathBuf;

use paredit_core_semantics::semantics::NodeKey;
use paredit_core_semantics::semantics::binding::BindingTable;
use paredit_core_semantics::semantics::typing::Ty;
use paredit_core_semantics::semantics::typing::policy::narrowed_by_predicate;
use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView};
use paredit_core_syntax::view_query::{is_paren_list, list_head};

use crate::shared::{SemanticFile, line_of, snippet};

const SNIPPET_LIMIT: usize = 56;

/// Which branch of a form a narrowing holds in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Branch {
    /// The branch taken when the test holds — `if`'s then, `when`'s body, a
    /// `cond` clause's body, a `typecase` clause's body.
    Then,
    /// The branch taken when the test fails — `if`'s else, `unless`'s body.
    Else,
}

impl Branch {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Then => "then",
            Self::Else => "else",
        }
    }
}

/// What the test was written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NarrowingSource {
    /// A type predicate call: `(stringp x)`.
    Predicate,
    /// A negated type predicate: `(not (stringp x))`, `(null x)` in test
    /// position of an `unless`, and so on. The negation flips which branch the
    /// narrowing holds in.
    NegatedPredicate,
    /// A `typecase`/`etypecase` clause's type specifier.
    TypecaseClause,
}

impl NarrowingSource {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Predicate => "predicate",
            Self::NegatedPredicate => "negated-predicate",
            Self::TypecaseClause => "typecase-clause",
        }
    }
}

/// One place where a branch knows more about a binding than its surroundings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrowingSite {
    /// The binding narrowed, by the name it was bound under.
    pub binding: String,
    /// What the branch proves it is.
    pub narrowed_to: Ty,
    pub branch: Branch,
    pub source: NarrowingSource,
    /// The head of the form that branches (`if`, `when`, `typecase`, …).
    pub form: String,
    /// The test or type specifier that does the narrowing.
    pub test: String,
    /// The whole branching form, which is the region the narrowing is scoped
    /// to. A refactor that moves code out of this span leaves the narrowing.
    pub span: ByteSpan,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrowingReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub dialect_modelled: bool,
    pub sites: Vec<NarrowingSite>,
}

#[must_use]
pub fn build_narrowing_report(file: &SemanticFile) -> NarrowingReportFile {
    let source = file.tree.source();
    let mut sites = Vec::new();
    collect(&file.tree.root_view(), &file.bindings, source, &mut sites);
    sites.sort_by_key(|site| {
        (
            site.span.start().get(),
            site.span.end().get(),
            site.branch.label(),
            site.binding.clone(),
        )
    });

    NarrowingReportFile {
        path: file.path.clone(),
        dialect: file.dialect,
        dialect_modelled: file.dialect_is_modelled(),
        sites,
    }
}

fn collect(
    view: &ExpressionView,
    bindings: &BindingTable,
    source: &str,
    sites: &mut Vec<NarrowingSite>,
) {
    if let Some(head) = list_head(view) {
        collect_from_form(view, head, bindings, source, sites);
    }
    for child in &view.children {
        collect(child, bindings, source, sites);
    }
}

fn collect_from_form(
    view: &ExpressionView,
    head: &str,
    bindings: &BindingTable,
    source: &str,
    sites: &mut Vec<NarrowingSite>,
) {
    // `if`'s test narrows both ways: the predicate's type in `then`, and — for
    // the two predicates whose complement this lattice can express — something
    // in `else`. `when`/`unless` are the one-armed cases, and `unless` is the
    // one that inverts.
    for (name, then_branch) in [
        ("if", Branch::Then),
        ("when", Branch::Then),
        ("unless", Branch::Else),
    ] {
        if common_lisp_operator_head_eq(head, name) {
            let Some(test) = view.children.get(1) else {
                return;
            };
            push_test_narrowings(view, head, test, then_branch, bindings, source, sites);
            return;
        }
    }

    if common_lisp_operator_head_eq(head, "cond") {
        for clause in view.children.iter().skip(1) {
            if let Some(test) = clause.children.first() {
                push_test_narrowings(clause, head, test, Branch::Then, bindings, source, sites);
            }
        }
        return;
    }

    for name in ["typecase", "etypecase", "ctypecase"] {
        if common_lisp_operator_head_eq(head, name) {
            push_typecase_narrowings(view, head, bindings, source, sites);
            return;
        }
    }
}

/// Reads a test form and pushes whatever it proves.
///
/// A bare `(not X)` is unwrapped once and flips the branch. Nesting further is
/// not chased: `(not (not (stringp x)))` is not idiomatic, and following it
/// would mean re-implementing the evaluator rather than reading a test.
fn push_test_narrowings(
    form: &ExpressionView,
    head: &str,
    test: &ExpressionView,
    branch: Branch,
    bindings: &BindingTable,
    source: &str,
    sites: &mut Vec<NarrowingSite>,
) {
    let (test, branch, negated) = match negation_operand(test) {
        Some(inner) => (inner, flip(branch), true),
        None => (test, branch, false),
    };

    let Some((predicate, argument)) = unary_call(test) else {
        return;
    };
    let Some(name) = bound_name(argument, bindings) else {
        return;
    };
    // `holds` is asked of the *predicate*, so a negated test asks the same
    // question about the branch it now applies to.
    let Some(ty) = narrowed_by_predicate(predicate, true) else {
        return;
    };

    sites.push(NarrowingSite {
        binding: name,
        narrowed_to: ty,
        branch,
        source: if negated {
            NarrowingSource::NegatedPredicate
        } else {
            NarrowingSource::Predicate
        },
        form: head.to_owned(),
        test: snippet(source, test.span, SNIPPET_LIMIT),
        span: form.span,
        line: line_of(source, form.span.start().get()),
    });
}

fn push_typecase_narrowings(
    view: &ExpressionView,
    head: &str,
    bindings: &BindingTable,
    source: &str,
    sites: &mut Vec<NarrowingSite>,
) {
    let Some(key) = view.children.get(1) else {
        return;
    };
    let Some(name) = bound_name(key, bindings) else {
        return;
    };

    for clause in view.children.iter().skip(2) {
        let Some(spec) = clause.children.first() else {
            continue;
        };
        let Some(ty) = atom_symbol_text(spec).and_then(Ty::from_name) else {
            continue;
        };
        sites.push(NarrowingSite {
            binding: name.clone(),
            narrowed_to: ty,
            branch: Branch::Then,
            source: NarrowingSource::TypecaseClause,
            form: head.to_owned(),
            test: snippet(source, spec.span, SNIPPET_LIMIT),
            span: clause.span,
            line: line_of(source, clause.span.start().get()),
        });
    }
}

const fn flip(branch: Branch) -> Branch {
    match branch {
        Branch::Then => Branch::Else,
        Branch::Else => Branch::Then,
    }
}

/// The operand of a `(not X)`/`(null X)` test, or `None`.
fn negation_operand(view: &ExpressionView) -> Option<&ExpressionView> {
    let head = list_head(view)?;
    (common_lisp_operator_head_eq(head, "not") && view.children.len() == 2)
        .then(|| view.children.get(1))
        .flatten()
}

/// A `(f x)` call's operator name and its single argument.
fn unary_call(view: &ExpressionView) -> Option<(&str, &ExpressionView)> {
    if !is_paren_list(view) || view.children.len() != 2 {
        return None;
    }
    Some((
        atom_symbol_text(view.children.first()?)?,
        view.children.get(1)?,
    ))
}

/// The name of the binding an atom resolves to, or `None` when it is not a
/// statically resolvable reference.
///
/// Going through the binding table rather than the spelling is what makes the
/// finding trustworthy: two `x` bindings in sibling scopes are different
/// things, and a report keyed on the name alone would attribute a narrowing to
/// the wrong one.
fn bound_name(view: &ExpressionView, bindings: &BindingTable) -> Option<String> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    let id = bindings.resolve(NodeKey::of(view)?)?;
    Some(bindings.binding(id).name().as_str().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use std::path::Path;

    fn report_of(source: &str, dialect: Dialect) -> NarrowingReportFile {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        build_narrowing_report(&SemanticFile::analyze(Path::new("t.lisp"), dialect, tree))
    }

    fn report(source: &str) -> NarrowingReportFile {
        report_of(source, Dialect::CommonLisp)
    }

    #[test]
    fn a_predicate_test_narrows_the_then_branch() {
        let report = report("(defun f (x) (when (stringp x) x))");
        assert_eq!(report.sites.len(), 1, "{report:?}");
        let site = &report.sites[0];
        assert!(site.binding.eq_ignore_ascii_case("x"));
        assert_eq!(site.narrowed_to, Ty::String);
        assert_eq!(site.branch, Branch::Then);
        assert_eq!(site.source, NarrowingSource::Predicate);
    }

    #[test]
    fn unless_narrows_its_else_branch_because_its_body_runs_when_the_test_fails() {
        let report = report("(defun f (x) (unless (stringp x) x))");
        assert_eq!(report.sites[0].branch, Branch::Else);
    }

    #[test]
    fn a_negated_test_flips_which_branch_holds() {
        let report = report("(defun f (x) (when (not (stringp x)) x))");
        assert_eq!(report.sites[0].branch, Branch::Else);
        assert_eq!(report.sites[0].source, NarrowingSource::NegatedPredicate);
    }

    #[test]
    fn a_cond_clause_narrows_its_own_body() {
        let report = report("(defun f (x) (cond ((integerp x) x) ((stringp x) x)))");
        assert_eq!(report.sites.len(), 2, "{report:?}");
        assert_eq!(report.sites[0].narrowed_to, Ty::Integer);
        assert_eq!(report.sites[1].narrowed_to, Ty::String);
    }

    #[test]
    fn a_typecase_clause_narrows_to_its_type_specifier() {
        let report = report("(defun f (x) (typecase x (integer x) (string x)))");
        assert_eq!(report.sites.len(), 2, "{report:?}");
        assert_eq!(report.sites[0].source, NarrowingSource::TypecaseClause);
        assert_eq!(report.sites[0].narrowed_to, Ty::Integer);
        assert_eq!(report.sites[1].narrowed_to, Ty::String);
    }

    #[test]
    fn a_typecase_clause_naming_an_unmodelled_type_is_skipped() {
        let report = report("(defun f (x) (typecase x (my-class x)))");
        assert!(report.sites.is_empty(), "{report:?}");
    }

    #[test]
    fn a_predicate_on_something_that_is_not_a_binding_is_not_a_narrowing() {
        // `*global*` is free, so the binding table does not resolve it and
        // there is nothing to attribute the narrowing to.
        let report = report("(defun f () (when (stringp *global*) 1))");
        assert!(report.sites.is_empty(), "{report:?}");
    }

    #[test]
    fn a_test_that_is_not_a_type_predicate_narrows_nothing() {
        let report = report("(defun f (x) (when (evenp x) x))");
        assert!(report.sites.is_empty(), "{report:?}");
    }

    #[test]
    fn findings_are_sorted_by_source_position() {
        let report = report(
            "(defun f (x) (list (when (stringp x) x) (when (integerp x) x) (when (consp x) x)))",
        );
        let starts = report
            .sites
            .iter()
            .map(|site| site.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 3);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let report = report_of("(when (string? x) x)", Dialect::Clojure);
        assert!(!report.dialect_modelled);
        assert!(report.sites.is_empty());
    }

    #[test]
    fn the_narrowing_is_scoped_to_the_whole_branching_form() {
        let source = "(defun f (x) (when (stringp x) x))";
        let report = report(source);
        let span = report.sites[0].span;
        assert_eq!(
            &source[span.start().get()..span.end().get()],
            "(when (stringp x) x)"
        );
    }
}
