//! Call-site arity checking that understands `&optional`, `&rest`, and `&key`.
//!
//! `inspect signature` compares positional counts, which is the right check for
//! a simple lambda list and the wrong one for every other kind. A function
//! taking `(a &key width height)` accepts one, three, or five arguments and
//! rejects two — a rule no positional count can express.
//!
//! The `&key` half is what makes this worth having. A call passing `:widht` to
//! a function taking `:width` is a runtime error on the branch that reaches it,
//! and it is invisible to a count-based check because the *number* of arguments
//! is right. Reporting the unknown keyword by name is the finding.
//!
//! Calls are matched to definitions in the same analyzed set, by name. A call
//! to something defined elsewhere is not reported: with no lambda list in hand
//! there is nothing to check against, and guessing would produce a finding
//! about a function this run never saw.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head};
use serde_json::{Value, json};

/// What is wrong with one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArityFault {
    TooFewArguments,
    TooManyArguments,
    /// A `:keyword` the lambda list does not accept.
    UnknownKeyword,
    /// A keyword argument with no value after it.
    OddKeywordArguments,
}

impl ArityFault {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::TooFewArguments => "too-few-arguments",
            Self::TooManyArguments => "too-many-arguments",
            Self::UnknownKeyword => "unknown-keyword",
            Self::OddKeywordArguments => "odd-keyword-arguments",
        }
    }
}

/// One call that does not fit its callee's lambda list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArityFinding {
    pub fault: ArityFault,
    pub callee: String,
    /// The offending keyword, for [`ArityFault::UnknownKeyword`].
    pub keyword: Option<String>,
    pub supplied: usize,
    /// The callee's lambda list, so the finding can be judged in place.
    pub lambda_list: String,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for ArityFinding {
    fn kind(&self) -> &'static str {
        self.fault.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.callee.clone(),
            self.keyword.clone().unwrap_or_else(|| "-".to_owned()),
            format!("supplied={}", self.supplied),
            self.lambda_list.clone(),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("fault", json!(self.fault.label())),
            ("callee", json!(self.callee)),
            ("keyword", json!(self.keyword)),
            ("supplied", json!(self.supplied)),
            ("lambda_list", json!(self.lambda_list)),
        ]
    }
}

/// A lambda list, reduced to what an arity check needs.
///
/// "Takes keywords" and "accepts these keywords" are separate facts, because
/// `&key &allow-other-keys` is both: the trailing arguments must still pair up,
/// and no individual name can be rejected. One `Option` cannot carry both.
#[derive(Debug, Clone)]
struct Signature {
    required: usize,
    optional: usize,
    rest: bool,
    takes_keywords: bool,
    /// Accepted keyword names, or `None` when `&allow-other-keys` waives the
    /// check.
    keywords: Option<Vec<String>>,
    text: String,
}

#[must_use]
pub fn build_keyword_arity_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<ArityFinding> {
    let source = tree.source();
    let root = tree.root_view();
    let signatures = collect_signatures(&root, dialect, source);

    let mut findings = Vec::new();
    collect_calls(&root, &signatures, source, &mut findings);

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // Lambda-list keywords are Common Lisp's, but the required/optional
        // shape is shared; other dialects simply match fewer callees.
        true,
        findings,
        vec![("checked_callee_count", json!(signatures.len()))],
    )
}

fn collect_signatures(
    root: &ExpressionView,
    dialect: Dialect,
    source: &str,
) -> BTreeMap<String, Signature> {
    let mut signatures = BTreeMap::new();
    for form in &root.children {
        let Some(head) = list_head(form) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, form, head) else {
            continue;
        };
        let Some(name) = shape.name(form) else {
            continue;
        };
        let Some(lambda_list) = shape.lambda_list(form) else {
            continue;
        };
        let Some(parameters) = shape.lambda_parameters(form) else {
            continue;
        };
        let text = source
            .get(lambda_list.span.start().get()..lambda_list.span.end().get())
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        signatures
            .entry(fold(name))
            .or_insert_with(|| read_signature(parameters, text));
    }
    signatures
}

/// Reads a lambda list into the counts and keyword set an arity check needs.
fn read_signature(parameters: &[ExpressionView], text: String) -> Signature {
    #[derive(Clone, Copy, PartialEq)]
    enum Section {
        Required,
        Optional,
        Rest,
        Key,
        Ignored,
    }

    let mut section = Section::Required;
    let mut required = 0usize;
    let mut optional = 0usize;
    let mut rest = false;
    let mut keywords: Vec<String> = Vec::new();
    let mut takes_keywords = false;
    let mut allow_other_keys = false;

    for parameter in parameters {
        let text = atom_symbol_text(parameter)
            .or_else(|| parameter.children.first().and_then(atom_symbol_text))
            .unwrap_or_default();
        if text.starts_with('&') {
            section = match text.to_ascii_lowercase().as_str() {
                "&optional" => Section::Optional,
                "&rest" | "&body" => Section::Rest,
                "&key" => {
                    takes_keywords = true;
                    Section::Key
                }
                "&allow-other-keys" => {
                    allow_other_keys = true;
                    Section::Ignored
                }
                // `&aux` binds locals rather than accepting arguments, so it
                // takes no part in arity.
                _ => Section::Ignored,
            };
            continue;
        }
        match section {
            Section::Required => required += 1,
            Section::Optional => optional += 1,
            Section::Rest => rest = true,
            Section::Key => keywords.push(fold(text)),
            Section::Ignored => {}
        }
    }

    Signature {
        required,
        optional,
        rest,
        takes_keywords,
        keywords: (takes_keywords && !allow_other_keys).then_some(keywords),
        text,
    }
}

