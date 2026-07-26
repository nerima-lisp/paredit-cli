//! Naming-convention consistency reporting across definition names.
//!
//! Every supported Lisp dialect shares one naming convention regardless of
//! surrounding host-language influence: hyphen-separated lowercase words,
//! with leading/trailing punctuation reserved for special meanings (`*earmuffs*`
//! for special variables, `+plus+` for constants, trailing `?`/`!`/`p` for
//! predicates and mutators). Names borrowed from `snake_case` or `camelCase`
//! hosts are flagged so agents can catch accidental style drift, especially
//! in generated or ported code.

use std::path::PathBuf;

use anyhow::Result;

use crate::domain::common_lisp::CommonLispPackageDeclarationForm;
use crate::domain::definition::{DefinitionCategory, definition_shape};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SyntaxTree};

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }

    atom_child(view, 0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamingStyle {
    /// Hyphen-separated lowercase words, the idiomatic Lisp convention.
    KebabCase,
    /// Contains an underscore-separated word boundary.
    SnakeCase,
    /// Contains an internal uppercase letter starting a new word.
    CamelCase,
    /// Contains both underscore and internal-uppercase word boundaries.
    Mixed,
}

impl NamingStyle {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::KebabCase => "kebab-case",
            Self::SnakeCase => "snake-case",
            Self::CamelCase => "camel-case",
            Self::Mixed => "mixed",
        }
    }

    /// Whether this style matches the shared cross-dialect Lisp convention.
    #[must_use]
    pub const fn is_idiomatic(self) -> bool {
        matches!(self, Self::KebabCase)
    }
}

/// Strips conventional Lisp naming punctuation (`*earmuffs*`, `+constants+`,
/// trailing `?`/`!`/`-p`) before classifying the remaining word boundaries, so
/// those markers are never mistaken for style violations.
fn core_name(name: &str) -> &str {
    let trimmed = name.trim_matches(|c: char| matches!(c, '*' | '+' | '?' | '!' | '%'));
    trimmed.strip_suffix("-p").unwrap_or(trimmed)
}

#[must_use]
pub fn naming_style(name: &str) -> NamingStyle {
    let core = core_name(name);
    if core.is_empty() || core.chars().all(|c| !c.is_ascii_alphabetic()) {
        return NamingStyle::KebabCase;
    }
    // All-uppercase names (`ALIST`, CL constant conventions) are idiomatic
    // and never flagged; only internal *lowercase-to-uppercase* transitions
    // indicate a borrowed camelCase/PascalCase convention.
    if core.chars().all(|c| !c.is_ascii_lowercase()) {
        return NamingStyle::KebabCase;
    }

    let has_underscore = core.contains('_');
    let has_internal_upper = core
        .as_bytes()
        .windows(2)
        .any(|pair| pair[0].is_ascii_lowercase() && pair[1].is_ascii_uppercase());

    match (has_underscore, has_internal_upper) {
        (true, true) => NamingStyle::Mixed,
        (true, false) => NamingStyle::SnakeCase,
        (false, true) => NamingStyle::CamelCase,
        (false, false) => NamingStyle::KebabCase,
    }
}

#[derive(Debug, Clone)]
pub struct NamingReportItem {
    pub path: String,
    pub span: ByteSpan,
    pub head: String,
    pub name: String,
    pub category: DefinitionCategory,
    pub style: NamingStyle,
}

#[derive(Debug)]
pub struct NamingReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    /// Every named definition, in source order.
    pub definitions: Vec<NamingReportItem>,
}

impl NamingReportFile {
    pub fn non_idiomatic(&self) -> impl Iterator<Item = &NamingReportItem> {
        self.definitions
            .iter()
            .filter(|item| !item.style.is_idiomatic())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NamingReportPolicyOptions {
    fail_on_non_idiomatic: bool,
}

impl NamingReportPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_non_idiomatic: bool) -> Self {
        Self {
            fail_on_non_idiomatic,
        }
    }

    #[must_use]
    pub const fn fail_on_non_idiomatic(self) -> bool {
        self.fail_on_non_idiomatic
    }
}

