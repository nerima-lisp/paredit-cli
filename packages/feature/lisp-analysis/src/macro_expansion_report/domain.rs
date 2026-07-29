//! What a `defmacro` in this file expands its own call sites into.
//!
//! Reading a macro call means holding the template and the arguments in your
//! head at once and doing the substitution by eye. That is exactly the work a
//! tool should do, and no report in this project did it: `macrolet` and
//! `define-symbol-macro` are handled by the refactor layer, but a plain
//! `defmacro` had nothing.
//!
//! **This does not evaluate.** It is a template substitution and nothing more:
//! parameters are replaced by the argument text the call site wrote, and the
//! result is printed. That is enough for the case that actually costs time —
//! "what does this call turn into" — and it is honest about everything else.
//!
//! What it declines, and says so rather than guessing:
//!
//! - A body that is not a single quasiquoted template. A macro that computes
//!   its expansion is a program, and running it is evaluation.
//! - A lambda list with `&optional`, `&key`, or destructuring. The mapping from
//!   arguments to parameters is then a small algorithm with defaults, and
//!   getting it subtly wrong would be worse than declining.
//! - Nested expansion. The result is expanded once; if it contains another
//!   macro call, that call is left as written.
//!
//! Every declined call is reported with its reason, so the output is a complete
//! account of the call sites rather than a filtered one.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding, line_of};

/// Why an expansion was not produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declined {
    /// The macro body is not a single quasiquoted template.
    ComputedExpansion,
    /// The lambda list uses `&optional`, `&key`, `&aux`, or destructuring.
    UnsupportedLambdaList,
    /// The call supplies the wrong number of arguments.
    ArityMismatch,
}

impl Declined {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ComputedExpansion => "computed-expansion",
            Self::UnsupportedLambdaList => "unsupported-lambda-list",
            Self::ArityMismatch => "arity-mismatch",
        }
    }
}

/// One call site of a macro defined in the same file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    pub macro_name: String,
    /// The call as written.
    pub call: String,
    /// The substituted template, or `None` when the expansion was declined.
    pub expansion: Option<String>,
    /// Why it was declined, if it was.
    pub declined: Option<Declined>,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for Expansion {
    fn kind(&self) -> &'static str {
        self.declined
            .map_or("expanded", |declined| declined.label())
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.macro_name.clone(),
            self.call.clone(),
            self.expansion.clone().unwrap_or_else(|| "-".to_owned()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("macro_name", json!(self.macro_name)),
            ("call", json!(self.call)),
            ("expansion", json!(self.expansion)),
            ("declined", json!(self.declined.map(Declined::label))),
        ]
    }
}

/// A macro definition, reduced to what substitution needs.
struct MacroDefinition {
    /// Required parameter names, folded.
    parameters: Vec<String>,
    /// The `&body`/`&rest` parameter, if any.
    rest: Option<String>,
    /// The template text, without its leading backquote.
    template: Option<String>,
    unsupported_lambda_list: bool,
}

#[must_use]
pub fn build_macro_expansion_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<Expansion> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();
    let mut macros: BTreeMap<String, MacroDefinition> = BTreeMap::new();

    if modelled {
        for form in &tree.root_view().children {
            if let Some((name, definition)) = read_macro(form, source) {
                macros.entry(name).or_insert(definition);
            }
        }
    }

    let mut findings = Vec::new();
    if modelled && !macros.is_empty() {
        collect_calls(&tree.root_view(), &macros, source, &mut findings);
    }

    let expanded = findings
        .iter()
        .filter(|finding: &&Expansion| finding.declined.is_none())
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        findings,
        vec![
            ("macro_count", json!(macros.len())),
            ("expanded_count", json!(expanded)),
        ],
    )
}

