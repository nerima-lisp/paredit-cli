use std::collections::HashSet;
use std::path::PathBuf;
use std::thread;

use crate::error::{AnalysisWorkerError, RemoveUnusedError, RemoveUnusedResult};

use paredit_core_semantics::definition_reference::{
    collect_package_form_spans, collect_reference_needles, collect_symbol_references,
};
use paredit_core_syntax::common_lisp::CommonLispPackageDeclarationForm;
use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    AtomOccurrence, ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SymbolName,
    SyntaxTree,
};

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

#[derive(Debug, Clone, Copy)]
pub struct UnusedDefinitionPolicyOptions {
    fail_on_unused: bool,
    require_unused_definitions: Option<usize>,
}

impl UnusedDefinitionPolicyOptions {
    pub fn new(
        fail_on_unused: bool,
        require_unused_definitions: Option<usize>,
    ) -> Result<Self, String> {
        if matches!(require_unused_definitions, Some(0)) {
            return Err("require-unused-definitions must be greater than zero".to_string());
        }

        Ok(Self {
            fail_on_unused,
            require_unused_definitions,
        })
    }

    #[must_use]
    pub const fn fail_on_unused(self) -> bool {
        self.fail_on_unused
    }

    #[must_use]
    pub const fn require_unused_definitions(self) -> Option<usize> {
        self.require_unused_definitions
    }
}

#[derive(Debug, Clone)]
pub struct DefinitionReportItem {
    pub path: String,
    pub span: ByteSpan,
    pub head: String,
    pub name: Option<String>,
    pub category: DefinitionCategory,
    pub parameter_count: Option<usize>,
    pub body_form_count: Option<usize>,
    pub package: Option<String>,
}

#[derive(Debug)]
pub struct DefinitionReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub package: Option<String>,
    pub definitions: Vec<DefinitionReportItem>,
}

#[derive(Debug)]
pub struct ParsedDefinitionFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub package: Option<String>,
    pub definitions: Vec<DefinitionReportItem>,
    pub atoms: Vec<AtomOccurrence>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct DefinitionReference {
    pub file_index: usize,
    pub path: String,
    pub span: ByteSpan,
}

#[derive(Debug)]
pub struct UnusedDefinitionItem {
    pub definition: DefinitionReportItem,
    pub references: Vec<DefinitionReference>,
}

#[derive(Debug)]
pub struct UnusedDefinitionFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub package: Option<String>,
    pub definitions: Vec<UnusedDefinitionItem>,
}

#[derive(Debug)]
pub struct UnusedDefinitionPolicy {
    pub fail_on_unused: bool,
    pub require_unused_definitions: Option<usize>,
    pub definition_count: usize,
    pub candidate_count: usize,
    pub actionable_candidate_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn build_definition_report(
    path: PathBuf,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> RemoveUnusedResult<DefinitionReportFile> {
    let (package, definitions) = collect_definition_forms(tree, dialect)?;

    Ok(DefinitionReportFile {
        path,
        dialect,
        package,
        definitions,
    })
}

pub fn build_parsed_definition_file(
    path: PathBuf,
    dialect: Dialect,
    tree: &SyntaxTree,
    text: &str,
) -> RemoveUnusedResult<ParsedDefinitionFile> {
    let (package, definitions) = collect_definition_forms(tree, dialect)?;

    Ok(ParsedDefinitionFile {
        path,
        dialect,
        package,
        definitions,
        atoms: tree.atom_occurrences(),
        text: text.to_owned(),
    })
}

pub fn collect_unused_definition_candidates(
    files: &[ParsedDefinitionFile],
) -> RemoveUnusedResult<Vec<UnusedDefinitionFile>> {
    for file in files {
        if file.dialect == Dialect::Unknown {
            return Err(RemoveUnusedError::UnsupportedDialect {
                operation: "unused-definition analysis",
                dialect: format!("unknown: {}", file.path.display()),
            });
        }
    }

    let views: Vec<Option<ExpressionView>> = files
        .iter()
        .map(|file| {
            SyntaxTree::parse_with_dialect(&file.text, file.dialect)
                .map_err(|source| RemoveUnusedError::ParseFailed {
                    path: file.path.display().to_string(),
                    source,
                })
                .map(|tree| Some(tree.root_view()))
        })
        .collect::<RemoveUnusedResult<_>>()?;

    let package_form_spans: Vec<Vec<ByteSpan>> = files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let mut spans = Vec::new();
            if let Some(view) = &views[index] {
                collect_package_form_spans(file.dialect, view, &mut spans);
            }
            spans
        })
        .collect();
    let atom_needles: Vec<HashSet<String>> = views
        .iter()
        .map(|view| {
            let mut needles = HashSet::new();
            if let Some(view) = view {
                collect_reference_needles(view, &mut needles);
            }
            needles
        })
        .collect();

