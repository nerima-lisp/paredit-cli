//! `implementation-package-symbol`: a symbol from an implementation's own
//! package.
//!
//! `sb-ext:run-program` is SBCL. `ccl:external-call` is Clozure. Neither
//! exists in the other, and code that names one will not load on the other at
//! all — not "behave differently", not load.
//!
//! The detection is a package-prefix match, which is exact rather than
//! heuristic: a symbol either carries a qualifier or it does not, and the
//! qualifier either names a known implementation package or it does not.
//! Nothing is inferred.
//!
//! The one thing worth a decision is the reader-conditional exception. A symbol
//! inside `#+sbcl` is portable *code* — the whole point of `#+` is to fence off
//! the unportable part — and reporting it would flag the correct idiom
//! alongside the incorrect one. The reader hands `#+sbcl (sb-ext:gc)` back as a
//! single atom containing guard and guarded form together, so the exception is
//! one predicate on the atom rather than an ancestor walk.
//!
//! The head filter is the whole document because the subject is a symbol in any
//! position — an argument, a `funcall` target, a `defpackage` `:import-from`
//! entry — and none of those is reachable from a head.
//!
//! Report-only: the portable replacement for an implementation extension is a
//! design decision, and often there is not one.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, for_each_subview};

pub const META: RuleMeta = RuleMeta::new(
    "implementation-package-symbol",
    RuleCategory::Portability,
    Severity::Warning,
    "a symbol from an implementation's own package, outside any reader conditional guarding it",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A symbol qualified with an implementation's package does not exist in any other \
         implementation, so a file naming one fails to read there rather than failing to work. \
         Guarding it with a reader conditional is what makes the dependency explicit and the file \
         portable.",
    )
    .with_example(
        "(sb-ext:run-program \"ls\" nil)",
        "#+sbcl (sb-ext:run-program \"ls\" nil)",
    )
    .with_caveat(
        "A symbol already inside a `#+`/`#-` form is not reported, at any depth: fencing the \
         unportable part off is precisely the remedy, and flagging it would report the fix.",
    ),
);

/// Package qualifiers that name one implementation's private world.
///
/// Deliberately not exhaustive over everything an implementation exports —
/// `sb-` alone would sweep in a project that happens to use that prefix — but
/// exhaustive over the prefixes that occur in practice.
const IMPLEMENTATION_PACKAGES: [&str; 21] = [
    "sb-ext",
    "sb-kernel",
    "sb-int",
    "sb-alien",
    "sb-thread",
    "sb-posix",
    "sb-unix",
    "sb-sys",
    "sb-mop",
    "sb-gray",
    "sb-bsd-sockets",
    "ccl",
    "excl",
    "clisp",
    "lispworks",
    "system",
    "sys",
    "si",
    "ext",
    "mp",
    "acl-compat",
];

/// One implementation-specific symbol, and which implementation it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementationSymbol {
    pub span: ByteSpan,
    pub symbol: String,
    pub package: &'static str,
}

/// The implementation package `symbol` is qualified with, if any.
///
/// Reads the qualifier before the *first* colon, so `sb-ext::internal` and
/// `sb-ext:public` are both attributed to `sb-ext`. A leading colon marks a
/// keyword, which has no package part to read.
#[must_use]
pub fn implementation_package(symbol: &str) -> Option<&'static str> {
    if symbol.starts_with(':') {
        return None;
    }
    let (qualifier, rest) = symbol.split_once(':')?;
    if rest.is_empty() {
        return None;
    }
    IMPLEMENTATION_PACKAGES
        .iter()
        .copied()
        .find(|package| qualifier.eq_ignore_ascii_case(package))
}

/// Whether `view` is a reader-conditional region.
///
/// The Common Lisp reader hands `#+sbcl (sb-ext:gc)` back as a *single atom*
/// whose text is the whole thing, guard and guarded form together. That is what
/// makes this rule's exception a one-line test rather than an ancestor walk:
/// there is nothing inside a guarded region to descend into, so skipping the
/// atom skips everything it fences off.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Reads one node and reports the implementation symbol it is, if any.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<ImplementationSymbol> {
    if is_reader_conditional(view) {
        return None;
    }
    let text = atom_text(view)?;
    implementation_package(text).map(|package| ImplementationSymbol {
        span: view.span,
        symbol: text.to_owned(),
        package,
    })
}

/// Every unguarded implementation symbol under `root`.
///
/// The dispatcher reaches each node on its own, so the rule itself uses
/// [`examine`]; this exists for the tests, which want a whole document's answer
/// in one call.
#[must_use]
pub fn collect(root: &ExpressionView) -> Vec<ImplementationSymbol> {
    let mut found = Vec::new();
    for_each_subview(root, |view| found.extend(examine(view)));
    found
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // Every node, not every head: the subject is a symbol wherever it
        // appears, and most appearances are not in operator position.
        HeadFilter::AllNodes
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // Exactly this node, not its subtree: the dispatcher will visit every
        // descendant in turn, and a rule that walked down from each of them
        // would report each symbol once per ancestor.
        if let Some(symbol) = examine(view) {
            sink.report(
                symbol.span,
                format!(
                    "{} is in the {} package, which only one implementation has; \
                     guard it with a reader conditional if the dependency is intended",
                    symbol.symbol, symbol.package
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn symbols(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect(&tree.root_view())
            .into_iter()
            .map(|item| item.symbol)
            .collect()
    }

    #[test]
    fn flags_an_unguarded_implementation_symbol() {
        assert_eq!(
            symbols("(sb-ext:run-program \"ls\" nil)"),
            vec!["sb-ext:run-program"]
        );
    }

    #[test]
    fn flags_the_double_colon_spelling_too() {
        assert_eq!(
            symbols("(sb-kernel::%instance-ref x 0)"),
            vec!["sb-kernel::%instance-ref"]
        );
    }

    #[test]
    fn does_not_flag_a_symbol_inside_a_reader_conditional() {
        assert!(symbols("#+sbcl (sb-ext:run-program \"ls\" nil)").is_empty());
        assert!(symbols("#-ccl (sb-ext:gc)").is_empty());
    }

    #[test]
    fn does_not_flag_a_guarded_form_nested_in_a_larger_one() {
        assert!(symbols("(defun f () #+sbcl (sb-ext:gc))").is_empty());
    }

    #[test]
    fn still_flags_an_unguarded_sibling_of_a_guarded_form() {
        // The conditional guards one form, not the rest of the body.
        assert_eq!(
            symbols("(defun f () #+sbcl (sb-ext:gc) (ccl:heap-utilization))"),
            vec!["ccl:heap-utilization"]
        );
    }

    #[test]
    fn does_not_flag_an_ordinary_package_qualified_symbol() {
        assert!(symbols("(alexandria:flatten xs)").is_empty());
        assert!(symbols("(cl:length xs)").is_empty());
        assert!(symbols("(my-app:run)").is_empty());
    }

    #[test]
    fn does_not_flag_a_keyword() {
        assert!(symbols("(list :sb-ext :ext)").is_empty());
    }

    #[test]
    fn does_not_flag_an_unqualified_symbol_that_merely_starts_the_same() {
        assert!(symbols("(sb-ext-lookalike x)").is_empty());
    }

    #[test]
    fn reads_the_qualifier_case_insensitively() {
        assert_eq!(symbols("(SB-EXT:GC)"), vec!["SB-EXT:GC"]);
    }

    #[test]
    fn finds_several_symbols_across_the_document() {
        assert_eq!(
            symbols("(sb-ext:gc)\n(ccl:gc)\n"),
            vec!["sb-ext:gc", "ccl:gc"]
        );
    }
}