fn collect_calls(
    view: &ExpressionView,
    signatures: &BTreeMap<String, Signature>,
    source: &str,
    findings: &mut Vec<ArityFinding>,
) {
    if is_paren_list(view) {
        if let Some(head) = list_head(view) {
            if let Some(signature) = signatures.get(&fold(head)) {
                check(view, head, signature, source, findings);
            }
        }
    }
    for child in &view.children {
        collect_calls(child, signatures, source, findings);
    }
}

fn check(
    call: &ExpressionView,
    head: &str,
    signature: &Signature,
    source: &str,
    findings: &mut Vec<ArityFinding>,
) {
    let arguments = &call.children[1..];
    let supplied = arguments.len();
    let mut push = |fault: ArityFault, keyword: Option<String>| {
        findings.push(ArityFinding {
            fault,
            callee: head.to_owned(),
            keyword,
            supplied,
            lambda_list: signature.text.clone(),
            span: call.span,
            line: line_of(source, call.span.start().get()),
        });
    };

    if supplied < signature.required {
        push(ArityFault::TooFewArguments, None);
        return;
    }

    if signature.takes_keywords {
        let positional = signature.required + signature.optional;
        let tail = &arguments[positional.min(supplied)..];
        if tail.len() % 2 != 0 {
            push(ArityFault::OddKeywordArguments, None);
            return;
        }
        // `None` means `&allow-other-keys`: the pairing above still applies,
        // but no individual name can be rejected.
        if let Some(accepted) = signature.keywords.as_ref() {
            for pair in tail.chunks(2) {
                let Some(text) = pair.first().and_then(atom_symbol_text) else {
                    continue;
                };
                // Only a literal keyword is checked. A computed indicator is
                // not knowable here, and reporting it would be inventing one.
                let Some(name) = text.strip_prefix(':') else {
                    continue;
                };
                if !accepted.iter().any(|known| known == &fold(name)) {
                    push(ArityFault::UnknownKeyword, Some(text.to_owned()));
                }
            }
        }
        return;
    }

    if !signature.rest && supplied > signature.required + signature.optional {
        push(ArityFault::TooManyArguments, None);
    }
}

fn fold(name: &str) -> String {
    name.to_ascii_uppercase()
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

    fn report(source: &str) -> FileFindings<ArityFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_keyword_arity_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn faults(source: &str) -> Vec<ArityFault> {
        report(source)
            .findings
            .iter()
            .map(|finding| finding.fault)
            .collect()
    }

    #[test]
    fn a_correct_positional_call_is_not_reported() {
        assert!(faults("(defun f (a b) (list a b))\n(defun g () (f 1 2))").is_empty());
    }

    #[test]
    fn too_few_positional_arguments_are_reported() {
        assert_eq!(
            faults("(defun f (a b) (list a b))\n(defun g () (f 1))"),
            vec![ArityFault::TooFewArguments]
        );
    }

    #[test]
    fn too_many_positional_arguments_are_reported() {
        assert_eq!(
            faults("(defun f (a) a)\n(defun g () (f 1 2))"),
            vec![ArityFault::TooManyArguments]
        );
    }

    #[test]
    fn an_optional_parameter_widens_the_accepted_range() {
        assert!(faults("(defun f (a &optional b) (list a b))\n(defun g () (f 1 2))").is_empty());
        assert!(faults("(defun f (a &optional b) (list a b))\n(defun g () (f 1))").is_empty());
    }

    #[test]
    fn a_rest_parameter_accepts_any_surplus() {
        assert!(faults("(defun f (a &rest r) (list a r))\n(defun g () (f 1 2 3 4))").is_empty());
    }

    #[test]
    fn a_misspelled_keyword_is_reported_by_name() {
        let report = report("(defun f (&key width) width)\n(defun g () (f :widht 1))");
        assert_eq!(report.findings[0].fault, ArityFault::UnknownKeyword);
        assert_eq!(report.findings[0].keyword.as_deref(), Some(":widht"));
    }

    #[test]
    fn an_accepted_keyword_is_not_reported() {
        assert!(faults("(defun f (&key width) width)\n(defun g () (f :width 1))").is_empty());
    }

    #[test]
    fn a_keyword_with_no_value_is_reported() {
        assert_eq!(
            faults("(defun f (&key width) width)\n(defun g () (f :width))"),
            vec![ArityFault::OddKeywordArguments]
        );
    }

    #[test]
    fn allow_other_keys_accepts_anything() {
        assert!(
            faults("(defun f (&key width &allow-other-keys) width)\n(defun g () (f :anything 1))")
                .is_empty()
        );
    }

    #[test]
    fn a_call_to_something_defined_elsewhere_is_not_checked() {
        assert!(faults("(defun g () (external-call 1 2 3))").is_empty());
    }

    #[test]
    fn the_lambda_list_is_reported_beside_the_fault() {
        let report = report("(defun f (a b) (list a b))\n(defun g () (f 1))");
        assert_eq!(report.findings[0].lambda_list, "(a b)");
    }
}
