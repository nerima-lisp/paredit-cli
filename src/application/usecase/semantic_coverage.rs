//! Measuring how much of the semantic layer resolves on real source.
//!
//! `src/domain/semantics` is conservative by design — it says `Known` only
//! when it can prove something, and stays silent otherwise. That discipline
//! is only worth its cost if it actually resolves a useful fraction of real
//! code, and *why* it does not resolve the rest is what decides where the
//! next round of work should go. This use case answers both questions: it
//! builds the binding and value tables for each file and counts, rather than
//! asserts, what they found.
//!
//! Discovery is a source-port responsibility, mirroring
//! [`crate::application::usecase::similarity_report`]: this module only knows
//! how to turn bytes into a report, not how paths become files.

use std::path::{Path, PathBuf};

use crate::domain::dialect::Dialect;
use crate::domain::semantics::binding::{Binding, BindingKind, build_binding_table};
use crate::domain::semantics::value::{build_value_table, evaluate_constant};
use crate::domain::sexpr::{ExpressionKind, SyntaxTree};
use crate::domain::view_query::for_each_subview;

/// The files to measure. Discovery (walking directories, filtering
/// extensions) happens behind [`SemanticCoverageSourcePort`]; this only names
/// the roots the caller asked about.
#[derive(Debug, Clone)]
pub struct SemanticCoverageRequest {
    pub paths: Vec<PathBuf>,
}

/// One file the source discovered, ready to be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSemanticCoverageFile {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticCoverageInventory {
    pub files: Vec<DiscoveredSemanticCoverageFile>,
}

/// Turns a request into loadable files, and files into bytes.
///
/// Only Common Lisp is worth measuring here: `build_binding_table` and
/// `build_value_table` return an empty table for every other dialect, so a
/// source that discovered other dialects would only measure zeroes.
pub trait SemanticCoverageSourcePort {
    fn discover(
        &mut self,
        request: &SemanticCoverageRequest,
    ) -> anyhow::Result<SemanticCoverageInventory>;

    fn load(&self, file: &DiscoveredSemanticCoverageFile) -> Result<Vec<u8>, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticCoverageProcessingStage {
    Read,
    Decode,
    Parse,
}

impl SemanticCoverageProcessingStage {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Decode => "decode",
            Self::Parse => "parse",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCoverageFileError {
    pub path: PathBuf,
    pub stage: SemanticCoverageProcessingStage,
    pub message: String,
}

#[derive(Debug)]
pub enum SemanticCoverageWorkflowError {
    Source(anyhow::Error),
}

impl std::fmt::Display for SemanticCoverageWorkflowError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(_) => formatter.write_str("semantic coverage source failed"),
        }
    }
}

impl std::error::Error for SemanticCoverageWorkflowError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error.as_ref()),
        }
    }
}

/// Why a `Variable` binding's constant value was not in the value table.
///
/// The variants mirror the checks
/// [`Binding::is_propagatable`](crate::domain::semantics::binding::Binding::is_propagatable)
/// makes, in the same order, so a binding falls into exactly the bucket that
/// explains the first disqualifying fact about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingNonResolutionReason {
    /// `setq`/`incf`/`push`/… reassigns it somewhere in its scope.
    Reassigned,
    /// Its scope contains an unknown macro call, quoted data, or a
    /// reader-conditional region.
    OpaqueScope,
    /// It is declared special, so a lexical read cannot be trusted.
    Special,
    /// The binder gave it no initial form to propagate.
    NoInitialForm,
    /// Every other condition held, but the initial form itself did not fold
    /// to a known value (an unregistered operator, unresolved sub-reference,
    /// or an unfoldable literal like a string or a float).
    InitialFormNotConstant,
}

/// A count of unresolved `Variable` bindings, broken down by the first
/// disqualifying fact about each one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BindingNonResolutionBreakdown {
    reassigned: usize,
    opaque_scope: usize,
    special: usize,
    no_initial_form: usize,
    initial_form_not_constant: usize,
}

