//! Common Lisp `typep`-to-predicate detection: a `(typep x 'TYPE)` whose type
//! specifier is a quoted CL type name that has a dedicated total predicate. For
//! these types `(typep x 'TYPE)` and `(PRED x)` are exactly equivalent — same
//! boolean for every object, `x` evaluated once — and the named predicate reads
//! more directly. For example `(typep x 'string)` is `(stringp x)`,
//! `(typep x 'null)` is `(null x)`, `(typep x 'list)` is `(listp x)`.
//!
//! Only type names in the [`TYPE_PREDICATES`] table are flagged (each mapping is
//! a CLHS-guaranteed exact equivalence). A compound type specifier
//! (`(integer 0 9)`), a type with no dedicated predicate (`fixnum`), the
//! always-true `t`, an unquoted or computed type argument, and a
//! reader-conditional `x` are all left alone. The type name may be written with
//! the `'` reader prefix or the explicit `(quote TYPE)` form.
//!
//! The fix rewrites `(typep x 'TYPE)` as `(PRED x)`, copying `x`'s source, so the
//! rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// CL type names that have a dedicated total predicate exactly equivalent to
/// `(typep x 'TYPE)`. Each pair is a CLHS-guaranteed equivalence.
const TYPE_PREDICATES: [(&str, &str); 21] = [
    ("null", "null"),
    ("symbol", "symbolp"),
    ("atom", "atom"),
    ("cons", "consp"),
    ("list", "listp"),
    ("number", "numberp"),
    ("integer", "integerp"),
    ("rational", "rationalp"),
    ("float", "floatp"),
    ("complex", "complexp"),
    ("real", "realp"),
    ("character", "characterp"),
    ("string", "stringp"),
    ("vector", "vectorp"),
    ("array", "arrayp"),
    ("hash-table", "hash-table-p"),
    ("function", "functionp"),
    ("keyword", "keywordp"),
    ("package", "packagep"),
    ("pathname", "pathnamep"),
    ("stream", "streamp"),
];

/// The quoted symbol name in a `'sym` atom or `(quote sym)` list, or `None`.
fn quoted_symbol(view: &ExpressionView) -> Option<String> {
    if let Some(text) = atom_text(view) {
        // A prefixed atom's `text` includes the prefix spelling; the symbol
        // content begins at `symbol_offset` (so `'string` -> `string`).
        if view.reader_prefixes.len() == 1 && matches!(view.reader_prefixes[0], ReaderPrefix::Quote)
        {
            return Some(text.get(view.symbol_offset..).unwrap_or(text).to_owned());
        }
        return None;
    }
    if is_paren_list(view)
        && view.children.len() == 2
        && view.reader_prefixes.is_empty()
        && list_head(view).is_some_and(|h| h.eq_ignore_ascii_case("quote"))
    {
        return atom_text(&view.children[1]).map(str::to_owned);
    }
    None
}

/// The dedicated predicate for a quoted type specifier, or `None`.
fn type_predicate(view: &ExpressionView) -> Option<&'static str> {
    let name = quoted_symbol(view)?;
    TYPE_PREDICATES
        .iter()
        .find(|(ty, _)| name.eq_ignore_ascii_case(ty))
        .map(|(_, pred)| *pred)
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct TypepPredicateItem {
    pub path: PathBuf,
    /// The span of the whole `(typep x 'TYPE)` form.
    pub span: ByteSpan,
    /// The dedicated predicate name to rewrite to (`stringp`, `null`, ...).
    pub predicate: &'static str,
    /// The span of the object operand `x`.
    pub object_span: ByteSpan,
}

#[derive(Debug)]
pub struct TypepPredicateSummary {
    pub typep_form_count: usize,
    pub violations: Vec<TypepPredicateItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct TypepPredicatePolicyOptions {
    fail_on_violation: bool,
}

impl TypepPredicatePolicyOptions {
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
pub struct TypepPredicatePolicy {
    pub fail_on_violation: bool,
    pub typep_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    typep_form_count: &mut usize,
    violations: &mut Vec<TypepPredicateItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("typep") {
        return;
    }
    *typep_form_count += 1;