fn read_macro(form: &ExpressionView, source: &str) -> Option<(String, MacroDefinition)> {
    let head = list_head(form)?;
    if !common_lisp_operator_head_eq(head, "defmacro") {
        return None;
    }
    let name = atom_symbol_text(form.children.get(1)?)?.to_ascii_uppercase();
    let lambda_list = form.children.get(2)?;

    let mut parameters = Vec::new();
    let mut rest = None;
    let mut unsupported = false;
    let mut in_rest = false;
    for parameter in &lambda_list.children {
        match atom_symbol_text(parameter) {
            Some(text) if text.starts_with('&') => {
                if text.eq_ignore_ascii_case("&body") || text.eq_ignore_ascii_case("&rest") {
                    in_rest = true;
                } else {
                    unsupported = true;
                }
            }
            Some(text) if in_rest => rest = Some(text.to_ascii_uppercase()),
            Some(text) => parameters.push(text.to_ascii_uppercase()),
            // A destructuring pattern rather than a name.
            None => unsupported = true,
        }
    }

    // Only a body that is exactly one quasiquoted form is a template. Anything
    // else — a `let` prelude, a `cond`, several forms — is a program.
    let body = form.children.get(3..).unwrap_or_default();
    let template = (body.len() == 1)
        .then(|| body.first())
        .flatten()
        .filter(|view| is_quasiquoted(view))
        .and_then(|view| {
            source
                .get(view.span.start().get()..view.span.end().get())
                .map(|text| text.trim_start_matches('`').to_owned())
        });

    Some((
        name,
        MacroDefinition {
            parameters,
            rest,
            template,
            unsupported_lambda_list: unsupported,
        },
    ))
}

fn collect_calls(
    view: &ExpressionView,
    macros: &BTreeMap<String, MacroDefinition>,
    source: &str,
    findings: &mut Vec<Expansion>,
) {
    if let Some(head) = list_head(view) {
        let folded = head.to_ascii_uppercase();
        // The `defmacro` form itself has the macro's name in child 1, not in
        // head position, so it is never mistaken for a call.
        if let Some(definition) = macros.get(&folded) {
            findings.push(expand(view, &folded, definition, source));
        }
    }
    for child in &view.children {
        collect_calls(child, macros, source, findings);
    }
}

fn expand(
    call: &ExpressionView,
    name: &str,
    definition: &MacroDefinition,
    source: &str,
) -> Expansion {
    let call_text = collapse(text_of(call, source));
    let mut finding = Expansion {
        macro_name: name.to_owned(),
        call: call_text,
        expansion: None,
        declined: None,
        span: call.span,
        line: line_of(source, call.span.start().get()),
    };

    if definition.unsupported_lambda_list {
        finding.declined = Some(Declined::UnsupportedLambdaList);
        return finding;
    }
    let Some(template) = definition.template.as_deref() else {
        finding.declined = Some(Declined::ComputedExpansion);
        return finding;
    };

    let arguments = &call.children[1..];
    let required = definition.parameters.len();
    if arguments.len() < required || (definition.rest.is_none() && arguments.len() > required) {
        finding.declined = Some(Declined::ArityMismatch);
        return finding;
    }

    let mut expansion = template.to_owned();
    for (parameter, argument) in definition.parameters.iter().zip(arguments) {
        expansion = substitute(&expansion, parameter, &text_of(argument, source), false);
    }
    if let Some(rest) = &definition.rest {
        let spliced = arguments[required..]
            .iter()
            .map(|argument| text_of(argument, source))
            .collect::<Vec<_>>()
            .join(" ");
        expansion = substitute(&expansion, rest, &spliced, true);
    }

    finding.expansion = Some(collapse(expansion));
    finding
}

/// Replaces `,name` (and `,@name` when `splicing`) with `replacement`.
///
/// Matching is on the unquote rather than on the bare name, which is what
/// keeps a parameter called `list` from rewriting every `(list …)` in the
/// template: only an unquoted occurrence is a substitution site.
fn substitute(template: &str, parameter: &str, replacement: &str, splicing: bool) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(position) = rest.find(',') {
        out.push_str(&rest[..position]);
        let after = &rest[position + 1..];
        let (marker, tail) = match after.strip_prefix('@') {
            Some(tail) if splicing => ("@", tail),
            _ => ("", after),
        };

        let end = tail
            .find(|character: char| !is_symbol_character(character))
            .unwrap_or(tail.len());
        let name = &tail[..end];

        if name.eq_ignore_ascii_case(parameter) {
            out.push_str(replacement);
        } else {
            out.push(',');
            out.push_str(marker);
            out.push_str(name);
        }
        rest = &tail[end..];
    }

    out.push_str(rest);
    out
}

