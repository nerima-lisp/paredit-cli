//! What every rule is handed besides the node it is looking at.

use std::cell::OnceCell;
use std::path::Path;

use paredit_core_semantics::semantics::binding::{BindingTable, build_binding_table};
use paredit_core_semantics::semantics::typing::TypeTable;
use paredit_core_semantics::semantics::typing::service::build_type_table;
use paredit_core_semantics::semantics::value::{ValueTable, build_value_table};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};

use crate::model::{RuleSetting, RuleSettings};

/// The read-only surroundings of a check: which file, which dialect, the whole
/// tree, and the exact source bytes.
///
/// The source matters because a fix copies untouched sub-forms verbatim rather
/// than re-printing them, which is how formatting survives an autofix. The
/// whole tree matters for the handful of rules that correlate separate
/// definitions.
///
/// It is also where the semantic layers hang. Each table is built on first
/// use, so a run whose active rules ask for none of them pays for none of
/// them, and a run with a hundred rules that all ask builds each exactly once.
///
/// The tables own their data and link to each other by id rather than by
/// borrow. A `ValueTable` holding `&BindingTable` would make this struct
/// self-referential, which is why [`paredit_core_semantics::semantics`] indexes
/// everything by `BindingId`.
#[derive(Debug)]
pub struct RuleContext<'a> {
    path: &'a Path,
    dialect: Dialect,
    tree: &'a SyntaxTree,
    source: &'a str,
    settings: &'a RuleSettings,
    bindings: OnceCell<BindingTable>,
    values: OnceCell<ValueTable>,
    types: OnceCell<TypeTable>,
}

/// The settings a context carries when the caller supplied none, which is every
/// run without a `--rule-arg`. A `static` so [`RuleContext::new`] stays `const`
/// and allocates nothing for the common case.
static NO_SETTINGS: RuleSettings = RuleSettings::empty();

impl<'a> RuleContext<'a> {
    #[must_use]
    pub const fn new(
        path: &'a Path,
        dialect: Dialect,
        tree: &'a SyntaxTree,
        source: &'a str,
    ) -> Self {
        Self {
            path,
            dialect,
            tree,
            source,
            settings: &NO_SETTINGS,
            bindings: OnceCell::new(),
            values: OnceCell::new(),
            types: OnceCell::new(),
        }
    }

    /// Supplies the caller's `--rule-arg` overrides.
    #[must_use]
    pub const fn with_settings(mut self, settings: &'a RuleSettings) -> Self {
        self.settings = settings;
        self
    }

    /// The value `rule` should use for the knob it declared as `setting`.
    ///
    /// A rule passes its own name and its own `RuleSetting` constant, so a
    /// misspelled key cannot compile and a rule cannot read another rule's
    /// threshold.
    #[must_use]
    pub fn setting(&self, rule: &str, setting: RuleSetting) -> i64 {
        self.settings.get(rule, setting)
    }

    /// Every binding in this file, and what each reference resolves to.
    ///
    /// Built on first use; `&self` rather than `&mut self` so a rule can hold
    /// the node it is checking at the same time.
    pub fn binding_table(&self) -> &BindingTable {
        self.bindings
            .get_or_init(|| build_binding_table(self.dialect, self.tree, self.source))
    }

    /// What each binding in this file provably evaluates to.
    pub fn value_table(&self) -> &ValueTable {
        self.values
            .get_or_init(|| build_value_table(self.dialect, self.tree, self.binding_table()))
    }

    /// What each expression in this file is provably of type.
    ///
    /// Stacks on the two tables above it, so asking for this builds those too;
    /// a run whose rules ask for none of the three still pays for none.
    pub fn type_table(&self) -> &TypeTable {
        self.types.get_or_init(|| {
            build_type_table(
                self.dialect,
                self.tree,
                self.binding_table(),
                self.value_table(),
            )
        })
    }

    pub const fn path(&self) -> &'a Path {
        self.path
    }

    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }

    pub const fn tree(&self) -> &'a SyntaxTree {
        self.tree
    }

    /// The file's exact source text.
    ///
    /// Needed by the rules whose subject is a *comment*: Emacs Lisp decides
    /// whether a file is lexically bound from a `-*- … -*-` header on line 1,
    /// which is nowhere in the tree.
    pub const fn source(&self) -> &'a str {
        self.source
    }

    /// The exact source of `span`, or `""` if the span is not a character
    /// boundary of this document. Spans handed to a rule come from the tree, so
    /// the fallback is unreachable in practice and exists only to keep fix
    /// synthesis total.
    pub fn slice(&self, span: ByteSpan) -> &'a str {
        self.source
            .get(span.start().get()..span.end().get())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::ByteOffset;

    #[test]
    fn slices_the_exact_source_of_a_span() {
        let input = "(progn (or x))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let context = RuleContext::new(Path::new("test.lisp"), Dialect::CommonLisp, &tree, input);
        let span = ByteSpan::new(ByteOffset::new(7), ByteOffset::new(13));
        assert_eq!(context.slice(span), "(or x)");
    }

    #[test]
    fn a_context_with_no_overrides_reads_the_declared_default() {
        const MAX: RuleSetting = RuleSetting::new("max", 5, "the deepest acceptable nesting");
        let input = "(a)";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let context = RuleContext::new(Path::new("test.lisp"), Dialect::CommonLisp, &tree, input);
        assert_eq!(context.setting("nesting-depth", MAX), 5);
    }

    #[test]
    fn a_supplied_override_reaches_the_rule_that_declared_it() {
        const MAX: RuleSetting = RuleSetting::new("max", 5, "the deepest acceptable nesting");
        let input = "(a)";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let mut settings = RuleSettings::new();
        settings.set("nesting-depth", "max", 12);
        let context = RuleContext::new(Path::new("test.lisp"), Dialect::CommonLisp, &tree, input)
            .with_settings(&settings);
        assert_eq!(context.setting("nesting-depth", MAX), 12);
        assert_eq!(context.setting("function-length", MAX), 5);
    }

    #[test]
    fn an_out_of_range_span_slices_to_empty() {
        let input = "(a)";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let context = RuleContext::new(Path::new("test.lisp"), Dialect::CommonLisp, &tree, input);
        let span = ByteSpan::new(ByteOffset::new(0), ByteOffset::new(99));
        assert_eq!(context.slice(span), "");
    }
}