impl BindingNonResolutionBreakdown {
    pub const fn reassigned(&self) -> usize {
        self.reassigned
    }

    pub const fn opaque_scope(&self) -> usize {
        self.opaque_scope
    }

    pub const fn special(&self) -> usize {
        self.special
    }

    pub const fn no_initial_form(&self) -> usize {
        self.no_initial_form
    }

    pub const fn initial_form_not_constant(&self) -> usize {
        self.initial_form_not_constant
    }

    pub const fn total(&self) -> usize {
        self.reassigned
            + self.opaque_scope
            + self.special
            + self.no_initial_form
            + self.initial_form_not_constant
    }

    fn record(&mut self, reason: BindingNonResolutionReason) {
        match reason {
            BindingNonResolutionReason::Reassigned => self.reassigned += 1,
            BindingNonResolutionReason::OpaqueScope => self.opaque_scope += 1,
            BindingNonResolutionReason::Special => self.special += 1,
            BindingNonResolutionReason::NoInitialForm => self.no_initial_form += 1,
            BindingNonResolutionReason::InitialFormNotConstant => {
                self.initial_form_not_constant += 1;
            }
        }
    }

    fn merge(&mut self, other: &Self) {
        self.reassigned += other.reassigned;
        self.opaque_scope += other.opaque_scope;
        self.special += other.special;
        self.no_initial_form += other.no_initial_form;
        self.initial_form_not_constant += other.initial_form_not_constant;
    }
}

/// What the semantic layer resolved in one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticCoverageFileReport {
    path: PathBuf,
    variable_binding_count: usize,
    resolved_binding_count: usize,
    list_expression_count: usize,
    known_list_expression_count: usize,
    non_resolution: BindingNonResolutionBreakdown,
}

impl SemanticCoverageFileReport {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every `Variable`-kind binding the binding table recorded. Function,
    /// macro, symbol-macro, and slot bindings are out of scope: only a
    /// variable's *value* is something the value table resolves.
    pub const fn variable_binding_count(&self) -> usize {
        self.variable_binding_count
    }

    /// How many of those the value table gave a constant value.
    pub const fn resolved_binding_count(&self) -> usize {
        self.resolved_binding_count
    }

    /// Every `(...)` list expression in the file, at any nesting depth.
    pub const fn list_expression_count(&self) -> usize {
        self.list_expression_count
    }

    /// How many of those `evaluate_constant` folded to `Known`.
    pub const fn known_list_expression_count(&self) -> usize {
        self.known_list_expression_count
    }

    pub const fn non_resolution(&self) -> &BindingNonResolutionBreakdown {
        &self.non_resolution
    }
}

/// The measurement across every file the source discovered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticCoverageReport {
    files: Vec<SemanticCoverageFileReport>,
    errors: Vec<SemanticCoverageFileError>,
}

impl SemanticCoverageReport {
    pub fn files(&self) -> &[SemanticCoverageFileReport] {
        &self.files
    }

    /// Files the source discovered but could not be read, decoded, or
    /// parsed. Measurement continues over the rest: a corpus with a handful
    /// of unreadable files should not lose every other file's numbers.
    pub fn errors(&self) -> &[SemanticCoverageFileError] {
        &self.errors
    }

    pub fn total_variable_bindings(&self) -> usize {
        self.files
            .iter()
            .map(SemanticCoverageFileReport::variable_binding_count)
            .sum()
    }

    pub fn total_resolved_bindings(&self) -> usize {
        self.files
            .iter()
            .map(SemanticCoverageFileReport::resolved_binding_count)
            .sum()
    }

    pub fn total_list_expressions(&self) -> usize {
        self.files
            .iter()
            .map(SemanticCoverageFileReport::list_expression_count)
            .sum()
    }

    pub fn total_known_list_expressions(&self) -> usize {
        self.files
            .iter()
            .map(SemanticCoverageFileReport::known_list_expression_count)
            .sum()
    }