    // children: [typep, object, type-spec] — the two-argument shape (no
    // environment argument).
    if view.children.len() != 3 {
        return;
    }
    let object = &view.children[1];
    let type_spec = &view.children[2];
    if is_reader_conditional(object) {
        return;
    }
    let Some(predicate) = type_predicate(type_spec) else {
        return;
    };

    violations.push(TypepPredicateItem {
        path: path.to_path_buf(),
        span: view.span,
        predicate,
        object_span: object.span,
    });
}

/// Collects every `(typep x 'TYPE)` with a dedicated predicate across a whole
/// file, along with the total number of `typep` forms scanned.
pub fn collect_typep_predicates(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<TypepPredicateItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut typep_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut typep_form_count, &mut violations);
        });
    }
    Ok((typep_form_count, violations))
}

#[must_use]
pub const fn summarize_typep_predicates(
    typep_form_count: usize,
    violations: Vec<TypepPredicateItem>,
) -> TypepPredicateSummary {
    TypepPredicateSummary {
        typep_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_typep_predicate_policy(
    options: TypepPredicatePolicyOptions,
    summary: &TypepPredicateSummary,
) -> TypepPredicatePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    TypepPredicatePolicy {
        fail_on_violation: options.fail_on_violation(),
        typep_form_count: summary.typep_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typeps(input: &str) -> (usize, Vec<TypepPredicateItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_typep_predicates(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect typep predicates")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_typep_string() {
        let source = "(typep obj 'string)";
        let (count, violations) = typeps(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].predicate, "stringp");
        assert_eq!(slice(source, violations[0].object_span), "obj");
    }

    #[test]
    fn maps_representative_types() {
        assert_eq!(typeps("(typep x 'null)").1[0].predicate, "null");
        assert_eq!(typeps("(typep x 'list)").1[0].predicate, "listp");
        assert_eq!(typeps("(typep x 'integer)").1[0].predicate, "integerp");
        assert_eq!(
            typeps("(typep x 'hash-table)").1[0].predicate,
            "hash-table-p"
        );
        assert_eq!(typeps("(typep x 'atom)").1[0].predicate, "atom");
    }

    #[test]
    fn recognizes_explicit_quote_form() {
        let (_, violations) = typeps("(typep x (quote symbol))");
        assert_eq!(violations[0].predicate, "symbolp");
    }

    #[test]
    fn preserves_compound_object() {
        let source = "(typep (car x) 'cons)";
        let (_, violations) = typeps(source);
        assert_eq!(slice(source, violations[0].object_span), "(car x)");
    }

    #[test]
    fn does_not_flag_type_without_dedicated_predicate() {
        assert!(typeps("(typep x 'fixnum)").1.is_empty());
        assert!(typeps("(typep x 'standard-object)").1.is_empty());
    }

    #[test]
    fn does_not_flag_compound_type_specifier() {
        assert!(typeps("(typep x '(integer 0 9))").1.is_empty());
    }

    #[test]
    fn does_not_flag_type_t() {
        // (typep x t) is always true; no "tp" predicate.
        assert!(typeps("(typep x t)").1.is_empty());
    }

    #[test]
    fn does_not_flag_environment_argument() {
        // The three-argument (typep x type env) shape is left alone.
        assert!(typeps("(typep x 'string env)").1.is_empty());
    }

    #[test]
    fn case_folds_head_and_type() {
        let (_, violations) = typeps("(TYPEP x 'STRING)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].predicate, "stringp");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(typep x 'string)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_typep_predicates(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect typep predicates");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = typeps("(typep x 'string)");
        let summary = summarize_typep_predicates(count, items);

        let quiet =
            evaluate_typep_predicate_policy(TypepPredicatePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_typep_predicate_policy(TypepPredicatePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
