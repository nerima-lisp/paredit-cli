//! The per-file Emacs Lisp facts that live outside the syntax tree.
//!
//! Three of the four things an agent needs to know before editing a `.el` file
//! are not forms:
//!
//! * whether `let` binds lexically, which a comment on line 1 decides;
//! * which functions are autoloaded, which `;;;###autoload` comments decide;
//! * what the file provides and requires, which *are* forms but are scattered
//!   through it.
//!
//! Reading them one at a time costs an agent three passes and three commands.
//! This report is the single answer, and it is deliberately the *whole* answer
//! for one file rather than a graph across many: the load-order questions a
//! graph answers already have `inspect dependencies`.

use std::path::{Path, PathBuf};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::emacs_lisp::{
    EmacsLispAutoloadPayload, EmacsLispDependencyForm, EmacsLispLexicalBinding, EmacsLispOperator,
    emacs_lisp_autoload_cookies, emacs_lisp_file_header,
};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};

/// How a file answers the lexical-binding question, in report form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexicalBindingStatus {
    Enabled,
    DisabledExplicitly,
    Absent,
}

impl LexicalBindingStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::DisabledExplicitly => "disabled-explicitly",
            Self::Absent => "absent",
        }
    }

    const fn of(binding: EmacsLispLexicalBinding) -> Self {
        match binding {
            EmacsLispLexicalBinding::Enabled => Self::Enabled,
            EmacsLispLexicalBinding::DisabledExplicitly => Self::DisabledExplicitly,
            EmacsLispLexicalBinding::Absent => Self::Absent,
        }
    }
}

/// One `require`/`provide`/`load`/`autoload` form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureReference {
    /// The form's head, as written.
    pub form: String,
    /// The feature or file it names, with the `'`, `:`, or `"` stripped.
    pub designator: String,
    /// Whether the named library is loaded when this file is loaded.
    pub eager: bool,
    pub span: ByteSpan,
}

/// One `;;;###autoload` cookie and what it attaches to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoloadEntry {
    /// The definition the cookie autoloads, when it precedes a named
    /// top-level definition. `None` for a cookie carrying its own form, or one
    /// that attaches to nothing.
    pub definition: Option<String>,
    /// The non-standard cookie prefix, empty for `;;;###autoload`.
    pub prefix: String,
    /// Whether the cookie carries a form on its own line rather than
    /// autoloading the next one.
    pub inline_form: bool,
    pub span: ByteSpan,
}

/// Everything this report knows about one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmacsLispFileFacts {
    pub path: PathBuf,
    pub lexical_binding: LexicalBindingStatus,
    /// The feature the file provides, when it has a `(provide 'FEATURE)`.
    pub provides: Option<String>,
    pub features: Vec<FeatureReference>,
    pub autoloads: Vec<AutoloadEntry>,
    /// Top-level definitions, for the "how much of this file is autoloaded"
    /// ratio the summary reports.
    pub definition_count: usize,
}

/// The policy a caller can gate on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmacsLispFilePolicyOptions {
    pub fail_on_missing_lexical_binding: bool,
}