    pub fn total_non_resolution(&self) -> BindingNonResolutionBreakdown {
        let mut total = BindingNonResolutionBreakdown::default();
        for file in &self.files {
            total.merge(&file.non_resolution);
        }
        total
    }
}

pub fn build_semantic_coverage_report(
    source: &mut impl SemanticCoverageSourcePort,
    request: SemanticCoverageRequest,
) -> Result<SemanticCoverageReport, SemanticCoverageWorkflowError> {
    let inventory = source
        .discover(&request)
        .map_err(SemanticCoverageWorkflowError::Source)?;

    let mut files = Vec::with_capacity(inventory.files.len());
    let mut errors = Vec::new();
    for file in &inventory.files {
        match measure_file(source, file) {
            Ok(report) => files.push(report),
            Err(error) => errors.push(error),
        }
    }

    Ok(SemanticCoverageReport { files, errors })
}

fn measure_file(
    source: &impl SemanticCoverageSourcePort,
    file: &DiscoveredSemanticCoverageFile,
) -> Result<SemanticCoverageFileReport, SemanticCoverageFileError> {
    let bytes = source.load(file).map_err(|message| {
        file_error(&file.path, SemanticCoverageProcessingStage::Read, message)
    })?;
    let text = String::from_utf8(bytes).map_err(|error| {
        file_error(
            &file.path,
            SemanticCoverageProcessingStage::Decode,
            error.to_string(),
        )
    })?;
    let tree = SyntaxTree::parse_with_dialect(&text, Dialect::CommonLisp).map_err(|error| {
        file_error(
            &file.path,
            SemanticCoverageProcessingStage::Parse,
            error.to_string(),
        )
    })?;

    let bindings = build_binding_table(Dialect::CommonLisp, &tree, &text);
    let values = build_value_table(Dialect::CommonLisp, &tree, &bindings);

    let mut report = SemanticCoverageFileReport {
        path: file.path.clone(),
        ..SemanticCoverageFileReport::default()
    };

    for (id, binding) in bindings.bindings() {
        if binding.kind() != BindingKind::Variable {
            continue;
        }
        report.variable_binding_count += 1;
        if values.binding_value(id).is_some() {
            report.resolved_binding_count += 1;
        } else {
            report
                .non_resolution
                .record(classify_non_resolution(binding));
        }
    }

    let document = tree.root_view();
    for_each_subview(&document, |view| {
        if view.kind != ExpressionKind::List {
            return;
        }
        report.list_expression_count += 1;
        if evaluate_constant(Dialect::CommonLisp, view, &bindings, &values).is_known() {
            report.known_list_expression_count += 1;
        }
    });

    Ok(report)
}

/// Classifies an unresolved `Variable` binding by the first disqualifying
/// fact, in the same order `Binding::is_propagatable` checks them.
fn classify_non_resolution(binding: &Binding) -> BindingNonResolutionReason {
    if !binding.assignments().is_empty() {
        BindingNonResolutionReason::Reassigned
    } else if !binding.opacity().is_transparent() {
        BindingNonResolutionReason::OpaqueScope
    } else if !binding.special().is_lexical() {
        BindingNonResolutionReason::Special
    } else if binding.init_form().is_none() {
        BindingNonResolutionReason::NoInitialForm
    } else {
        BindingNonResolutionReason::InitialFormNotConstant
    }
}