    // `--jobs` governs this fan-out.
    let worker_count = paredit_core_safety::limits::effective_jobs_or_available()
        .get()
        .clamp(1, files.len().max(1));
    let mut ordered: Vec<Option<UnusedDefinitionFile>> = (0..files.len()).map(|_| None).collect();
    thread::scope(|scope| -> RemoveUnusedResult<()> {
        let views = &views;
        let package_form_spans = &package_form_spans;
        let atom_needles = &atom_needles;
        let handles: Vec<_> = (0..worker_count)
            .map(|worker| {
                scope.spawn(move || {
                    files
                        .iter()
                        .enumerate()
                        .skip(worker)
                        .step_by(worker_count)
                        .map(|(file_index, file)| {
                            (
                                file_index,
                                file_unused_definition_report(
                                    files,
                                    views,
                                    package_form_spans,
                                    atom_needles,
                                    file_index,
                                    file,
                                ),
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            for (file_index, report) in handle
                .join()
                .map_err(|_| AnalysisWorkerError::ReferenceWorkerPanicked)?
            {
                ordered[file_index] = Some(report);
            }
        }
        Ok(())
    })?;
    Ok(ordered.into_iter().flatten().collect())
}

fn file_unused_definition_report(
    files: &[ParsedDefinitionFile],
    views: &[Option<ExpressionView>],
    package_form_spans: &[Vec<ByteSpan>],
    atom_needles: &[HashSet<String>],
    file_index: usize,
    file: &ParsedDefinitionFile,
) -> UnusedDefinitionFile {
    UnusedDefinitionFile {
        path: file.path.clone(),
        dialect: file.dialect,
        package: file.package.clone(),
        definitions: file
            .definitions
            .iter()
            .filter_map(|definition| {
                let name = definition.name.as_ref()?;
                let symbol = SymbolName::new(name.clone()).ok()?;
                let needle = paredit_core_syntax::common_lisp::common_lisp_symbol_reference_needle(
                    symbol.as_str(),
                );
                let references = files
                    .iter()
                    .enumerate()
                    .flat_map(|(other_index, other)| {
                        let mut spans = Vec::new();
                        if let Some(view) = views[other_index]
                            .as_ref()
                            .filter(|_| atom_needles[other_index].contains(&needle))
                        {
                            collect_symbol_references(
                                other.dialect,
                                view,
                                &symbol,
                                &other.text,
                                &mut spans,
                            );
                        }

                        let other_package_spans = &package_form_spans[other_index];
                        spans.retain(|span| {
                            !other_package_spans
                                .iter()
                                .any(|package| package.contains_span(*span))
                        });
                        spans
                            .into_iter()
                            .filter(move |span| {
                                !(other_index == file_index && definition.span.contains_span(*span))
                            })
                            .map(move |span| DefinitionReference {
                                file_index: other_index,
                                path: String::new(),
                                span,
                            })
                    })
                    .collect();

                Some(UnusedDefinitionItem {
                    definition: definition.clone(),
                    references,
                })
            })
            .collect(),
    }
}

pub fn collect_definition_forms(
    tree: &SyntaxTree,
    dialect: Dialect,
) -> RemoveUnusedResult<(Option<String>, Vec<DefinitionReportItem>)> {
    let mut current_package = None;
    let mut definitions = Vec::new();

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };

        if dialect.common_lisp_package_declaration_form_for_head(head)
            == Some(CommonLispPackageDeclarationForm::InPackage)
        {
            if let Some(package_name) = atom_child(&view, 1) {
                current_package = Some(package_name.to_owned());
            }
            continue;
        }

        let Some(shape) = definition_shape(dialect, &view, head) else {
            continue;
        };

        definitions.push(DefinitionReportItem {
            path: path.to_string(),
            span: view.span,
            head: head.to_owned(),
            name: shape.name(&view).map(ToOwned::to_owned),
            category: shape.category,
            parameter_count: shape.lambda_parameter_count(&view),
            body_form_count: Some(shape.body_form_count(&view)),
            package: current_package.clone(),
        });
    }

    Ok((current_package, definitions))
}

#[must_use]
pub fn unused_definition_candidate_count(reports: &[UnusedDefinitionFile]) -> usize {
    reports
        .iter()
        .flat_map(|report| &report.definitions)
        .filter(|item| item.references.is_empty())
        .count()
}

#[must_use]
pub fn unused_definition_actionable_candidate_count(reports: &[UnusedDefinitionFile]) -> usize {
    reports
        .iter()
        .flat_map(|report| &report.definitions)
        .filter(|item| item.references.is_empty() && item.definition.category.is_bulk_removable())
        .count()
}

#[must_use]
pub fn evaluate_unused_definition_policy(
    options: UnusedDefinitionPolicyOptions,
    reports: &[UnusedDefinitionFile],
) -> UnusedDefinitionPolicy {
    let definition_count = reports
        .iter()
        .map(|report| report.definitions.len())
        .sum::<usize>();
    let candidate_count = unused_definition_candidate_count(reports);
    let actionable_candidate_count = unused_definition_actionable_candidate_count(reports);
    let mut violations = Vec::new();

    if options.fail_on_unused() && actionable_candidate_count > 0 {
        violations.push(format!(
            "actionable_candidate_count {actionable_candidate_count} exceeds 0"
        ));
    }
    if let Some(required) = options.require_unused_definitions() {
        if actionable_candidate_count < required {
            violations.push(format!(
                "actionable_candidate_count {actionable_candidate_count} is below required {required}"
            ));
        }
    }

    UnusedDefinitionPolicy {
        fail_on_unused: options.fail_on_unused(),
        require_unused_definitions: options.require_unused_definitions(),
        definition_count,
        candidate_count,
        actionable_candidate_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_file(path: &str, dialect: Dialect, text: &str) -> ParsedDefinitionFile {
        let tree = SyntaxTree::parse_with_dialect(text, dialect).expect("fixture must parse");
        build_parsed_definition_file(PathBuf::from(path), dialect, &tree, text)
            .expect("fixture report must build")
    }

    #[test]
    fn validates_unused_definition_threshold() {
        assert!(UnusedDefinitionPolicyOptions::new(true, Some(1)).is_ok());
        assert_eq!(
            UnusedDefinitionPolicyOptions::new(false, Some(0)).unwrap_err(),
            "require-unused-definitions must be greater than zero"
        );
    }

    #[test]
    fn collects_unused_definitions_for_every_known_dialect() {
        let fixtures = [
            (
                "common-lisp.lisp",
                Dialect::CommonLisp,
                "(defun cl-unused () 1)\n",
            ),
            (
                "emacs-lisp.el",
                Dialect::EmacsLisp,
                "(defun el-unused () 1)\n",
            ),
            (
                "scheme.scm",
                Dialect::Scheme,
                "(define scheme-unused (lambda () 1))\n",
            ),
            ("clojure.clj", Dialect::Clojure, "(defn clj-unused [] 1)\n"),
            ("janet.janet", Dialect::Janet, "(defn janet-unused [] 1)\n"),
            ("fennel.fnl", Dialect::Fennel, "(fn fennel-unused [] 1)\n"),
        ];
        let files: Vec<_> = fixtures
            .iter()
            .map(|(path, dialect, text)| parsed_file(path, *dialect, text))
            .collect();

        let reports = collect_unused_definition_candidates(&files).expect("report must build");

        assert_eq!(reports.len(), fixtures.len());
        assert_eq!(unused_definition_candidate_count(&reports), fixtures.len());
        for ((_, dialect, _), report) in fixtures.iter().zip(&reports) {
            assert_eq!(report.dialect, *dialect);
            assert_eq!(report.definitions.len(), 1);
            assert!(report.definitions[0].references.is_empty());
        }
    }

    #[test]
    fn rejects_unknown_dialect_before_parsing_any_file() {
        let files = vec![
            ParsedDefinitionFile {
                path: PathBuf::from("broken.lisp"),
                dialect: Dialect::CommonLisp,
                package: None,
                definitions: Vec::new(),
                atoms: Vec::new(),
                text: "(defun broken ()".to_owned(),
            },
            ParsedDefinitionFile {
                path: PathBuf::from("unknown.lisp"),
                dialect: Dialect::Unknown,
                package: None,
                definitions: Vec::new(),
                atoms: Vec::new(),
                text: "()".to_owned(),
            },
        ];

        let error = collect_unused_definition_candidates(&files).expect_err("input must fail");

        assert_eq!(
            error.to_string(),
            "unused-definition analysis does not support dialect unknown: unknown.lisp"
        );
    }

    #[test]
    fn propagates_malformed_input_errors() {
        let files = vec![ParsedDefinitionFile {
            path: PathBuf::from("broken.lisp"),
            dialect: Dialect::CommonLisp,
            package: None,
            definitions: Vec::new(),
            atoms: Vec::new(),
            text: "(defun broken ()".to_owned(),
        }];

        let error = collect_unused_definition_candidates(&files).expect_err("input must fail");

        assert_eq!(error.to_string(), "failed to parse broken.lisp");
    }

    #[test]
    fn parses_reader_syntax_with_the_input_dialect() {
        let text = "(defun el-reader () [?\\)])\n";
        assert!(SyntaxTree::parse_with_dialect(text, Dialect::EmacsLisp).is_ok());
        // `[`, `]` and `?` are Common Lisp constituent characters and `\)` a
        // single-escaped `)`, so this also parses as Common Lisp -- just as a
        // symbol-heavy form rather than a character literal in a vector. The
        // point of this test is that removal reads `text` with the *input*
        // dialect (Emacs Lisp) rather than either reader's opinion of it.
        assert!(SyntaxTree::parse_with_dialect(text, Dialect::CommonLisp).is_ok());
        let files = vec![parsed_file("reader.el", Dialect::EmacsLisp, text)];

        let reports = collect_unused_definition_candidates(&files).expect("report must build");

        assert_eq!(unused_definition_candidate_count(&reports), 1);
    }

    #[test]
    fn common_lisp_symbol_matching_is_package_aware_but_scheme_is_exact() {
        let common_lisp = vec![parsed_file(
            "symbols.lisp",
            Dialect::CommonLisp,
            "(defun Foo () 1)\n(pkg:foo)\n",
        )];
        let scheme = vec![parsed_file(
            "symbols.scm",
            Dialect::Scheme,
            "(define Foo (lambda () 1))\n(pkg:foo)\n",
        )];

        let common_lisp_report =
            collect_unused_definition_candidates(&common_lisp).expect("report must build");
        let scheme_report =
            collect_unused_definition_candidates(&scheme).expect("report must build");

        assert_eq!(unused_definition_candidate_count(&common_lisp_report), 0);
        assert_eq!(unused_definition_candidate_count(&scheme_report), 1);
    }
}