impl EmacsLispFilePolicyOptions {
    #[must_use]
    pub const fn new(fail_on_missing_lexical_binding: bool) -> Self {
        Self {
            fail_on_missing_lexical_binding,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmacsLispFilePolicy {
    pub fail_on_missing_lexical_binding: bool,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Reads one parsed file.
///
/// `source` is required and not incidental: the lexical-binding header and the
/// autoload cookies are both comments, so neither is reachable from `tree`
/// alone.
#[must_use]
pub fn collect_emacs_lisp_file_facts(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    source: &str,
) -> EmacsLispFileFacts {
    let mut facts = EmacsLispFileFacts {
        path: PathBuf::from(path),
        lexical_binding: LexicalBindingStatus::Absent,
        provides: None,
        features: Vec::new(),
        autoloads: Vec::new(),
        definition_count: 0,
    };

    // Every fact below is an Emacs Lisp one. Reporting them for a `.lisp`
    // file would invent a `lexical-binding` answer for a dialect that has no
    // such concept.
    if dialect != Dialect::EmacsLisp {
        return facts;
    }

    facts.lexical_binding =
        LexicalBindingStatus::of(emacs_lisp_file_header(source).lexical_binding());

    let document = tree.root_view();
    for form in &document.children {
        collect_form(form, &mut facts);
    }

    facts.autoloads = collect_autoloads(tree, &document);
    facts
}

fn collect_form(form: &ExpressionView, facts: &mut EmacsLispFileFacts) {
    let Some(head) = head_text(form) else {
        return;
    };
    let Some(operator) = EmacsLispOperator::from_head(head) else {
        return;
    };

    if operator.is_definition() {
        facts.definition_count += 1;
    }

    let Some(dependency) = operator.dependency_form() else {
        return;
    };
    let Some(designator) = form
        .children
        .get(dependency.designator_child_index())
        .and_then(designator_text)
    else {
        return;
    };

    if dependency == EmacsLispDependencyForm::Provide {
        facts.provides = Some(designator.clone());
    }

    facts.features.push(FeatureReference {
        form: head.to_owned(),
        designator,
        eager: dependency.loads_eagerly(),
        span: form.span,
    });
}

/// Pairs each cookie with the top-level definition that follows it.
fn collect_autoloads(tree: &SyntaxTree, document: &ExpressionView) -> Vec<AutoloadEntry> {
    emacs_lisp_autoload_cookies(tree)
        .into_iter()
        .map(|cookie| {
            let inline_form = cookie.payload() == EmacsLispAutoloadPayload::InlineForm;
            AutoloadEntry {
                definition: (!inline_form)
                    .then(|| following_definition_name(document, cookie.span()))
                    .flatten(),
                prefix: cookie.prefix().to_owned(),
                inline_form,
                span: cookie.span(),
            }
        })
        .collect()
}

/// The name of the first top-level definition starting after `cookie`.
///
/// Top-level only, because that is all `loaddefs` extracts: a cookie in front
/// of a definition nested in a `progn` autoloads nothing, and reporting the
/// nested name would say otherwise.
fn following_definition_name(document: &ExpressionView, cookie: ByteSpan) -> Option<String> {
    document
        .children
        .iter()
        .filter(|form| form.span.start().get() >= cookie.end().get())
        .min_by_key(|form| form.span.start().get())
        .and_then(|form| {
            let head = head_text(form)?;
            EmacsLispOperator::from_head(head)
                .filter(|operator| operator.is_definition())
                .and_then(|_| form.children.get(1))
                .and_then(head_text)
                .map(str::to_owned)
        })
}

/// A `require`/`provide` argument with its reader syntax stripped.
///
/// The three spellings `'feature`, `"file"`, and bare `feature` all name the
/// same thing, and a report that kept them apart would make `subr-x` and
/// `'subr-x` two different dependencies.
fn designator_text(view: &ExpressionView) -> Option<String> {
    let text = view.text.as_deref()?;
    let text = text
        .strip_prefix('\'')
        .or_else(|| text.strip_prefix(':'))
        .unwrap_or(text);
    let text = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text);
    (!text.is_empty()).then(|| text.to_owned())
}

fn head_text(view: &ExpressionView) -> Option<&str> {
    match view.kind {
        ExpressionKind::List => view.children.first().and_then(head_text),
        ExpressionKind::Atom => view
            .reader_prefixes
            .is_empty()
            .then_some(view.text.as_deref())
            .flatten(),
        ExpressionKind::Root => None,
    }
}

/// Evaluates the gate over every file's facts.
#[must_use]
pub fn evaluate_emacs_lisp_file_policy(
    options: EmacsLispFilePolicyOptions,
    facts: &[EmacsLispFileFacts],
) -> EmacsLispFilePolicy {
    let violations: Vec<String> = if options.fail_on_missing_lexical_binding {
        facts
            .iter()
            .filter(|file| file.lexical_binding == LexicalBindingStatus::Absent)
            .map(|file| {
                format!(
                    "{} has no lexical-binding setting on its first line",
                    file.path.display()
                )
            })
            .collect()
    } else {
        Vec::new()
    };

    EmacsLispFilePolicy {
        fail_on_missing_lexical_binding: options.fail_on_missing_lexical_binding,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts_of(source: &str) -> EmacsLispFileFacts {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::EmacsLisp).expect("parse");
        collect_emacs_lisp_file_facts(Path::new("f.el"), Dialect::EmacsLisp, &tree, source)
    }

    #[test]
    fn reads_the_lexical_binding_header() {
        assert_eq!(
            facts_of(";;; f.el -*- lexical-binding: t -*-\n").lexical_binding,
            LexicalBindingStatus::Enabled
        );
        assert_eq!(
            facts_of(";;; f.el -*- lexical-binding: nil -*-\n").lexical_binding,
            LexicalBindingStatus::DisabledExplicitly
        );
        assert_eq!(
            facts_of(";;; f.el --- x\n").lexical_binding,
            LexicalBindingStatus::Absent
        );
    }

    #[test]
    fn collects_the_provided_feature_and_the_required_ones() {
        let facts = facts_of("(require 'subr-x)\n(require \"cl-lib\")\n(provide 'mine)\n");

        assert_eq!(facts.provides.as_deref(), Some("mine"));
        let designators: Vec<_> = facts
            .features
            .iter()
            .map(|feature| feature.designator.as_str())
            .collect();
        // The three spellings a designator can take all normalize to the same
        // name, so `'subr-x` and `subr-x` are one dependency.
        assert_eq!(designators, ["subr-x", "cl-lib", "mine"]);
    }

    #[test]
    fn separates_eager_loads_from_deferred_ones() {
        let facts = facts_of("(require 'a)\n(autoload 'f \"b\")\n(declare-function g \"c\")\n");
        let eager: Vec<_> = facts
            .features
            .iter()
            .map(|feature| (feature.form.as_str(), feature.eager))
            .collect();

        assert_eq!(
            eager,
            [
                ("require", true),
                ("autoload", false),
                ("declare-function", false)
            ]
        );
    }

    #[test]
    fn pairs_a_cookie_with_the_definition_that_follows_it() {
        let facts = facts_of(";;;###autoload\n(defun my-command () nil)\n");

        assert_eq!(facts.autoloads.len(), 1);
        assert_eq!(facts.autoloads[0].definition.as_deref(), Some("my-command"));
        assert!(!facts.autoloads[0].inline_form);
    }

    #[test]
    fn a_cookie_on_a_nested_definition_names_no_definition() {
        // `loaddefs` extracts top-level forms only, so nothing here is
        // autoloaded and reporting the nested name would say otherwise.
        let facts = facts_of("(progn\n  ;;;###autoload\n  (defun nested () nil))\n");

        assert_eq!(facts.autoloads.len(), 1);
        assert_eq!(facts.autoloads[0].definition, None);
    }

    #[test]
    fn a_cookie_carrying_its_own_form_is_marked_rather_than_paired() {
        let facts = facts_of(";;;###autoload (autoload 'f \"lib\")\n(defun g () nil)\n");

        assert_eq!(facts.autoloads.len(), 1);
        assert!(facts.autoloads[0].inline_form);
        // `g` is *not* autoloaded: the inline form replaced the next-form
        // behaviour rather than adding to it.
        assert_eq!(facts.autoloads[0].definition, None);
    }

    #[test]
    fn counts_top_level_definitions() {
        let facts = facts_of("(defun f () nil)\n(defvar v nil)\n(defcustom c nil \"D.\")\n");
        assert_eq!(facts.definition_count, 3);
    }

    #[test]
    fn another_dialect_gets_no_invented_answers() {
        let source = "(defun f () nil)\n";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let facts =
            collect_emacs_lisp_file_facts(Path::new("f.lisp"), Dialect::CommonLisp, &tree, source);

        assert_eq!(facts.lexical_binding, LexicalBindingStatus::Absent);
        assert_eq!(facts.definition_count, 0);
        assert!(facts.features.is_empty());
    }

    #[test]
    fn the_gate_names_every_file_without_a_header() {
        let facts = vec![
            facts_of(";;; a.el -*- lexical-binding: t -*-\n"),
            facts_of(";;; b.el --- x\n"),
        ];
        let policy = evaluate_emacs_lisp_file_policy(EmacsLispFilePolicyOptions::new(true), &facts);

        assert!(!policy.passed);
        assert_eq!(policy.violations.len(), 1);
    }

    #[test]
    fn the_gate_is_silent_when_it_is_off() {
        let facts = vec![facts_of(";;; b.el --- x\n")];
        let policy =
            evaluate_emacs_lisp_file_policy(EmacsLispFilePolicyOptions::new(false), &facts);

        assert!(policy.passed);
        assert!(policy.violations.is_empty());
    }
}