const fn is_symbol_character(character: char) -> bool {
    !matches!(
        character,
        ' ' | '\t' | '\n' | '\r' | '(' | ')' | '\'' | '`' | ',' | '"' | ';'
    )
}

fn text_of(view: &ExpressionView, source: &str) -> String {
    source
        .get(view.span.start().get()..view.span.end().get())
        .unwrap_or_default()
        .to_owned()
}

fn collapse(text: String) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_quasiquoted(view: &ExpressionView) -> bool {
    use paredit_core_syntax::sexpr::ReaderPrefix;
    view.reader_prefixes.contains(&ReaderPrefix::Quasiquote)
        || view
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with('`'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<Expansion> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_macro_expansion_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn only(source: &str) -> Expansion {
        let report = report(source);
        assert_eq!(report.findings.len(), 1, "{report:?}");
        report.findings[0].clone()
    }

    #[test]
    fn a_simple_template_expands_at_its_call_site() {
        let finding =
            only("(defmacro twice (form) `(progn ,form ,form))\n(defun f () (twice (bump)))");
        assert_eq!(finding.expansion.as_deref(), Some("(progn (bump) (bump))"));
        assert_eq!(finding.kind(), "expanded");
    }

    #[test]
    fn a_body_parameter_splices_every_remaining_argument() {
        let finding = only(
            "(defmacro guard (test &body body) `(when ,test ,@body))\n\
             (defun f () (guard (ready-p) (a) (b)))",
        );
        assert_eq!(
            finding.expansion.as_deref(),
            Some("(when (ready-p) (a) (b))")
        );
    }

    #[test]
    fn a_parameter_name_is_replaced_only_where_it_is_unquoted() {
        // `list` appears in the template as an operator and as an unquote; only
        // the unquote is a substitution site.
        let finding = only("(defmacro m (list) `(list ,list))\n(defun f () (m 1))");
        assert_eq!(finding.expansion.as_deref(), Some("(list 1)"));
    }

    #[test]
    fn a_computed_expansion_is_declined_rather_than_guessed_at() {
        let finding =
            only("(defmacro m (x) (let ((g (gensym))) `(let ((,g ,x)) ,g)))\n(defun f () (m 1))");
        assert_eq!(finding.declined, Some(Declined::ComputedExpansion));
        assert!(finding.expansion.is_none());
    }

    #[test]
    fn a_keyword_lambda_list_is_declined() {
        let finding = only("(defmacro m (x &key y) `(list ,x ,y))\n(defun f () (m 1 :y 2))");
        assert_eq!(finding.declined, Some(Declined::UnsupportedLambdaList));
    }

    #[test]
    fn a_call_with_the_wrong_argument_count_is_declined() {
        let finding = only("(defmacro m (x y) `(list ,x ,y))\n(defun f () (m 1))");
        assert_eq!(finding.declined, Some(Declined::ArityMismatch));
    }

    #[test]
    fn a_macro_with_no_call_site_produces_no_finding() {
        let report = report("(defmacro m (x) `(list ,x))");
        assert!(report.findings.is_empty(), "{report:?}");
        assert_eq!(report.summary[0], ("macro_count", json!(1)));
    }

    #[test]
    fn a_call_to_a_macro_defined_elsewhere_is_not_expanded() {
        let report = report("(defun f () (some-other-macro 1))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn every_call_site_is_reported_not_just_the_first() {
        let report = report("(defmacro m (x) `(list ,x))\n(defun f () (m 1))\n(defun g () (m 2))");
        assert_eq!(report.findings.len(), 2, "{report:?}");
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree =
            SyntaxTree::parse_with_dialect("(defmacro m [x] x)", Dialect::Clojure).expect("parse");
        let report = build_macro_expansion_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
