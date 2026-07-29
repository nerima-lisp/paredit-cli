//! Common Lisp `typep`-to-predicate detection: a `(typep x 'TYPE)` whose type
//! specifier is a quoted CL type name that has a dedicated total predicate. For
//! these types `(typep x 'TYPE)` and `(PRED x)` are exactly equivalent — same
//! boolean for every object, `x` evaluated once — and the named predicate reads
//! more directly. For example `(typep x 'string)` is `(stringp x)`,
//! `(typep x 'null)` is `(null x)`, `(typep x 'list)` is `(listp x)`.
//!
//! Only type names in the `TYPE_PREDICATES` table are flagged (each mapping is
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

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

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
    /// The span of the whole `(typep x 'TYPE)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The dedicated predicate name to rewrite to (`stringp`, `null`, ...).
    pub predicate: &'static str,
    /// The span of the object operand `x`.
    pub object_span: ByteSpan,
}

impl Finding for TypepPredicateItem {
    /// The dedicated predicate, so `(typep x 'string)` and `(typep x 'null)`
    /// are separable without parsing JSON.
    ///
    /// It is a tag rather than data: it comes from `TYPE_PREDICATES`, a closed
    /// table of canonical names, not from the spelling in the source — a
    /// `(TYPEP x 'STRING)` and a `(typep x 'string)` both report `stringp`.
    fn kind(&self) -> &'static str {
        self.predicate
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// None: the one column the old text row carried was the predicate, and it
    /// now leads every row as the kind.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("predicate", json!(self.predicate)),
            (
                "object_span",
                json!({
                    "start": self.object_span.start().get(),
                    "end": self.object_span.end().get(),
                }),
            ),
        ]
    }

    /// The same sentence the `typep-predicate` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "typep against this type has a dedicated predicate; use ({} x)",
            self.predicate
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
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
        span: view.span,
        line: line_of(source, view.span.start().get()),
        predicate,
        object_span: object.span,
    });
}

/// Collects every `(typep x 'TYPE)` with a dedicated predicate in one file,
/// with the number of `typep` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no replaceable `typep` here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_typep_predicate_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<TypepPredicateItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("typep_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut typep_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut typep_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("typep_form_count", json!(typep_form_count))],
    ))
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<TypepPredicateItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_typep_predicate_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build typep predicate report")
    }

    /// The `(typep_form_count, violations)` pair the report is built from.
    fn typeps(input: &str) -> (u64, Vec<TypepPredicateItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "typep_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("typep_form_count in the summary");
        (count, report.findings)
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

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(typep x 'string)", Dialect::Clojure).expect("parse");
        let report = build_typep_predicate_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build typep predicate report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("typep_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(typep x 'fixnum)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_predicate_and_its_object_span() {
        let report = report("(defun f (obj)\n  (typep obj 'string))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "stringp");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("predicate", json!("stringp")),
                (
                    "object_span",
                    json!({
                        "start": finding.object_span.start().get(),
                        "end": finding.object_span.end().get(),
                    })
                ),
            ]
        );
        // The predicate leads the row as the kind, so no column repeats it.
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_typep_scanned_not_only_the_flagged_ones() {
        let report = report("(typep x 'string)\n(typep x 'fixnum)\n(typep x 'list)\n");
        assert_eq!(report.summary, vec![("typep_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