#[derive(Debug)]
pub struct NamingReportPolicy {
    pub fail_on_non_idiomatic: bool,
    pub named_definition_count: usize,
    pub non_idiomatic_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn build_naming_report(
    path: PathBuf,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<NamingReportFile> {
    let mut definitions = Vec::new();

    for index in 0..tree.root_children().len() {
        let form_path = Path::root_child(index);
        let view = tree.select_path(&form_path)?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        if dialect.common_lisp_package_declaration_form_for_head(head)
            == Some(CommonLispPackageDeclarationForm::InPackage)
        {
            continue;
        }
        let Some(shape) = definition_shape(dialect, &view, head) else {
            continue;
        };
        let Some(name) = shape.name(&view) else {
            continue;
        };

        definitions.push(NamingReportItem {
            path: form_path.to_string(),
            span: view.span,
            head: head.to_owned(),
            name: name.to_owned(),
            category: shape.category,
            style: naming_style(name),
        });
    }

    Ok(NamingReportFile {
        path,
        dialect,
        definitions,
    })
}

#[must_use]
pub fn evaluate_naming_report_policy(
    options: NamingReportPolicyOptions,
    reports: &[NamingReportFile],
) -> NamingReportPolicy {
    let named_definition_count = reports
        .iter()
        .map(|report| report.definitions.len())
        .sum::<usize>();
    let non_idiomatic_count = reports
        .iter()
        .map(|report| report.non_idiomatic().count())
        .sum::<usize>();

    let mut violations = Vec::new();
    if options.fail_on_non_idiomatic() && non_idiomatic_count > 0 {
        violations.push(format!(
            "non_idiomatic_count {non_idiomatic_count} exceeds 0"
        ));
    }

    NamingReportPolicy {
        fail_on_non_idiomatic: options.fail_on_non_idiomatic(),
        named_definition_count,
        non_idiomatic_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_kebab_case_as_idiomatic() {
        assert_eq!(naming_style("render-pane"), NamingStyle::KebabCase);
        assert_eq!(naming_style("area"), NamingStyle::KebabCase);
        assert!(naming_style("render-pane").is_idiomatic());
    }

    #[test]
    fn classifies_earmuffs_and_predicates_as_idiomatic() {
        assert_eq!(naming_style("*default-width*"), NamingStyle::KebabCase);
        assert_eq!(naming_style("+max-width+"), NamingStyle::KebabCase);
        assert_eq!(naming_style("empty?"), NamingStyle::KebabCase);
        assert_eq!(naming_style("empty-p"), NamingStyle::KebabCase);
        assert_eq!(naming_style("reset!"), NamingStyle::KebabCase);
        assert_eq!(naming_style("ALIST"), NamingStyle::KebabCase);
    }

    #[test]
    fn classifies_snake_case_as_non_idiomatic() {
        let style = naming_style("render_pane");
        assert_eq!(style, NamingStyle::SnakeCase);
        assert!(!style.is_idiomatic());
    }

    #[test]
    fn classifies_camel_case_as_non_idiomatic() {
        let style = naming_style("renderPane");
        assert_eq!(style, NamingStyle::CamelCase);
        assert!(!style.is_idiomatic());
    }

    #[test]
    fn classifies_pascal_case_as_camel_case() {
        assert_eq!(naming_style("RenderPane"), NamingStyle::CamelCase);
    }

    #[test]
    fn classifies_underscore_after_capitalized_word_as_snake_case() {
        // The uppercase letter follows an underscore, a legitimate word
        // boundary, not a lowercase letter, so this is not a camelCase hump.
        assert_eq!(naming_style("render_Pane"), NamingStyle::SnakeCase);
    }

    #[test]
    fn classifies_underscore_plus_internal_hump_as_mixed() {
        // "renderPane" has a genuine lowercase-to-uppercase hump, and
        // "_width" adds an underscore boundary, so both style markers fire.
        assert_eq!(naming_style("renderPane_width"), NamingStyle::Mixed);
    }

    #[test]
    fn builds_report_and_flags_only_non_idiomatic_names() {
        let tree = SyntaxTree::parse(
            "(defun render-pane (x) x)\n(defun render_pane (x) x)\n(defun renderPane (x) x)",
        )
        .expect("parse input");
        let report = build_naming_report(PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build naming report");

        assert_eq!(report.definitions.len(), 3);
        let flagged = report.non_idiomatic().collect::<Vec<_>>();
        assert_eq!(flagged.len(), 2);
        assert_eq!(flagged[0].name, "render_pane");
        assert_eq!(flagged[1].name, "renderPane");
    }

    #[test]
    fn ignores_in_package_and_non_definition_forms() {
        let tree =
            SyntaxTree::parse("(in-package :foo)\n(defun f (x) x)\n(+ 1 2)").expect("parse input");
        let report = build_naming_report(PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build naming report");

        assert_eq!(report.definitions.len(), 1);
        assert_eq!(report.definitions[0].name, "f");
    }

    #[test]
    fn policy_passes_without_flag() {
        let tree = SyntaxTree::parse("(defun render_pane (x) x)").expect("parse input");
        let report = build_naming_report(PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build naming report");
        let policy = evaluate_naming_report_policy(
            NamingReportPolicyOptions::new(false),
            std::slice::from_ref(&report),
        );

        assert!(policy.passed);
        assert_eq!(policy.non_idiomatic_count, 1);
    }

    #[test]
    fn policy_fails_when_flag_set_and_violations_found() {
        let tree = SyntaxTree::parse("(defun render_pane (x) x)").expect("parse input");
        let report = build_naming_report(PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build naming report");
        let policy = evaluate_naming_report_policy(
            NamingReportPolicyOptions::new(true),
            std::slice::from_ref(&report),
        );

        assert!(!policy.passed);
        assert_eq!(policy.violations.len(), 1);
    }

    #[test]
    fn policy_passes_when_flag_set_and_no_violations() {
        let tree = SyntaxTree::parse("(defun render-pane (x) x)").expect("parse input");
        let report = build_naming_report(PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build naming report");
        let policy = evaluate_naming_report_policy(
            NamingReportPolicyOptions::new(true),
            std::slice::from_ref(&report),
        );

        assert!(policy.passed);
    }
}