fn file_error(
    path: &Path,
    stage: SemanticCoverageProcessingStage,
    message: String,
) -> SemanticCoverageFileError {
    SemanticCoverageFileError {
        path: path.to_path_buf(),
        stage,
        message,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[derive(Default)]
    struct FakeSource {
        files: BTreeMap<PathBuf, Result<Vec<u8>, String>>,
    }

    impl FakeSource {
        fn with_file(mut self, path: &str, text: &str) -> Self {
            self.files
                .insert(PathBuf::from(path), Ok(text.as_bytes().to_vec()));
            self
        }

        fn with_error(mut self, path: &str, message: &str) -> Self {
            self.files
                .insert(PathBuf::from(path), Err(message.to_owned()));
            self
        }
    }

    impl SemanticCoverageSourcePort for FakeSource {
        fn discover(
            &mut self,
            _request: &SemanticCoverageRequest,
        ) -> anyhow::Result<SemanticCoverageInventory> {
            Ok(SemanticCoverageInventory {
                files: self
                    .files
                    .keys()
                    .cloned()
                    .map(|path| DiscoveredSemanticCoverageFile { path })
                    .collect(),
            })
        }

        fn load(&self, file: &DiscoveredSemanticCoverageFile) -> Result<Vec<u8>, String> {
            self.files[&file.path].clone()
        }
    }

    fn report(text: &str) -> SemanticCoverageFileReport {
        let mut source = FakeSource::default().with_file("a.lisp", text);
        let report = build_semantic_coverage_report(
            &mut source,
            SemanticCoverageRequest {
                paths: vec![PathBuf::from("a.lisp")],
            },
        )
        .expect("workflow succeeds");
        assert!(report.errors().is_empty());
        report.files().first().cloned().expect("one file measured")
    }

    #[test]
    fn a_plain_let_binding_resolves_and_propagates_into_its_reference() {
        let report = report("(let ((x 1)) (+ x 1))");
        assert_eq!(report.variable_binding_count(), 1);
        assert_eq!(report.resolved_binding_count(), 1);
        assert_eq!(report.non_resolution().total(), 0);
        // `(+ x 1)` folds through the resolved binding; `(let (...) ...)`
        // does not, since a binder is not itself a fold candidate.
        assert_eq!(report.known_list_expression_count(), 1);
    }

    #[test]
    fn a_reassigned_binding_is_attributed_to_reassignment() {
        let report = report("(let ((x 1)) (setq x 2) x)");
        assert_eq!(report.resolved_binding_count(), 0);
        assert_eq!(report.non_resolution().reassigned(), 1);
        assert_eq!(report.non_resolution().total(), 1);
    }

    #[test]
    fn a_binding_with_no_initial_form_is_attributed_accordingly() {
        let report = report("(let (x) x)");
        assert_eq!(report.resolved_binding_count(), 0);
        assert_eq!(report.non_resolution().no_initial_form(), 1);
    }

    #[test]
    fn a_binding_whose_initial_form_is_not_constant_is_attributed_accordingly() {
        let report = report("(let ((x (read))) x)");
        assert_eq!(report.resolved_binding_count(), 0);
        assert_eq!(report.non_resolution().initial_form_not_constant(), 1);
    }

    #[test]
    fn a_declared_special_binding_is_attributed_accordingly() {
        let report = report("(let ((x 1)) (declare (special x)) x)");
        assert_eq!(report.resolved_binding_count(), 0);
        assert_eq!(report.non_resolution().special(), 1);
    }

    #[test]
    fn an_opaque_scope_is_attributed_accordingly() {
        let report = report("(let ((x 1)) (some-unknown-macro x) x)");
        assert_eq!(report.resolved_binding_count(), 0);
        assert_eq!(report.non_resolution().opaque_scope(), 1);
    }

    #[test]
    fn a_processing_error_is_collected_without_failing_the_whole_report() {
        let mut source = FakeSource::default()
            .with_file("good.lisp", "(let ((x 1)) x)")
            .with_error("bad.lisp", "permission denied");
        let report = build_semantic_coverage_report(
            &mut source,
            SemanticCoverageRequest {
                paths: vec![PathBuf::from("good.lisp"), PathBuf::from("bad.lisp")],
            },
        )
        .expect("workflow succeeds despite a per-file error");
        assert_eq!(report.files().len(), 1);
        assert_eq!(report.errors().len(), 1);
        assert_eq!(
            report.errors()[0].stage,
            SemanticCoverageProcessingStage::Read
        );
    }
}
